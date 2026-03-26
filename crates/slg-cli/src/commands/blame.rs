use clap::Args;
use slg_core::errors::SlgError;
use slg_core::types::OutputFormat;
use slg_git::detector;
use slg_index::embedder::Embedder;
use slg_index::search::{self, SearchOptions};
use slg_index::store::IndexStore;
use slg_output::{json, text, xml};
use slg_security::output_guard::OutputGuard;
use slg_security::paths;
use std::time::Instant;

#[derive(Args)]
pub struct BlameArgs {
    /// File path to analyze
    pub file: String,

    /// Function name to focus on
    #[arg(long, name = "fn")]
    pub func: Option<String>,

    /// Include risk scores
    #[arg(long)]
    pub risk: bool,
}

/// Semantic blame: find who understands a file and why.
pub async fn run(
    args: BlameArgs,
    format: OutputFormat,
    max_tokens: Option<usize>,
) -> Result<(), SlgError> {
    let cwd = std::env::current_dir().map_err(SlgError::Io)?;
    let git_root = detector::find_git_root(&cwd)?;
    let repo_hash = detector::compute_repo_hash(&git_root);
    let branch = detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
    let index_path = paths::safe_index_path(&repo_hash, &branch)?;

    if !index_path.exists() {
        return Err(SlgError::NoIndex);
    }

    let store = IndexStore::open(&index_path)?;
    let embedder = Embedder::new()?;

    // Build query from file + optional function name
    let query = if let Some(ref func) = args.func {
        format!("changes to function {} in file {}", func, args.file)
    } else {
        format!("changes to file {}", args.file)
    };

    let options = SearchOptions {
        limit: 10,
        since: None,
        until: None,
        author: None,
        module: Some(args.file.clone()),
        max_tokens: max_tokens.unwrap_or(4096),
        enable_reranker: false,
        format,
    };

    let start = Instant::now();
    let results = search::search(&query, &store, &embedder, &options).await?;
    let latency_ms = start.elapsed().as_millis() as u64;

    let output = match format {
        OutputFormat::Xml => xml::format_xml(&results, &query, latency_ms),
        OutputFormat::Json => json::format_json(&results, &query, latency_ms),
        OutputFormat::Text => text::format_text(&results, &query, latency_ms),
    };

    let guard = OutputGuard::new();
    let safe_output = guard.check_and_sanitize(&output, 50_000);
    println!("{}", safe_output);

    Ok(())
}
