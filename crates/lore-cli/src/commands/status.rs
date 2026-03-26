use colored::Colorize;
use lore_core::errors::LoreError;
use lore_core::types::OutputFormat;
use lore_git::detector;
use lore_index::store::IndexStore;
use lore_security::paths;

/// Show current index status, storage, and MCP state.
pub async fn run(format: OutputFormat) -> Result<(), LoreError> {
    let cwd = std::env::current_dir().map_err(LoreError::Io)?;
    let git_root = detector::find_git_root(&cwd)?;
    let repo_hash = detector::compute_repo_hash(&git_root);
    let branch = detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());

    let index_path = paths::safe_index_path(&repo_hash, &branch)?;

    let (indexed, commit_count, index_size_bytes) = if index_path.exists() {
        let store = IndexStore::open(&index_path)?;
        let all_hashes = store.list_all_hashes()?;
        let size = std::fs::metadata(&index_path).map(|m| m.len()).unwrap_or(0);
        (true, all_hashes.len(), size)
    } else {
        (false, 0, 0u64)
    };

    // Collect storage info
    let lore_home = paths::lore_home();
    let indices_base = lore_home.join("indices");
    let (total_branches, total_size_bytes) = if indices_base.exists() {
        let mut branches = 0u32;
        let mut size = 0u64;
        if let Ok(entries) = std::fs::read_dir(&indices_base) {
            for repo_entry in entries.flatten() {
                if repo_entry.path().is_dir() {
                    if let Ok(files) = std::fs::read_dir(repo_entry.path()) {
                        for file_entry in files.flatten() {
                            let path = file_entry.path();
                            if path.extension().and_then(|e| e.to_str()) == Some("db") {
                                branches += 1;
                                if let Ok(m) = std::fs::metadata(&path) {
                                    size += m.len();
                                }
                            }
                        }
                    }
                }
            }
        }
        (branches, size)
    } else {
        (0, 0)
    };

    match format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "repository": git_root.display().to_string(),
                "branch": branch,
                "repo_hash": &repo_hash[..8],
                "indexed": indexed,
                "index_path": index_path.display().to_string(),
                "commit_count": commit_count,
                "index_size_bytes": index_size_bytes,
                "storage": {
                    "path": lore_home.display().to_string(),
                    "total_branches": total_branches,
                    "total_size_bytes": total_size_bytes,
                }
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&json).unwrap_or_default()
            );
        }
        OutputFormat::Xml => {
            println!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
            println!("<lore_status>");
            println!("  <repository>{}</repository>", git_root.display());
            println!("  <branch>{}</branch>", branch);
            println!("  <repo_hash>{}</repo_hash>", &repo_hash[..8]);
            println!("  <indexed>{}</indexed>", indexed);
            println!("  <commit_count>{}</commit_count>", commit_count);
            println!(
                "  <index_size_bytes>{}</index_size_bytes>",
                index_size_bytes
            );
            println!("  <storage path=\"{}\">", lore_home.display());
            println!("    <total_branches>{}</total_branches>", total_branches);
            println!(
                "    <total_size_bytes>{}</total_size_bytes>",
                total_size_bytes
            );
            println!("  </storage>");
            println!("</lore_status>");
        }
        OutputFormat::Text => {
            println!("{}", "lore status".bold());
            println!();
            println!("Repository: {}", git_root.display());
            println!("Branch:     {}", branch);
            println!("Repo hash:  {}", &repo_hash[..8]);

            if indexed {
                println!();
                println!("{} Index active", "✓".green());
                println!("  Path:    {}", index_path.display());
                println!("  Commits: {}", commit_count);
                let size_kb = index_size_bytes / 1024;
                if size_kb > 1024 {
                    println!("  Size:    {:.1} MB", size_kb as f64 / 1024.0);
                } else {
                    println!("  Size:    {} KB", size_kb);
                }
            } else {
                println!();
                println!("{} No index found", "⚠".yellow());
                println!("  Run `lore init` to create an index");
            }

            println!();
            println!("Storage:    {}", lore_home.display());
            println!("  Indices:  {} branches indexed", total_branches);
            println!(
                "  Total:    {:.1} MB",
                total_size_bytes as f64 / (1024.0 * 1024.0)
            );
        }
    }

    Ok(())
}
