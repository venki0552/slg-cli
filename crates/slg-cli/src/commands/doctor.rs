use colored::Colorize;
use slg_core::errors::SlgError;
use slg_git::{detector, hooks, shell};
use slg_security::paths;

/// Run health checks and optionally fix issues.
pub async fn run(fix_all: bool) -> Result<(), SlgError> {
    println!("{}", "slg doctor".bold());
    println!();

    let mut issues = 0u32;
    let mut fixed = 0u32;

    // 1. Check binary version
    let version = env!("CARGO_PKG_VERSION");
    println!("{} slg version: {}", "✓".green(), version);

    // 2. Check slg home directory
    let slg_home = paths::slg_home();
    if slg_home.exists() {
        println!("{} slg home: {}", "✓".green(), slg_home.display());
    } else {
        println!("{} slg home: {} (not found)", "✗".red(), slg_home.display());
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
    let cwd = std::env::current_dir().map_err(SlgError::Io)?;
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
                        println!("{} No index (run `slg init`)", "⚠".yellow());
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
            println!("Run `slg doctor --fix-all` to auto-fix");
        }
    }

    Ok(())
}
