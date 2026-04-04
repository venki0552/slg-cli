use serde_json::{json, Value};
use slg_core::errors::SlgError;
use slg_core::types::CommitDoc;
use slg_git::{detector, ingestion};
use slg_index::embedder::Embedder;
use slg_index::store::IndexStore;
use slg_security::paths;
use slg_security::redactor::SecretRedactor;
use slg_security::sanitizer::CommitSanitizer;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Once;
use tracing::{debug, error, info};

static INDEXING_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static INDEXING_COMMITS_DONE: AtomicU64 = AtomicU64::new(0);
static INDEXING_FINISHED: AtomicBool = AtomicBool::new(false);
static INDEXING_ERROR: AtomicBool = AtomicBool::new(false);
static INIT_ONCE: Once = Once::new();

/// Batch size for embedding calls — one ONNX session, large batches.
const EMBED_BATCH_SIZE: usize = 256;
/// Channel capacity between ingestion → embedder.
const INGEST_CHANNEL_CAP: usize = 512;
/// Channel capacity between embedder → DB writer (in batches).
const WRITER_CHANNEL_CAP: usize = 32;

/// Check if an index exists at the given path.
pub fn index_exists(index_path: &Path) -> bool {
    index_path.exists()
}

/// Check if indexing is currently in progress.
pub fn is_indexing() -> bool {
    INDEXING_IN_PROGRESS.load(Ordering::Relaxed)
}

/// Set the indexing-in-progress flag.
pub fn set_indexing(val: bool) {
    INDEXING_IN_PROGRESS.store(val, Ordering::Relaxed);
}

/// Get the number of commits indexed so far in the background task.
pub fn indexed_count() -> u64 {
    INDEXING_COMMITS_DONE.load(Ordering::Relaxed)
}

/// Check if background indexing finished successfully.
pub fn indexing_finished() -> bool {
    INDEXING_FINISHED.load(Ordering::Relaxed)
}

/// Spawn background indexing for a repo if not already running.
/// Returns immediately — the indexing happens in a tokio task.
pub fn spawn_background_index(git_root: PathBuf, repo_hash: String, branch: String) {
    INIT_ONCE.call_once(move || {
        set_indexing(true);
        INDEXING_COMMITS_DONE.store(0, Ordering::Relaxed);
        INDEXING_FINISHED.store(false, Ordering::Relaxed);
        INDEXING_ERROR.store(false, Ordering::Relaxed);

        info!(
            "Auto-init: spawning background indexing for {}/{}",
            &repo_hash[..8],
            branch
        );

        tokio::spawn(async move {
            match run_background_index(&git_root, &repo_hash, &branch).await {
                Ok(count) => {
                    info!(
                        "Auto-init: background indexing complete — {} commits indexed",
                        count
                    );
                    INDEXING_FINISHED.store(true, Ordering::Relaxed);
                }
                Err(e) => {
                    error!("Auto-init: background indexing failed: {}", e);
                    INDEXING_ERROR.store(true, Ordering::Relaxed);
                }
            }
            set_indexing(false);
        });
    });
}

