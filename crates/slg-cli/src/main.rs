mod commands;

use clap::{Parser, Subcommand, ValueEnum};
use slg_core::errors::SlgError;
use slg_core::types::OutputFormat;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "slg",
    version,
    about = "Semantic git intelligence for LLM agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format
    #[arg(long, global = true, default_value = "text", value_enum)]
    format: CliFormat,

    /// Maximum response tokens
    #[arg(long, global = true)]
    max_tokens: Option<usize>,

    /// Suppress all non-result output (for hooks)
    #[arg(long, global = true)]
    silent: bool,
}

#[derive(Clone, ValueEnum)]
enum CliFormat {
    Text,
    Xml,
    Json,
}

impl From<CliFormat> for OutputFormat {
    fn from(f: CliFormat) -> OutputFormat {
        match f {
            CliFormat::Text => OutputFormat::Text,
            CliFormat::Xml => OutputFormat::Xml,
            CliFormat::Json => OutputFormat::Json,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize slg for this repository
    Init(commands::init::InitArgs),

    /// Explicitly index current branch (full)
    Index(commands::index::IndexArgs),

    /// Reindex (delta-only by default, used by hooks)
    Reindex(commands::reindex::ReindexArgs),

    /// Search git history semantically
    Why(commands::why::WhyArgs),

    /// Semantic ownership — who understands a file and why
    Blame(commands::blame::BlameArgs),

    /// Find which commit likely introduced a bug
    Bisect(commands::bisect::BisectArgs),

    /// Intent-grouped semantic git log
    Log(commands::log::LogArgs),

    /// Intent-level diff between two refs
    Diff(commands::diff::DiffArgs),

    /// Blast radius analysis before reverting
    #[command(name = "revert-risk")]
    RevertRisk(commands::revert_risk::RevertRiskArgs),

    /// Show what is indexed, storage, MCP state
    Status,

    /// Remove stale branch indices
    Cleanup(commands::cleanup::CleanupArgs),

    /// Run health checks and optionally fix issues
    Doctor {
        /// Auto-fix all detected issues
        #[arg(long)]
        fix_all: bool,
    },

    /// Start the MCP server (stdio JSON-RPC)
    Serve,

    /// Start the MCP server (alias for serve)
    Mcp,

    /// Manually trigger reindex (for CI use)
    Sync(commands::sync::SyncArgs),

    // --- Internal commands (used by hooks and plugin) ---
    /// Machine-readable health check (for plugin status bar)
    #[command(name = "_health", hide = true)]
    Health,

    /// Print stable repo hash
    #[command(name = "_repo-hash", hide = true)]
    RepoHash,

    /// Index a single commit by hash
    #[command(name = "_index-commit", hide = true)]
    IndexCommit {
        /// Commit hash to index
        hash: String,
    },

    /// Print index path for current repo
    #[command(name = "_index-path", hide = true)]
    IndexPath,
}

#[tokio::main]
async fn main() {
    // Initialize tracing from SLG_LOG env var
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("SLG_LOG").unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let format: OutputFormat = cli.format.into();

    let result = match cli.command {
        Commands::Init(args) => commands::init::run(args).await,
        Commands::Index(args) => commands::index::run(args).await,
        Commands::Reindex(args) => commands::reindex::run(args).await,
        Commands::Why(args) => commands::why::run(args, format, cli.max_tokens).await,
        Commands::Blame(args) => commands::blame::run(args, format, cli.max_tokens).await,
        Commands::Bisect(args) => commands::bisect::run(args, format, cli.max_tokens).await,
        Commands::Log(args) => commands::log::run(args, format, cli.max_tokens).await,
        Commands::Diff(args) => commands::diff::run(args, format, cli.max_tokens).await,
        Commands::RevertRisk(args) => {
            commands::revert_risk::run(args, format, cli.max_tokens).await
        }
        Commands::Status => commands::status::run(format).await,
        Commands::Cleanup(args) => commands::cleanup::run(args).await,
        Commands::Doctor { fix_all } => commands::doctor::run(fix_all).await,
        Commands::Serve | Commands::Mcp => commands::serve::run().await,
        Commands::Sync(args) => commands::sync::run(args).await,
        Commands::Health => run_health().await,
        Commands::RepoHash => run_repo_hash(),
        Commands::IndexCommit { hash } => run_index_commit(&hash).await,
        Commands::IndexPath => run_index_path(),
    };

    if let Err(e) = result {
        if !cli.silent {
            eprintln!("Error: {}", e);
        }
        std::process::exit(1);
    }
}

/// Machine-readable health JSON for plugin status bar.
async fn run_health() -> Result<(), SlgError> {
    let cwd = std::env::current_dir().map_err(SlgError::Io)?;

    let (git_root, branch, indexed, size_kb) = match slg_git::detector::find_git_root(&cwd) {
        Ok(root) => {
            let branch = slg_git::detector::get_current_branch(&root)
                .unwrap_or_else(|_| "main".to_string());
            let repo_hash = slg_git::detector::compute_repo_hash(&root);
            let index_path = slg_security::paths::safe_index_path(&repo_hash, &branch)?;
            let (indexed, size_kb) = if index_path.exists() {
                let size = std::fs::metadata(&index_path)
                    .map(|m| m.len() / 1024)
                    .unwrap_or(0);
                (true, size)
            } else {
                (false, 0)
            };
            (Some(root), branch, indexed, size_kb)
        }
        Err(_) => (None, String::new(), false, 0),
    };

    let health = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "git_root": git_root.map(|p| p.display().to_string()),
        "branch": branch,
        "indexed": indexed,
        "size_kb": size_kb,
    });

