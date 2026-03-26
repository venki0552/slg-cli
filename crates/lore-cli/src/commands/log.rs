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

#[derive(Args)]
pub struct LogArgs {
    /// Search query
    pub query: String,

    /// Filter commits after this date (ISO format)
    #[arg(long)]
    pub since: Option<String>,

    /// Group results by intent
    #[arg(long)]
    pub by_intent: bool,

    /// Number of results
    #[arg(long, default_value = "10")]
    pub limit: u32,
}

/// Intent-grouped semantic git log.
pub async fn run(args: LogArgs, format: OutputFormat, max_tokens: Option<usize>) -> Result<(), LoreError> {
    let cwd = std::env::current_dir().map_err(LoreError::Io)?;
    let git_root = detector::find_git_root(&cwd)?;
    let repo_hash = detector::compute_repo_hash(&git_root);
    let branch = detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
    let index_path = paths::safe_index_path(&repo_hash, &branch)?;

    if !index_path.exists() {
        return Err(LoreError::NoIndex);
    }

    let store = IndexStore::open(&index_path)?;
    let embedder = Embedder::new()?;

    let since_ts = args.since.as_ref().and_then(|s| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|dt| dt.and_utc().timestamp())
    });

    let options = SearchOptions {
        limit: args.limit.min(20),
        since: since_ts,
        until: None,
        author: None,
        module: None,
        max_tokens: max_tokens.unwrap_or(8192),
        enable_reranker: false,
        format,
    };

    let start = Instant::now();
    let results = search::search(&args.query, &store, &embedder, &options).await?;
    let latency_ms = start.elapsed().as_millis() as u64;

    let output = match format {
        OutputFormat::Xml => xml::format_xml(&results, &args.query, latency_ms),
        OutputFormat::Json => json::format_json(&results, &args.query, latency_ms),
        OutputFormat::Text => text::format_text(&results, &args.query, latency_ms),
    };

    let guard = OutputGuard::new();
    let safe_output = guard.check_and_sanitize(&output, 50_000);
    println!("{}", safe_output);

    Ok(())
}
