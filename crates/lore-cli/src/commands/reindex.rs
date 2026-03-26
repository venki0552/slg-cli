use clap::Args;
use lore_core::errors::LoreError;
use lore_git::{delta, detector};
use lore_index::embedder::Embedder;
use lore_index::store::IndexStore;
use lore_security::paths;
use tracing::info;

#[derive(Args)]
pub struct ReindexArgs {
    /// Only index new commits since last indexed
    #[arg(long)]
    pub delta_only: bool,

    /// Run in background
    #[arg(long)]
    pub background: bool,

    /// Suppress non-result output
    #[arg(long)]
    pub silent: bool,
}

/// Reindex the current branch (delta-only for speed, used by hooks).
pub async fn run(args: ReindexArgs) -> Result<(), LoreError> {
    let cwd = std::env::current_dir().map_err(LoreError::Io)?;
    let git_root = detector::find_git_root(&cwd)?;
    let repo_hash = detector::compute_repo_hash(&git_root);
    let branch = detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
    let index_path = paths::safe_index_path(&repo_hash, &branch)?;

    if !index_path.exists() {
        // No existing index — fall back to full index
        if !args.silent {
            eprintln!("No existing index, performing full index...");
        }
        return super::index::run(super::index::IndexArgs {
            background: args.background,
            silent: args.silent,
        })
        .await;
    }

    let store = IndexStore::open(&index_path)?;
    let embedder = Embedder::new()?;

    // Get commits not yet indexed
    let _existing_hashes = store.list_all_hashes()?;
    let base_branch = detector::detect_base_branch(&git_root);
    let repo = git2::Repository::open(&git_root)
        .map_err(|e| LoreError::Git(format!("Failed to open repo: {}", e)))?;
    let new_commits = delta::get_delta_commits(&repo, &base_branch, &branch)?;

    if new_commits.is_empty() {
        if !args.silent {
            eprintln!("Already up to date.");
        }
        return Ok(());
    }

    if !args.silent {
        eprintln!(
            "Reindexing {} new commits on '{}'...",
            new_commits.len(),
            branch
        );
    }

    // new_commits is Vec<String> of commit hashes — we need to build CommitDoc for each
    let sanitizer = lore_security::sanitizer::CommitSanitizer;
    let redactor = lore_security::redactor::SecretRedactor::new();

    let mut indexed = 0u64;
    for hash in &new_commits {
        if store.commit_exists(hash)? {
            continue;
        }

        let oid = git2::Oid::from_str(hash)
            .map_err(|e| LoreError::Git(format!("Invalid hash: {}", e)))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| LoreError::Git(format!("Commit not found: {}", e)))?;
        let raw_doc = lore_git::ingestion::build_raw_commit_doc(&repo, &commit, &branch)?;
        let mut doc = sanitizer.sanitize(raw_doc);
        let (redacted_diff, secret_count) = redactor.redact(&doc.diff_summary);
        if secret_count > 0 {
            doc.diff_summary = redacted_diff;
            doc.secrets_redacted += secret_count;
        }

        let embedding = embedder.embed_commit(&doc)?;
        store.store_commit(&doc, &embedding)?;
        lore_index::bm25::BM25Index::index_commit(&store, &doc)?;

        indexed += 1;
    }

    store.update_metadata(&repo_hash, &branch, &base_branch)?;

    if !args.silent {
        eprintln!("Reindexed {} commits.", indexed);
    }

    info!(
        "Delta reindex: {} commits for {}/{}",
        indexed,
        &repo_hash[..8],
        branch
    );

    Ok(())
}
