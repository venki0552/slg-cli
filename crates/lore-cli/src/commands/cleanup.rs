use clap::Args;
use colored::Colorize;
use lore_core::errors::LoreError;
use lore_security::paths;

#[derive(Args)]
pub struct CleanupArgs {
    /// Remove indices older than this many days (default: 7)
    #[arg(long, default_value = "7")]
    pub older_than: u64,

    /// Dry run: show what would be deleted without deleting
    #[arg(long)]
    pub dry_run: bool,
}

/// Remove stale branch indices to reclaim disk space.
pub async fn run(args: CleanupArgs) -> Result<(), LoreError> {
    let lore_home = paths::lore_home();
    let indices_base = lore_home.join("indices");

    if !indices_base.exists() {
        println!("No indices found.");
        return Ok(());
    }

    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(args.older_than * 24 * 60 * 60);

    let mut removed = 0u32;
    let mut freed_bytes = 0u64;

    if let Ok(repos) = std::fs::read_dir(&indices_base) {
        for repo_entry in repos.flatten() {
            if !repo_entry.path().is_dir() {
                continue;
            }

            if let Ok(files) = std::fs::read_dir(repo_entry.path()) {
                for file_entry in files.flatten() {
                    let file_path = file_entry.path();
                    // Index files are stored as {branch}.db directly (not in subdirectories)
                    if file_path.extension().and_then(|e| e.to_str()) != Some("db") {
                        continue;
                    }

                    let should_remove = match std::fs::metadata(&file_path) {
                        Ok(m) => match m.modified() {
                            Ok(modified) => modified < cutoff,
                            Err(_) => false,
                        },
                        Err(_) => false,
                    };

                    if should_remove {
                        let file_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);

                        if args.dry_run {
                            println!(
                                "  {} Would remove: {} ({:.1} KB)",
                                "→".cyan(),
                                file_path.display(),
                                file_size as f64 / 1024.0
                            );
                        } else {
                            if let Err(e) = std::fs::remove_file(&file_path) {
                                eprintln!("  {} Failed to remove {}: {}", "✗".red(), file_path.display(), e);
                                continue;
                            }
                            println!(
                                "  {} Removed: {} ({:.1} KB)",
                                "✓".green(),
                                file_path.display(),
                                file_size as f64 / 1024.0
                            );
                        }

                        freed_bytes += file_size;
                        removed += 1;
                    }
                }
            }

            // Clean up empty repo directories
            if !args.dry_run {
                let repo_path = repo_entry.path();
                if let Ok(mut entries) = std::fs::read_dir(&repo_path) {
                    if entries.next().is_none() {
                        let _ = std::fs::remove_dir(&repo_path);
                    }
                }
            }
        }
    }

    if removed == 0 {
        println!("No stale indices found (older than {} days).", args.older_than);
    } else {
        let action = if args.dry_run { "Would remove" } else { "Removed" };
        println!(
            "\n{} {} indices, freeing {:.1} MB",
            action,
            removed,
            freed_bytes as f64 / (1024.0 * 1024.0)
        );
    }

    Ok(())
}
