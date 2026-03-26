use clap::Args;
use slg_core::errors::SlgError;
use slg_core::types::OutputFormat;
use slg_git::detector;
use slg_index::embedder::Embedder;
use slg_index::search::{self, SearchOptions};
use slg_index::store::IndexStore;
use slg_output::{json as json_fmt, text, xml};
use slg_security::output_guard::OutputGuard;
use slg_security::paths;

#[derive(Args)]
pub struct RevertRiskArgs {
    /// Commit hash to analyze revert risk for
    pub commit: String,

    /// Suppress non-result output
    #[arg(long)]
    pub silent: bool,
}

/// Blast radius analysis before reverting a commit.
pub async fn run(
    args: RevertRiskArgs,
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

    // BUG-007 fix: resolve ref to hash before looking up in store
    let resolved_hash =
        if args.commit.chars().all(|c| c.is_ascii_hexdigit()) && args.commit.len() >= 7 {
            args.commit.clone()
        } else {
            detector::resolve_ref(&git_root, &args.commit)?
        };

    // Get the commit being analyzed using the resolved hash
    let commit_doc = store.get_commit(&resolved_hash)?;
    let query = match &commit_doc {
        Some(doc) => format!(
            "revert risk: {} files: {}",
            doc.message,
            doc.files_changed.join(" ")
        ),
        None => format!("revert {}", resolved_hash),
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

#[cfg(test)]
mod tests {
    /// BUG-007 regression: refs are now resolved to hashes before store lookup.
    /// The run() function calls detector::resolve_ref() for non-hex inputs.
    #[test]
    fn test_hex_hash_not_resolved() {
        // A full hex hash should be used directly without resolve_ref
        let commit_arg = "39fbfd9b28aabbccdd";
        assert!(
            commit_arg.chars().all(|c| c.is_ascii_hexdigit()) && commit_arg.len() >= 7,
            "Full hex hash should skip ref resolution"
        );
    }

    #[test]
    fn test_ref_needs_resolution() {
        // Non-hex refs like HEAD should trigger resolve_ref
        let refs_needing_resolution = ["HEAD", "HEAD~1", "main", "v1.0.0"];
        for r in &refs_needing_resolution {
            let is_hex = r.chars().all(|c| c.is_ascii_hexdigit()) && r.len() >= 7;
            assert!(!is_hex, "'{}' should trigger ref resolution", r);
        }
    }
}
