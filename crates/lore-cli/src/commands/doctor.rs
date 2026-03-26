use colored::Colorize;
use lore_core::errors::LoreError;
use lore_git::{detector, hooks, shell};
use lore_security::paths;

/// Run health checks and optionally fix issues.
pub async fn run(fix_all: bool) -> Result<(), LoreError> {
    println!("{}", "lore doctor".bold());
    println!();

    let mut issues = 0u32;
    let mut fixed = 0u32;

    // 1. Check binary version
    let version = env!("CARGO_PKG_VERSION");
    println!("{} lore version: {}", "✓".green(), version);

    // 2. Check lore home directory
    let lore_home = paths::lore_home();
    if lore_home.exists() {
        println!("{} lore home: {}", "✓".green(), lore_home.display());
    } else {
        println!(
            "{} lore home: {} (not found)",
            "✗".red(),
            lore_home.display()
        );
        issues += 1;
    }

    // 3. Check embedding model
    let models_dir = paths::models_dir();
    if models_dir.exists() {
        println!("{} Models directory exists", "✓".green());
    } else {
        println!("{} Models directory missing", "⚠".yellow());
        issues += 1;
    }

    // 4. Check git repo
    let cwd = std::env::current_dir().map_err(LoreError::Io)?;
    match detector::find_git_root(&cwd) {
        Ok(git_root) => {
            println!("{} Git repo: {}", "✓".green(), git_root.display());

            // 5. Check hooks
            if hooks::hooks_installed(&git_root) {
                println!("{} Git hooks installed", "✓".green());
            } else {
                println!("{} Git hooks not installed", "✗".red());
                issues += 1;
                if fix_all {
                    hooks::install_hooks(&git_root)?;
                    println!("  {} Hooks installed", "→".cyan());
                    fixed += 1;
                }
            }

            // 6. Check index
            let repo_hash = detector::compute_repo_hash(&git_root);
            let branch =
                detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
            match paths::safe_index_path(&repo_hash, &branch) {
                Ok(index_path) => {
                    if index_path.exists() {
                        println!("{} Index exists: {}", "✓".green(), index_path.display());
                    } else {
                        println!("{} No index (run `lore init`)", "⚠".yellow());
                        issues += 1;
                    }
                }
                Err(e) => {
                    println!("{} Index path error: {}", "✗".red(), e);
                    issues += 1;
                }
            }
        }
        Err(_) => {
            println!("{} Not in a git repository", "⚠".yellow());
        }
    }

    // 7. Check shell integration
    let detected_shell = shell::detect_shell();
    println!("  Shell: {:?}", detected_shell);

    // Summary
    println!();
    if issues == 0 {
        println!("{}", "All checks passed!".green().bold());
    } else {
        println!(
            "{} issues found{}",
            issues,
            if fix_all {
                format!(", {} fixed", fixed)
            } else {
                String::new()
            }
        );
        if !fix_all && issues > fixed {
            println!("Run `lore doctor --fix-all` to auto-fix");
        }
    }

    Ok(())
}