/// The actual indexing work — streaming 3-stage pipeline.
///   [Git Ingestion] →(chan)→ [Single Embedder] →(chan)→ [DB Writer]
async fn run_background_index(
    git_root: &Path,
    repo_hash: &str,
    branch: &str,
) -> Result<u64, SlgError> {
    let index_path = paths::safe_index_path(repo_hash, branch)?;

    // Ensure ~/.slg/ directory
    let slg_home = paths::slg_home();
    std::fs::create_dir_all(&slg_home).map_err(SlgError::Io)?;

    // Install hooks silently
    if let Err(e) = slg_git::hooks::install_hooks(git_root) {
        debug!("Auto-init: hook install skipped: {}", e);
    }

    let store = IndexStore::open(&index_path)?;
    store.create_schema()?;

    // ── Stage 1: Git ingestion ────────────────────────────────────────────
    let (ingest_tx, mut ingest_rx) =
        tokio::sync::mpsc::channel::<CommitDoc>(INGEST_CHANNEL_CAP);

    let git_root_owned = git_root.to_path_buf();
    let branch_owned = branch.to_string();

    let ingest_handle = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let sanitizer = CommitSanitizer;
        let redactor = SecretRedactor::new();
        rt.block_on(ingestion::index_full_branch(
            &git_root_owned,
            &branch_owned,
            &sanitizer,
            &redactor,
            ingest_tx,
        ))
    });

    // ── Stage 2: Embedder ─────────────────────────────────────────────────
    let (writer_tx, mut writer_rx) =
        tokio::sync::mpsc::channel::<(Vec<CommitDoc>, Vec<Vec<f32>>)>(WRITER_CHANNEL_CAP);

    let store_for_filter = IndexStore::open(&index_path)?;
    let embed_handle = tokio::task::spawn_blocking(move || {
        let embedder = Embedder::new()?;
        let rt = tokio::runtime::Handle::current();
        let mut batch: Vec<CommitDoc> = Vec::with_capacity(EMBED_BATCH_SIZE);

        loop {
            let doc_opt = rt.block_on(ingest_rx.recv());
            match doc_opt {
                Some(doc) => {
                    if store_for_filter.commit_exists(&doc.hash).unwrap_or(true) {
                        continue;
                    }
                    batch.push(doc);
                }
                None => {
                    // Channel closed — flush remaining
                    if !batch.is_empty() {
                        let doc_refs: Vec<&CommitDoc> = batch.iter().collect();
                        let embeddings = embedder.embed_batch(&doc_refs)?;
                        INDEXING_COMMITS_DONE.fetch_add(batch.len() as u64, Ordering::Relaxed);
                        rt.block_on(writer_tx.send((batch, embeddings)))
                            .map_err(|_| SlgError::Embedding("Writer channel closed".into()))?;
                    }
                    break;
                }
            }

            if batch.len() >= EMBED_BATCH_SIZE {
                let doc_refs: Vec<&CommitDoc> = batch.iter().collect();
                let embeddings = embedder.embed_batch(&doc_refs)?;
                INDEXING_COMMITS_DONE.fetch_add(batch.len() as u64, Ordering::Relaxed);
                let ready_batch = std::mem::replace(&mut batch, Vec::with_capacity(EMBED_BATCH_SIZE));
                rt.block_on(writer_tx.send((ready_batch, embeddings)))
                    .map_err(|_| SlgError::Embedding("Writer channel closed".into()))?;
            }
        }
        Ok::<(), SlgError>(())
    });

    // ── Stage 3: DB writer ────────────────────────────────────────────────
    let stored_count = std::sync::Arc::new(AtomicU64::new(0));
    let stored_clone = stored_count.clone();
    let store_for_writer = IndexStore::open(&index_path)?;

    let writer_handle = tokio::spawn(async move {
        while let Some((docs, embeddings)) = writer_rx.recv().await {
            let count = store_for_writer.store_batch(&docs, &embeddings)?;
            for doc in &docs {
                slg_index::bm25::BM25Index::index_commit(&store_for_writer, doc)?;
            }
            stored_clone.fetch_add(count, Ordering::Relaxed);
        }
        Ok::<(), SlgError>(())
    });

    // ── Wait for pipeline ─────────────────────────────────────────────────
    match ingest_handle.await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => debug!("Auto-init: ingestion warning: {}", e),
        Err(e) => debug!("Auto-init: ingestion task panicked: {}", e),
    }

    embed_handle
        .await
        .map_err(|e| SlgError::Embedding(format!("Embed task panicked: {}", e)))??;

    writer_handle
        .await
        .map_err(|e| SlgError::Embedding(format!("Writer task panicked: {}", e)))??;

    let stored = stored_count.load(Ordering::Relaxed);

    let base_branch = detector::detect_base_branch(git_root);
    store.update_metadata(repo_hash, branch, &base_branch)?;

    Ok(stored)
}

/// Generate the "initializing" response with live progress info.
pub fn initializing_response(tool_name: &str) -> Value {
    let commits_so_far = indexed_count();
    let message = if commits_so_far > 0 {
        format!(
            "slg is indexing this repository ({} commits indexed so far). Please retry in a few seconds.",
            commits_so_far
        )
    } else {
        "slg is indexing this repository for the first time. Please retry in a few seconds.".to_string()
    };

    json!({
        "content": [{
            "type": "text",
            "text": message
        }],
        "status": "initializing",
        "commits_indexed": commits_so_far,
        "tool": tool_name
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initializing_response() {
        let resp = initializing_response("slg_why");
        assert_eq!(resp["status"], "initializing");
        assert_eq!(resp["tool"], "slg_why");
    }

    #[test]
    fn test_initializing_response_with_progress() {
        INDEXING_COMMITS_DONE.store(42, Ordering::Relaxed);
        let resp = initializing_response("slg_why");
        assert_eq!(resp["commits_indexed"], 42);
        let text = resp["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("42"));
        INDEXING_COMMITS_DONE.store(0, Ordering::Relaxed);
    }

    #[test]
    fn test_indexing_flag() {
        set_indexing(false);
        assert!(!is_indexing());
        set_indexing(true);
        assert!(is_indexing());
        set_indexing(false);
    }
}
