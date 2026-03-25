use clap::Args;
use lore_core::errors::LoreError;
use lore_core::types::OutputFormat;
use lore_git::detector;
use lore_index::embedder::Embedder;
use lore_index::search::{self, SearchOptions};
use lore_index::store::IndexStore;
use lore_output::{json, text, xml};
use lore_security::output_guard::OutputGuard;
use lore_security::paths;
use std::time::Instant;
use tracing::info;

#[derive(Args)]
pub struct WhyArgs {
    /// Semantic search query
    pub query: String,

    /// Number of results (default 3, max 10)
    #[arg(long, default_value = "3")]
    pub limit: u32,

    /// Filter commits after this date (ISO format)
    #[arg(long)]
    pub since: Option<String>,

    /// Filter by author name
    #[arg(long)]
    pub author: Option<String>,

    /// Filter to files under this path
    #[arg(long)]
    pub module: Option<String>,

    /// Maximum response tokens
    #[arg(long)]
    pub max_tokens: Option<usize>,

    /// Enable reranking
    #[arg(long)]
    pub rerank: bool,
}

pub async fn run(args: WhyArgs, format: OutputFormat, global_max_tokens: Option<usize>) -> Result<(), LoreError> {
    // Validate query
    if args.query.len() > 500 {
        return Err(LoreError::QueryTooLong);
    }

    // Find repo
    let cwd = std::env::current_dir().map_err(|e| LoreError::Io(e))?;
    let git_root = detector::find_git_root(&cwd)?;

    let repo_hash = detector::compute_repo_hash(&git_root);
    let branch = detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
    let index_path = paths::safe_index_path(&repo_hash, &branch)?;

    if !index_path.exists() {
        return Err(LoreError::NoIndex);
    }

    // Open store + embedder
    let store = IndexStore::open(&index_path)?;
    let embedder = Embedder::new()?;

    // Parse since date
    let since_ts = args.since.as_ref().and_then(|s| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|dt| dt.and_utc().timestamp())
    });

    let max_tokens = args
        .max_tokens
        .or(global_max_tokens)
        .unwrap_or(4096);

    let options = SearchOptions {
        limit: args.limit.min(10),
        since: since_ts,
        until: None,
        author: args.author,
        module: args.module,
        max_tokens,
        enable_reranker: args.rerank,
        format,
    };

    let start = Instant::now();
    let results = search::search(&args.query, &store, &embedder, &options).await?;
    let latency_ms = start.elapsed().as_millis() as u64;

    info!(
        "Search completed: {} results in {}ms",
        results.len(),
        latency_ms
    );

    // Format output
    let output = match format {
        OutputFormat::Xml => xml::format_xml(&results, &args.query, latency_ms),
        OutputFormat::Json => json::format_json(&results, &args.query, latency_ms),
        OutputFormat::Text => text::format_text(&results, &args.query, latency_ms),
    };

    // Run output guard
    let guard = OutputGuard::new();
    let safe_output = guard.check_and_sanitize(&output, 50_000);

    println!("{}", safe_output);

    Ok(())
}
