use rusqlite::{params, Connection, OptionalExtension};
use slg_core::errors::SlgError;
use slg_core::types::{CommitDoc, CommitIntent, IndexMetadata};
use std::path::{Path, PathBuf};
use tracing::debug;

/// SQLite-backed index store for commit documents and embeddings.
pub struct IndexStore {
    conn: Connection,
    path: PathBuf,
}

impl IndexStore {
    /// Open or create a SQLite DB at the given path.
    pub fn open(path: &Path) -> Result<Self, SlgError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SlgError::Database(format!("Failed to create index directory: {}", e))
            })?;
        }

        let conn = Connection::open(path)
            .map_err(|e| SlgError::Database(format!("Failed to open database: {}", e)))?;

        // Enable WAL mode for concurrent reads
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| SlgError::Database(format!("Failed to set WAL mode: {}", e)))?;

        let store = Self {
            conn,
            path: path.to_path_buf(),
        };
        store.create_schema()?;

        Ok(store)
    }

    /// Create all tables and indices.
    pub fn create_schema(&self) -> Result<(), SlgError> {
        self.conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS commits (
                hash               TEXT PRIMARY KEY,
                short_hash         TEXT NOT NULL,
                message            TEXT NOT NULL,
                body               TEXT,
                diff_summary       TEXT NOT NULL,
                author             TEXT NOT NULL,
                timestamp          INTEGER NOT NULL,
                files_changed      TEXT NOT NULL,
                insertions         INTEGER NOT NULL DEFAULT 0,
                deletions          INTEGER NOT NULL DEFAULT 0,
                linked_issues      TEXT NOT NULL,
                linked_prs         TEXT NOT NULL,
                intent             TEXT NOT NULL,
                risk_score         REAL NOT NULL DEFAULT 0.0,
                branch             TEXT NOT NULL,
                injection_flagged  INTEGER NOT NULL DEFAULT 0,
                secrets_redacted   INTEGER NOT NULL DEFAULT 0,
                indexed_at         INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS commit_embeddings (
                hash       TEXT PRIMARY KEY REFERENCES commits(hash) ON DELETE CASCADE,
                embedding  BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS bm25_terms (
                hash     TEXT NOT NULL REFERENCES commits(hash) ON DELETE CASCADE,
                term     TEXT NOT NULL,
                tf       REAL NOT NULL,
                PRIMARY KEY (hash, term)
            );

            CREATE TABLE IF NOT EXISTS bm25_doc_freq (
                term       TEXT PRIMARY KEY,
                doc_freq   INTEGER NOT NULL DEFAULT 0,
                total_docs INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS file_signals (
                file_path   TEXT NOT NULL,
                commit_hash TEXT NOT NULL REFERENCES commits(hash) ON DELETE CASCADE,
                churn_score REAL NOT NULL DEFAULT 0.0,
                PRIMARY KEY (file_path, commit_hash)
            );

            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_commits_timestamp ON commits(timestamp);
            CREATE INDEX IF NOT EXISTS idx_commits_author ON commits(author);
            CREATE INDEX IF NOT EXISTS idx_commits_intent ON commits(intent);
            CREATE INDEX IF NOT EXISTS idx_bm25_terms_term ON bm25_terms(term);
            ",
            )
            .map_err(|e| SlgError::Database(format!("Failed to create schema: {}", e)))?;

        // Set schema version if not set
        self.conn
            .execute(
                "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '1')",
                [],
            )
            .map_err(|e| SlgError::Database(format!("Failed to set schema version: {}", e)))?;

        Ok(())
    }

    /// Store a commit document along with its embedding.
    /// Idempotent: skips if hash already exists.
    pub fn store_commit(&self, doc: &CommitDoc, embedding: &[f32]) -> Result<(), SlgError> {
        if self.commit_exists(&doc.hash)? {
            return Ok(());
        }

        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| SlgError::Database(format!("Failed to begin transaction: {}", e)))?;

        let now = chrono::Utc::now().timestamp();
        let files_json = serde_json::to_string(&doc.files_changed).unwrap_or_default();
        let issues_json = serde_json::to_string(&doc.linked_issues).unwrap_or_default();
        let prs_json = serde_json::to_string(&doc.linked_prs).unwrap_or_default();

        tx.execute(
            "INSERT INTO commits (hash, short_hash, message, body, diff_summary, author,
             timestamp, files_changed, insertions, deletions, linked_issues, linked_prs,
             intent, risk_score, branch, injection_flagged, secrets_redacted, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                doc.hash,
                doc.short_hash,
                doc.message,
                doc.body,
                doc.diff_summary,
                doc.author,
                doc.timestamp,
                files_json,
                doc.insertions,
                doc.deletions,
                issues_json,
                prs_json,
                format!("{:?}", doc.intent),
                doc.risk_score,
                doc.branch,
                doc.injection_flagged as i32,
                doc.secrets_redacted,
                now,
            ],
        )
        .map_err(|e| SlgError::Database(format!("Failed to insert commit: {}", e)))?;

        // Store embedding as blob (f32 array → bytes)
        let embedding_bytes = embedding_to_bytes(embedding);
        tx.execute(
            "INSERT INTO commit_embeddings (hash, embedding) VALUES (?1, ?2)",
            params![doc.hash, embedding_bytes],
        )
        .map_err(|e| SlgError::Database(format!("Failed to insert embedding: {}", e)))?;

        tx.commit()
            .map_err(|e| SlgError::Database(format!("Failed to commit transaction: {}", e)))?;

        debug!("Stored commit: {}", doc.short_hash);
        Ok(())
    }

    /// Check if a commit hash exists in the index.
    pub fn commit_exists(&self, hash: &str) -> Result<bool, SlgError> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM commits WHERE hash = ?1",
                params![hash],
                |row| row.get(0),
            )
            .map_err(|e| SlgError::Database(format!("Query failed: {}", e)))?;
        Ok(count > 0)
    }

    /// Get a commit document by hash.
    pub fn get_commit(&self, hash: &str) -> Result<Option<CommitDoc>, SlgError> {
        let result = self
            .conn
            .query_row(
                "SELECT hash, short_hash, message, body, diff_summary, author,
                 timestamp, files_changed, insertions, deletions, linked_issues,
                 linked_prs, intent, risk_score, branch, injection_flagged, secrets_redacted
                 FROM commits WHERE hash = ?1",
                params![hash],
                |row| {
                    let files_json: String = row.get(7)?;
                    let issues_json: String = row.get(10)?;
                    let prs_json: String = row.get(11)?;
                    let intent_str: String = row.get(12)?;

                    Ok(CommitDoc {
                        hash: row.get(0)?,
                        short_hash: row.get(1)?,
                        message: row.get(2)?,
                        body: row.get(3)?,
                        diff_summary: row.get(4)?,
                        author: row.get(5)?,
                        timestamp: row.get(6)?,
                        files_changed: serde_json::from_str(&files_json).unwrap_or_default(),
                        insertions: row.get(8)?,
                        deletions: row.get(9)?,
                        linked_issues: serde_json::from_str(&issues_json).unwrap_or_default(),
                        linked_prs: serde_json::from_str(&prs_json).unwrap_or_default(),
                        intent: parse_intent(&intent_str),
                        risk_score: row.get(13)?,
                        branch: row.get(14)?,
                        injection_flagged: row.get::<_, i32>(15)? != 0,
                        secrets_redacted: row.get::<_, i32>(16)? as u32,
                    })
                },
            )
            .optional()
            .map_err(|e| SlgError::Database(format!("Query failed: {}", e)))?;
        Ok(result)
    }

    /// List all commit hashes in the index.
    pub fn list_all_hashes(&self) -> Result<Vec<String>, SlgError> {
        let mut stmt = self
            .conn
            .prepare("SELECT hash FROM commits ORDER BY timestamp DESC")
            .map_err(|e| SlgError::Database(format!("Prepare failed: {}", e)))?;

        let hashes = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| SlgError::Database(format!("Query failed: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(hashes)
    }

    /// Get the embedding for a commit hash.
    pub fn get_embedding(&self, hash: &str) -> Result<Option<Vec<f32>>, SlgError> {
        let result = self
            .conn
            .query_row(
                "SELECT embedding FROM commit_embeddings WHERE hash = ?1",
                params![hash],
                |row| {
                    let bytes: Vec<u8> = row.get(0)?;
                    Ok(bytes_to_embedding(&bytes))
                },
            )
            .optional()
            .map_err(|e| SlgError::Database(format!("Query failed: {}", e)))?;
        Ok(result)
    }

    /// Vector search: brute-force cosine similarity against all embeddings.
    /// Returns (hash, score) pairs sorted by descending score.
    pub fn vector_search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<(String, f32)>, SlgError> {
        let mut stmt = self
            .conn
            .prepare("SELECT hash, embedding FROM commit_embeddings")
            .map_err(|e| SlgError::Database(format!("Prepare failed: {}", e)))?;

        let mut results: Vec<(String, f32)> = stmt
            .query_map([], |row| {
                let hash: String = row.get(0)?;
                let bytes: Vec<u8> = row.get(1)?;
                Ok((hash, bytes))
            })
            .map_err(|e| SlgError::Database(format!("Query failed: {}", e)))?
            .filter_map(|r| r.ok())
            .map(|(hash, bytes)| {
                let embedding = bytes_to_embedding(&bytes);
                let score = cosine_similarity(query_embedding, &embedding);
                (hash, score)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }

    /// Update last_accessed timestamp in meta table.
    pub fn update_last_accessed(&self) -> Result<(), SlgError> {
        let now = chrono::Utc::now().timestamp();
        self.conn
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_accessed', ?1)",
                params![now.to_string()],
            )
            .map_err(|e| SlgError::Database(format!("Update failed: {}", e)))?;
        Ok(())
    }

    /// Get index metadata.
    pub fn get_metadata(
        &self,
        repo_hash: &str,
        branch: &str,
        base_branch: &str,
    ) -> Result<IndexMetadata, SlgError> {
        let commit_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM commits", [], |row| row.get(0))
            .unwrap_or(0);

        let last_commit = self.get_meta("last_commit").unwrap_or_default();
        let indexed_at: i64 = self
            .get_meta("indexed_at")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let last_accessed: i64 = self
            .get_meta("last_accessed")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let model_version = self.get_meta("model_version").unwrap_or_default();
        let index_version: u32 = self
            .get_meta("index_version")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let is_delta: bool = self
            .get_meta("is_delta")
            .map(|s| s == "true")
            .unwrap_or(false);

        Ok(IndexMetadata {
            repo_hash: repo_hash.to_string(),
            branch: branch.to_string(),
            base_branch: base_branch.to_string(),
            commit_count: commit_count as u64,
            last_commit,
            indexed_at,
            last_accessed,
            model_version,
            index_version,
            size_bytes: self.get_size_bytes().unwrap_or(0),
            is_delta,
        })
    }

    /// Get a meta value by key.
    fn get_meta(&self, key: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    /// Update metadata after indexing completes.
    pub fn update_metadata(
        &self,
        repo_hash: &str,
        branch: &str,
        base_branch: &str,
    ) -> Result<(), SlgError> {
        let now = chrono::Utc::now().timestamp();
        let pairs = [
            ("repo_hash", repo_hash.to_string()),
            ("branch", branch.to_string()),
            ("base_branch", base_branch.to_string()),
            ("indexed_at", now.to_string()),
            ("last_accessed", now.to_string()),
            ("index_version", "1".to_string()),
            ("model_version", "all-MiniLM-L6-v2".to_string()),
        ];
        for (key, value) in &pairs {
            self.conn
                .execute(
                    "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
                    params![key, value],
                )
                .map_err(|e| SlgError::Database(format!("Meta update failed: {}", e)))?;
        }
        Ok(())
    }

    /// Get the file size of the .db file.
    pub fn get_size_bytes(&self) -> Result<u64, SlgError> {
        let metadata = std::fs::metadata(&self.path)
            .map_err(|e| SlgError::Database(format!("Failed to get file metadata: {}", e)))?;
        Ok(metadata.len())
    }

    /// Get a reference to the underlying connection (for BM25 operations).
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

/// Convert f32 embedding to bytes for blob storage.
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert bytes back to f32 embedding.
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Parse intent string from DB back to CommitIntent.
fn parse_intent(s: &str) -> CommitIntent {
    match s {
        "Fix" => CommitIntent::Fix,
        "Feature" => CommitIntent::Feature,
        "Refactor" => CommitIntent::Refactor,
        "Perf" => CommitIntent::Perf,
        "Security" => CommitIntent::Security,
        "Docs" => CommitIntent::Docs,
        "Test" => CommitIntent::Test,
        "Chore" => CommitIntent::Chore,
        "Revert" => CommitIntent::Revert,
        _ => CommitIntent::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_doc(hash: &str, message: &str) -> CommitDoc {
        CommitDoc {
            hash: hash.to_string(),
            short_hash: hash[..7.min(hash.len())].to_string(),
            message: message.to_string(),
            body: None,
            diff_summary: "src/main.rs: modified".to_string(),
            author: "Test User".to_string(),
            timestamp: 1700000000,
            files_changed: vec!["src/main.rs".to_string()],
            insertions: 10,
            deletions: 5,
            linked_issues: vec![],
            linked_prs: vec![],
            intent: CommitIntent::Fix,
            risk_score: 0.3,
            branch: "main".to_string(),
            injection_flagged: false,
            secrets_redacted: 0,
        }
    }

    #[test]
    fn test_store_and_retrieve_commit() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = IndexStore::open(&db_path).unwrap();

        let doc = make_test_doc("abc123def456789", "fix: resolve crash");
        let embedding = vec![0.1f32; 384];
        store.store_commit(&doc, &embedding).unwrap();

        let retrieved = store.get_commit("abc123def456789").unwrap().unwrap();
        assert_eq!(retrieved.message, "fix: resolve crash");
        assert_eq!(retrieved.author, "Test User");
    }

    #[test]
    fn test_commit_exists() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = IndexStore::open(&db_path).unwrap();

        assert!(!store.commit_exists("nonexistent").unwrap());

        let doc = make_test_doc("abc123def456789", "fix: test");
        store.store_commit(&doc, &vec![0.1f32; 384]).unwrap();

        assert!(store.commit_exists("abc123def456789").unwrap());
    }

    #[test]
    fn test_idempotent_store() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = IndexStore::open(&db_path).unwrap();

        let doc = make_test_doc("abc123def456789", "fix: test");
        let embedding = vec![0.1f32; 384];
        store.store_commit(&doc, &embedding).unwrap();
        store.store_commit(&doc, &embedding).unwrap(); // Should not error

        let hashes = store.list_all_hashes().unwrap();
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn test_vector_search() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = IndexStore::open(&db_path).unwrap();

        // Store 3 commits with different embeddings
        let mut emb1 = vec![0.0f32; 384];
        emb1[0] = 1.0;
        let doc1 = make_test_doc("hash1_abcdefghij", "fix: auth crash");
        store.store_commit(&doc1, &emb1).unwrap();

        let mut emb2 = vec![0.0f32; 384];
        emb2[1] = 1.0;
        let doc2 = make_test_doc("hash2_abcdefghij", "feat: new login");
        store.store_commit(&doc2, &emb2).unwrap();

        let mut emb3 = vec![0.0f32; 384];
        emb3[0] = 0.9;
        emb3[1] = 0.1;
        let doc3 = make_test_doc("hash3_abcdefghij", "fix: session timeout");
        store.store_commit(&doc3, &emb3).unwrap();

        // Search for something similar to emb1
        let query = emb1.clone();
        let results = store.vector_search(&query, 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "hash1_abcdefghij"); // Most similar
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![1.0f32, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0f32, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_metadata() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = IndexStore::open(&db_path).unwrap();

        let meta = store.get_metadata("repo123", "main", "main").unwrap();
        assert_eq!(meta.commit_count, 0);
        assert_eq!(meta.repo_hash, "repo123");
        assert_eq!(meta.branch, "main");
    }

    #[test]
    fn test_embedding_roundtrip() {
        let original: Vec<f32> = (0..384).map(|i| i as f32 * 0.01).collect();
        let bytes = embedding_to_bytes(&original);
        let recovered = bytes_to_embedding(&bytes);
        assert_eq!(original.len(), recovered.len());
        for (a, b) in original.iter().zip(recovered.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }
}
