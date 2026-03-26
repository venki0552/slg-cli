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

            if let Ok(branches) = std::fs::read_dir(repo_entry.path()) {
                for branch_entry in branches.flatten() {
                    let branch_path = branch_entry.path();
                    if !branch_path.is_dir() {
                        continue;
                    }

                    // Check last modified time of the index db
                    let db_path = branch_path.join("lore.db");
                    let should_remove = if db_path.exists() {
                        match std::fs::metadata(&db_path) {
                            Ok(m) => match m.modified() {
                                Ok(modified) => modified < cutoff,
                                Err(_) => false,
                            },
                            Err(_) => false,
                        }
                    } else {
                        // Directory exists but no db — clean it up
                        true
                    };

                    if should_remove {
                        let dir_size = dir_size(&branch_path);

                        if args.dry_run {
                            println!(
                                "  {} Would remove: {} ({:.1} KB)",
                                "→".cyan(),
                                branch_path.display(),
                                dir_size as f64 / 1024.0
                            );
                        } else {
                            if let Err(e) = std::fs::remove_dir_all(&branch_path) {
                                eprintln!("  {} Failed to remove {}: {}", "✗".red(), branch_path.display(), e);
                                continue;
                            }
                            println!(
                                "  {} Removed: {} ({:.1} KB)",
                                "✓".green(),
                                branch_path.display(),
                                dir_size as f64 / 1024.0
                            );
                        }

                        freed_bytes += dir_size;
                        removed += 1;
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

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Ok(m) = std::fs::metadata(&p) {
                    total += m.len();
                }
            } else if p.is_dir() {
                total += dir_size(&p);
            }
        }
    }
    total
}
