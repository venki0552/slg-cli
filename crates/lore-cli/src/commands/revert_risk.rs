use clap::Args;
use lore_core::errors::LoreError;
use lore_core::types::OutputFormat;
use lore_git::detector;
use lore_index::embedder::Embedder;
use lore_index::search::{self, SearchOptions};
use lore_index::store::IndexStore;
use lore_output::{text, xml, json as json_fmt};
use lore_security::output_guard::OutputGuard;
use lore_security::paths;

#[derive(Args)]
pub struct RevertRiskArgs {
    /// Commit hash to analyze revert risk for
    pub commit: String,

    /// Suppress non-result output
    #[arg(long)]
    pub silent: bool,
}

/// Blast radius analysis before reverting a commit.
pub async fn run(args: RevertRiskArgs, format: OutputFormat, max_tokens: Option<usize>) -> Result<(), LoreError> {
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

    // Get the commit being analyzed
    let commit_doc = store.get_commit(&args.commit)?;
    let query = match &commit_doc {
        Some(doc) => format!("revert risk: {} files: {}", doc.message, doc.files_changed.join(" ")),
        None => format!("revert {}", args.commit),
    };

    let options = SearchOptions {
        limit: 10,
        since: None,
        until: None,
        author: None,
        module: None,
        max_tokens: max_tokens.unwrap_or(4096),
        enable_reranker: false,
        format,
    };

    let start = std::time::Instant::now();
    let results = search::search(&query, &store, &embedder, &options).await?;
    let latency_ms = start.elapsed().as_millis() as u64;

    let guard = OutputGuard::new();
    let output = match format {
        OutputFormat::Json => json_fmt::format_json(&results, &query, latency_ms),
        OutputFormat::Xml => xml::format_xml(&results, &query, latency_ms),
        OutputFormat::Text => text::format_text(&results, &query, latency_ms),
    };
    let safe = guard.check_and_sanitize(&output, 50_000);
    print!("{}", safe);

    Ok(())
}
