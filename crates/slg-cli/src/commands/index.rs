use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use slg_core::errors::SlgError;
use slg_core::types::CommitDoc;
use slg_git::{detector, ingestion};
use slg_index::embedder::Embedder;
use slg_index::store::IndexStore;
use slg_security::paths;
use slg_security::redactor::SecretRedactor;
use slg_security::sanitizer::CommitSanitizer;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

/// Batch size for embedding calls. Large batches let ONNX Runtime amortize
/// overhead and maximise SIMD throughput within a single session.
const EMBED_BATCH_SIZE: usize = 256;
/// Channel capacity between ingestion → embedder.
const INGEST_CHANNEL_CAP: usize = 512;
/// Channel capacity between embedder → DB writer (in batches).
const WRITER_CHANNEL_CAP: usize = 32;

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
///
/// Architecture: 3 concurrent stages connected by channels.
///   [Git Ingestion] →(chan)→ [Single Embedder] →(chan)→ [DB Writer]
/// All stages run concurrently so git I/O, ONNX inference, and DB writes
/// overlap in time.
pub async fn run(args: IndexArgs) -> Result<(), SlgError> {
    let cwd = std::env::current_dir().map_err(SlgError::Io)?;
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

    let embedded_count = Arc::new(AtomicU64::new(0));
    let stored_count = Arc::new(AtomicU64::new(0));
    let skipped_count = Arc::new(AtomicU64::new(0));
    let ingested_count = Arc::new(AtomicU64::new(0));
    let embed_time_us = Arc::new(AtomicU64::new(0));
    let write_time_us = Arc::new(AtomicU64::new(0));
    let bm25_time_us = Arc::new(AtomicU64::new(0));

    let pipeline_start = Instant::now();

    // ── Stage 1: Git ingestion ────────────────────────────────────────────
    // Streams CommitDoc through a channel, skipping already-indexed commits.
    let (ingest_tx, mut ingest_rx) =
        tokio::sync::mpsc::channel::<CommitDoc>(INGEST_CHANNEL_CAP);

    let git_root_clone = git_root.clone();
    let branch_clone = branch.clone();
    let ingest_counter = ingested_count.clone();

    let ingest_handle = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let sanitizer = CommitSanitizer;
        let redactor = SecretRedactor::new();

        // Wrap sender to count ingested commits
        let (counting_tx, mut counting_rx) =
            tokio::sync::mpsc::channel::<CommitDoc>(INGEST_CHANNEL_CAP);
        let ingest_tx_inner = ingest_tx;
        let counter = ingest_counter;

        // Forward task: count and relay
        let fwd = rt.spawn(async move {
            while let Some(doc) = counting_rx.recv().await {
                counter.fetch_add(1, Ordering::Relaxed);
                if ingest_tx_inner.send(doc).await.is_err() {
                    break;
                }
            }
        });

        let result = rt.block_on(ingestion::index_full_branch(
            &git_root_clone,
            &branch_clone,
            &sanitizer,
            &redactor,
            counting_tx,
        ));
        rt.block_on(fwd).ok();
        result
    });

    // ── Stage 2: Embedder ─────────────────────────────────────────────────
    // Single ONNX session, large batches for maximum throughput.
    // Reads from ingest channel, sends (docs, embeddings) batches to writer.
    let (writer_tx, mut writer_rx) =
        tokio::sync::mpsc::channel::<(Vec<CommitDoc>, Vec<Vec<f32>>)>(WRITER_CHANNEL_CAP);

    let embed_progress = embedded_count.clone();
    let skip_counter = skipped_count.clone();
    let embed_timer = embed_time_us.clone();

    // Filter + batch + embed in a spawn_blocking so ONNX gets a real thread.
    let store_for_filter = IndexStore::open(&index_path)?;
    let embed_handle = tokio::task::spawn_blocking(move || {
        let model_start = Instant::now();
        let embedder = Embedder::new()?;
        let model_load_ms = model_start.elapsed().as_millis();
        eprintln!("  [embed] Model loaded in {}ms", model_load_ms);

        let rt = tokio::runtime::Handle::current();
        let mut batch: Vec<CommitDoc> = Vec::with_capacity(EMBED_BATCH_SIZE);

        loop {
            // Drain from channel into batch
            let doc_opt = rt.block_on(ingest_rx.recv());
            match doc_opt {
                Some(doc) => {
                    // Skip already indexed
                    if store_for_filter.commit_exists(&doc.hash).unwrap_or(true) {
                        skip_counter.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    batch.push(doc);
                }
                None => {
                    // Channel closed — flush remaining batch
                    if !batch.is_empty() {
                        let doc_refs: Vec<&CommitDoc> = batch.iter().collect();
                        let t = Instant::now();
                        let embeddings = embedder.embed_batch(&doc_refs)?;
                        embed_timer.fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);
                        embed_progress.fetch_add(batch.len() as u64, Ordering::Relaxed);
                        rt.block_on(writer_tx.send((batch, embeddings)))
                            .map_err(|_| SlgError::Embedding("Writer channel closed".into()))?;
                    }
                    break;
                }
            }

            if batch.len() >= EMBED_BATCH_SIZE {
                let doc_refs: Vec<&CommitDoc> = batch.iter().collect();
                let t = Instant::now();
                let embeddings = embedder.embed_batch(&doc_refs)?;
                embed_timer.fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);
                embed_progress.fetch_add(batch.len() as u64, Ordering::Relaxed);
                let ready_batch = std::mem::replace(&mut batch, Vec::with_capacity(EMBED_BATCH_SIZE));
                rt.block_on(writer_tx.send((ready_batch, embeddings)))
                    .map_err(|_| SlgError::Embedding("Writer channel closed".into()))?;
            }
        }
        Ok::<(), SlgError>(())
    });

    // ── Stage 3: DB writer ────────────────────────────────────────────────
    // Receives embedded batches and writes to SQLite in transactions.
    let store_progress = stored_count.clone();
    let write_timer = write_time_us.clone();
    let bm25_timer = bm25_time_us.clone();
    let store_for_writer = IndexStore::open(&index_path)?;
    let writer_handle = tokio::spawn(async move {
        while let Some((docs, embeddings)) = writer_rx.recv().await {
            let t = Instant::now();
            let count = store_for_writer.store_batch(&docs, &embeddings)?;
            write_timer.fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);

            let t2 = Instant::now();
            for doc in &docs {
                slg_index::bm25::BM25Index::index_commit(&store_for_writer, doc)?;
            }
            bm25_timer.fetch_add(t2.elapsed().as_micros() as u64, Ordering::Relaxed);

            store_progress.fetch_add(count, Ordering::Relaxed);
        }
        Ok::<(), SlgError>(())
    });

    // ── Progress poller ───────────────────────────────────────────────────
    let pb_clone = pb.as_ref().cloned();
    let embed_for_poll = embedded_count.clone();
    let store_for_poll = stored_count.clone();
    let ingest_for_poll = ingested_count.clone();
    let skip_for_poll = skipped_count.clone();
    let progress_task = tokio::spawn(async move {
        loop {
            let ing = ingest_for_poll.load(Ordering::Relaxed);
            let skp = skip_for_poll.load(Ordering::Relaxed);
            let emb = embed_for_poll.load(Ordering::Relaxed);
            let sto = store_for_poll.load(Ordering::Relaxed);
            if let Some(ref pb) = pb_clone {
                pb.set_message(format!(
                    "ingested {} | skipped {} | embedded {} | stored {}",
                    ing, skp, emb, sto
                ));
                pb.set_position(sto);
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    });

    // ── Wait for pipeline to complete ─────────────────────────────────────
    // Ingestion finishes first (drops ingest_tx) → embed loop exits →
    // drops writer_tx → writer loop exits.
    match ingest_handle.await {
        Ok(Ok(_)) => {}
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

    embed_handle
        .await
        .map_err(|e| SlgError::Embedding(format!("Embed task panicked: {}", e)))??;

    writer_handle
        .await
        .map_err(|e| SlgError::Embedding(format!("Writer task panicked: {}", e)))??;

    progress_task.abort();
    let _ = progress_task.await;

    let total_elapsed = pipeline_start.elapsed();
    let stored = stored_count.load(Ordering::Relaxed);
    let ingested = ingested_count.load(Ordering::Relaxed);
    let skipped = skipped_count.load(Ordering::Relaxed);
    let embedded = embedded_count.load(Ordering::Relaxed);
    let embed_ms = embed_time_us.load(Ordering::Relaxed) / 1000;
    let write_ms = write_time_us.load(Ordering::Relaxed) / 1000;
    let bm25_ms = bm25_time_us.load(Ordering::Relaxed) / 1000;

    if let Some(pb) = pb {
        if stored == 0 {
            pb.finish_with_message("Already up to date");
        } else {
            pb.finish_with_message(format!("Indexed {} commits", stored));
        }
    }

    // ── Analytics summary ─────────────────────────────────────────────────
    if !args.silent {
        let total_ms = total_elapsed.as_millis();
        let throughput = if total_ms > 0 {
            (stored as f64 / total_ms as f64) * 1000.0
        } else {
            0.0
        };
        let embed_per_sec = if embed_ms > 0 {
            (embedded as f64 / embed_ms as f64) * 1000.0
        } else {
            0.0
        };

        eprintln!();
        eprintln!("  ┌──────────────────────────────────────────┐");
        eprintln!("  │          Indexing Analytics               │");
        eprintln!("  ├──────────────────────────────────────────┤");
        eprintln!("  │  Total time:       {:>8.1}s              │", total_ms as f64 / 1000.0);
        eprintln!("  │  Commits ingested: {:>8}               │", ingested);
        eprintln!("  │  Commits skipped:  {:>8}               │", skipped);
        eprintln!("  │  Commits embedded: {:>8}               │", embedded);
        eprintln!("  │  Commits stored:   {:>8}               │", stored);
        eprintln!("  ├──────────────────────────────────────────┤");
        eprintln!("  │  Embedding time:   {:>8.1}s ({:>4.1}%)     │",
            embed_ms as f64 / 1000.0,
            if total_ms > 0 { embed_ms as f64 / total_ms as f64 * 100.0 } else { 0.0 });
        eprintln!("  │  DB write time:    {:>8.1}s ({:>4.1}%)     │",
            write_ms as f64 / 1000.0,
            if total_ms > 0 { write_ms as f64 / total_ms as f64 * 100.0 } else { 0.0 });
        eprintln!("  │  BM25 index time:  {:>8.1}s ({:>4.1}%)     │",
            bm25_ms as f64 / 1000.0,
            if total_ms > 0 { bm25_ms as f64 / total_ms as f64 * 100.0 } else { 0.0 });
        eprintln!("  │  Pipeline overhead:{:>8.1}s              │",
            (total_ms as f64 - embed_ms as f64 - write_ms as f64 - bm25_ms as f64) / 1000.0);
        eprintln!("  ├──────────────────────────────────────────┤");
        eprintln!("  │  Embed throughput: {:>8.1} commits/s     │", embed_per_sec);
        eprintln!("  │  Overall rate:     {:>8.1} commits/s     │", throughput);
        if embedded > 0 {
            eprintln!("  │  Avg embed/commit: {:>8.1}ms             │", embed_ms as f64 / embedded as f64);
            eprintln!("  │  Avg write/commit: {:>8.1}ms             │", write_ms as f64 / stored.max(1) as f64);
            eprintln!("  │  Avg BM25/commit:  {:>8.1}ms             │", bm25_ms as f64 / stored.max(1) as f64);
        }
        eprintln!("  └──────────────────────────────────────────┘");
    }

    // Update metadata
    store.update_metadata(
        &repo_hash,
        &branch,
        &detector::detect_base_branch(&git_root),
    )?;

    if !args.silent {
        eprintln!("Index stored at: {}", index_path.display());
    }

    info!(
        "Indexed {} commits for {}/{}",
        stored,
        &repo_hash[..8],
        branch
    );

    Ok(())
}