    println!("{}", serde_json::to_string(&health).unwrap_or_default());
    Ok(())
}

/// Print stable repo hash.
fn run_repo_hash() -> Result<(), SlgError> {
    let cwd = std::env::current_dir().map_err(SlgError::Io)?;
    let git_root = slg_git::detector::find_git_root(&cwd)?;
    let hash = slg_git::detector::compute_repo_hash(&git_root);
    println!("{}", hash);
    Ok(())
}

/// Index a single commit by hash (used by post-commit hook).
async fn run_index_commit(hash: &str) -> Result<(), SlgError> {
    let cwd = std::env::current_dir().map_err(SlgError::Io)?;
    let git_root = slg_git::detector::find_git_root(&cwd)?;
    let repo_hash = slg_git::detector::compute_repo_hash(&git_root);
    let branch =
        slg_git::detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
    let index_path = slg_security::paths::safe_index_path(&repo_hash, &branch)?;

    if !index_path.exists() {
        return Ok(()); // Silently skip if no index yet
    }

    let store = slg_index::store::IndexStore::open(&index_path)?;

    if store.commit_exists(hash)? {
        return Ok(()); // Already indexed
    }

    let repo = git2::Repository::open(&git_root).map_err(|e| SlgError::Git(e.to_string()))?;

    let oid =
        git2::Oid::from_str(hash).map_err(|e| SlgError::Git(format!("Invalid hash: {}", e)))?;

    let commit = repo
        .find_commit(oid)
        .map_err(|e| SlgError::Git(format!("Commit not found: {}", e)))?;

    let doc = slg_git::ingestion::build_raw_commit_doc(&repo, &commit, &branch)?;

    let embedder = slg_index::embedder::Embedder::new()?;
    let embedding = embedder.embed_commit(&doc)?;
    store.store_commit(&doc, &embedding)?;

    slg_index::bm25::BM25Index::index_commit(&store, &doc)?;

    Ok(())
}

/// Print index path for current repo.
fn run_index_path() -> Result<(), SlgError> {
    let cwd = std::env::current_dir().map_err(SlgError::Io)?;
    let git_root = slg_git::detector::find_git_root(&cwd)?;
    let repo_hash = slg_git::detector::compute_repo_hash(&git_root);
    let branch =
        slg_git::detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
    let index_path = slg_security::paths::safe_index_path(&repo_hash, &branch)?;
    println!("{}", index_path.display());
    Ok(())
}
