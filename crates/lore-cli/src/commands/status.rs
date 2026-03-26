use colored::Colorize;
use lore_core::errors::LoreError;
use lore_git::detector;
use lore_index::store::IndexStore;
use lore_security::paths;

/// Show current index status, storage, and MCP state.
pub async fn run() -> Result<(), LoreError> {
    println!("{}", "lore status".bold());
    println!();

    let cwd = std::env::current_dir().map_err(LoreError::Io)?;
    let git_root = detector::find_git_root(&cwd)?;
    let repo_hash = detector::compute_repo_hash(&git_root);
    let branch = detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());

    println!("Repository: {}", git_root.display());
    println!("Branch:     {}", branch);
    println!("Repo hash:  {}", &repo_hash[..8]);

    let index_path = paths::safe_index_path(&repo_hash, &branch)?;

    if index_path.exists() {
        let store = IndexStore::open(&index_path)?;
        let all_hashes = store.list_all_hashes()?;

        println!();
        println!("{} Index active", "✓".green());
        println!("  Path:    {}", index_path.display());
        println!("  Commits: {}", all_hashes.len());

        // Calculate index size
        if let Ok(metadata) = std::fs::metadata(&index_path) {
            let size_kb = metadata.len() / 1024;
            if size_kb > 1024 {
                println!("  Size:    {:.1} MB", size_kb as f64 / 1024.0);
            } else {
                println!("  Size:    {} KB", size_kb);
            }
        }
    } else {
        println!();
        println!("{} No index found", "⚠".yellow());
        println!("  Run `lore init` to create an index");
    }

    // Check lore home
    let lore_home = paths::lore_home();
    println!();
    println!("Storage:    {}", lore_home.display());

    // List all indexed repos/branches
    let indices_base = lore_home.join("indices");
    if indices_base.exists() {
        let mut total_size = 0u64;
        let mut branch_count = 0u32;
        if let Ok(entries) = std::fs::read_dir(&indices_base) {
            for repo_entry in entries.flatten() {
                if repo_entry.path().is_dir() {
                    if let Ok(files) = std::fs::read_dir(repo_entry.path()) {
                        for file_entry in files.flatten() {
                            let path = file_entry.path();
                            if path.extension().and_then(|e| e.to_str()) == Some("db") {
                                branch_count += 1;
                                if let Ok(m) = std::fs::metadata(&path) {
                                    total_size += m.len();
                                }
                            }
                        }
                    }
                }
            }
        }
        println!("  Indices:  {} branches indexed", branch_count);
        println!("  Total:    {:.1} MB", total_size as f64 / (1024.0 * 1024.0));
    }

    Ok(())
}
