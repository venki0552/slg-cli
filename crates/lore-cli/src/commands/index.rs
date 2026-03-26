use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use lore_core::errors::LoreError;
use lore_git::{detector, ingestion};
use lore_index::embedder::Embedder;
use lore_index::store::IndexStore;
use lore_security::paths;
use lore_security::redactor::SecretRedactor;
use lore_security::sanitizer::CommitSanitizer;
use tracing::info;

#[derive(Args)]
pub struct IndexArgs {
    /// Index in background
    #[arg(long)]
    pub background: bool,

    /// Suppress non-result output
    #[arg(long)]
    pub silent: bool,
}

/// Run full indexing of the current branch.
pub async fn run(args: IndexArgs) -> Result<(), LoreError> {
    let cwd = std::env::current_dir().map_err(LoreError::Io)?;
    let git_root = detector::find_git_root(&cwd)?;
    let repo_hash = detector::compute_repo_hash(&git_root);
    let branch = detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
    let index_path = paths::safe_index_path(&repo_hash, &branch)?;

    if !args.silent {
        eprintln!("Indexing branch '{}' at {}", branch, git_root.display());
    }

    // Create store and schema
    let store = IndexStore::open(&index_path)?;
    store.create_schema()?;

    // Load embedder
    let embedder = Embedder::new()?;

    // Ingest all commits
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let git_root_clone = git_root.clone();
    let branch_clone = branch.clone();

    let ingest_handle = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let sanitizer = CommitSanitizer;
        let redactor = SecretRedactor::new();
        rt.block_on(ingestion::index_full_branch(
            &git_root_clone,
            &branch_clone,
            &sanitizer,
            &redactor,
            tx,
        ))
    });

    let pb = if !args.silent {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg} [{pos}]")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        pb.set_message("Indexing commits");
        Some(pb)
    } else {
        None
    };

    let mut indexed = 0u64;
    while let Some(doc) = rx.recv().await {
        if store.commit_exists(&doc.hash)? {
            continue;
        }

        let embedding = embedder.embed_commit(&doc)?;
        store.store_commit(&doc, &embedding)?;

        // Index BM25 terms
        lore_index::bm25::BM25Index::index_commit(&store, &doc)?;

        indexed += 1;
        if let Some(ref pb) = pb {
            pb.set_position(indexed);
        }
    }

    // Wait for ingestion to finish
    match ingest_handle.await {
        Ok(Ok(_count)) => {}
        Ok(Err(e)) => {
            if !args.silent {
                eprintln!("Warning: ingestion ended with error: {}", e);
            }
        }
        Err(e) => {
            if !args.silent {
                eprintln!("Warning: ingestion task panicked: {}", e);
            }
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message(format!("Indexed {} commits", indexed));
    }

    // Update metadata
    store.update_metadata(&repo_hash, &branch, &detector::detect_base_branch(&git_root))?;

    if !args.silent {
        eprintln!("Index stored at: {}", index_path.display());
    }

    info!("Indexed {} commits for {}/{}", indexed, &repo_hash[..8], branch);

    Ok(())
}
