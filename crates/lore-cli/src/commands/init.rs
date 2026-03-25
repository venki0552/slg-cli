use clap::Args;
use colored::Colorize;
use lore_core::errors::LoreError;
use lore_git::{detector, hooks};
use lore_security::paths;
use tracing::info;

#[derive(Args)]
pub struct InitArgs {
    /// Install globally (shell integration + git template)
    #[arg(long)]
    pub global: bool,

    /// Index in background
    #[arg(long)]
    pub background: bool,

    /// Only register MCP, skip hooks
    #[arg(long)]
    pub mcp_only: bool,

    /// Only install git hooks
    #[arg(long)]
    pub hooks_only: bool,

    /// Only install shell integration
    #[arg(long)]
    pub shell_only: bool,

    /// Suppress non-result output
    #[arg(long)]
    pub silent: bool,

    /// Auto-fix all detected issues
    #[arg(long)]
    pub fix_all: bool,
}

pub async fn run(args: InitArgs) -> Result<(), LoreError> {
    // 1. Find git root
    let git_root = detector::find_git_root(&std::env::current_dir().map_err(|e| {
        LoreError::Io(e)
    })?)?;

    if !args.silent {
        println!("{}", "lore init".bold());
        println!("Git root: {}", git_root.display());
    }

    // 2. Compute repo hash
    let repo_hash = detector::compute_repo_hash(&git_root);

    if !args.silent {
        println!("Repo hash: {}", &repo_hash[..8]);
    }

    // 3. Ensure ~/.lore/ directory exists
    let lore_home = paths::lore_home();
    std::fs::create_dir_all(&lore_home).map_err(|e| {
        LoreError::Io(e)
    })?;

    if !args.silent {
        println!("{} Created {}", "✓".green(), lore_home.display());
    }

    // 4. Install hooks (unless --mcp-only)
    if !args.mcp_only && !args.shell_only {
        hooks::install_hooks(&git_root)?;
        if !args.silent {
            println!("{} Git hooks installed", "✓".green());
        }
    }

    // 5. Shell integration (if --global or --shell-only)
    if args.global || args.shell_only {
        let shell = lore_git::shell::detect_shell();
        match lore_git::shell::install_shell_integration(&shell) {
            Ok(_) => {
                if !args.silent {
                    println!("{} Shell integration installed", "✓".green());
                }
            }
            Err(e) => {
                if !args.silent {
                    println!(
                        "{} Shell integration: {}",
                        "⚠".yellow(),
                        e
                    );
                }
            }
        }
    }

    // 6. Index info
    let branch = detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
    let index_path = paths::safe_index_path(&repo_hash, &branch)?;

    if !args.silent {
        println!("\nIndex path: {}", index_path.display());
        println!(
            "{} Run `lore why \"your question\"` to search git history",
            "→".cyan()
        );
    }

    info!("lore init completed for repo {}", &repo_hash[..8]);

    Ok(())
}
