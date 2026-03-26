use rusqlite::params;
use slg_core::errors::SlgError;
use slg_core::types::CommitDoc;

use crate::store::IndexStore;

const K1: f32 = 1.5;
const B: f32 = 0.75;

/// Stop words to remove during tokenization.
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "to", "of", "and", "or", "in",
    "on", "at", "for", "with", "by", "this", "that", "it", "its", "we", "our", "i", "you", "he",
    "she",
];

/// BM25 lexical search index.
pub struct BM25Index;

impl BM25Index {
    /// Tokenize text: lowercase, split, remove stopwords, length-filter, dedup.
    pub fn tokenize(text: &str) -> Vec<String> {
        let lower = text.to_lowercase();
        let tokens: Vec<String> = lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| !t.is_empty())
            .filter(|t| t.len() >= 2 && t.len() <= 50)
            .filter(|t| !STOPWORDS.contains(t))
            .map(|t| t.to_string())
            .collect();

        // Deduplicate while preserving order
        let mut seen = std::collections::HashSet::new();
        tokens
            .into_iter()
            .filter(|t| seen.insert(t.clone()))
            .collect()
    }

    /// Index a single commit document into the BM25 tables.
    pub fn index_commit(store: &IndexStore, doc: &CommitDoc) -> Result<(), SlgError> {
        let mut text = doc.message.clone();
        if let Some(body) = &doc.body {
            text.push(' ');
            text.push_str(body);
        }
        text.push(' ');
        text.push_str(&doc.files_changed.join(" "));
        text.push(' ');
        text.push_str(&doc.linked_issues.join(" "));

        let tokens = Self::tokenize(&text);
        if tokens.is_empty() {
            return Ok(());
        }

        // Count term frequencies
        let mut term_counts = std::collections::HashMap::new();
        // Re-tokenize without dedup to get actual counts
        let lower = text.to_lowercase();
        let all_tokens: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| !t.is_empty())
            .filter(|t| t.len() >= 2 && t.len() <= 50)
            .filter(|t| !STOPWORDS.contains(t))
            .collect();

        let total_terms = all_tokens.len() as f32;
        for token in &all_tokens {
            *term_counts.entry(token.to_string()).or_insert(0u32) += 1;
        }

        let conn = store.connection();

        // Store TF for each unique term
        for (term, count) in &term_counts {
            let tf = *count as f32 / total_terms;
            conn.execute(
                "INSERT OR REPLACE INTO bm25_terms (hash, term, tf) VALUES (?1, ?2, ?3)",
                params![doc.hash, term, tf],
            )
            .map_err(|e| SlgError::Database(format!("BM25 index_commit: {}", e)))?;
        }

        // Update document frequency for each unique term
        for term in term_counts.keys() {
            conn.execute(
                "INSERT INTO bm25_doc_freq (term, doc_freq) VALUES (?1, 1)
                 ON CONFLICT(term) DO UPDATE SET doc_freq = doc_freq + 1",
                params![term],
            )
            .map_err(|e| SlgError::Database(format!("BM25 doc_freq update: {}", e)))?;
        }

        Ok(())
    }

    /// Search the BM25 index for commits matching the query.
    /// Returns (hash, bm25_score) pairs sorted by score descending.
    pub fn search(
        store: &IndexStore,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<(String, f32)>, SlgError> {
        let query_tokens = Self::tokenize(query);
        if query_tokens.is_empty() {
            return Ok(vec![]);
        }

        let conn = store.connection();

        // Get total doc count
        let total_docs: f32 = conn
            .query_row("SELECT COUNT(DISTINCT hash) FROM bm25_terms", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap_or(0) as f32;

        if total_docs == 0.0 {
            return Ok(vec![]);
        }

        // Get average document length
        let _avg_dl: f32 = conn
            .query_row(
                "SELECT AVG(doc_len) FROM (
                    SELECT hash, SUM(tf) as doc_len FROM bm25_terms GROUP BY hash
                )",
                [],
                |row| row.get::<_, f64>(0),
            )
            .unwrap_or(1.0) as f32;

        // For each query term, get IDF and matching docs
        let mut doc_scores: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();

        for term in &query_tokens {
            // Get document frequency
            let df: f32 = conn
                .query_row(
                    "SELECT doc_freq FROM bm25_doc_freq WHERE term = ?1",
                    params![term],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as f32;

            if df == 0.0 {
                continue;
            }

            // IDF = ln((N - df + 0.5) / (df + 0.5) + 1)
            let idf = ((total_docs - df + 0.5) / (df + 0.5) + 1.0).ln();

            // Get all docs with this term
            let mut stmt = conn
                .prepare("SELECT hash, tf FROM bm25_terms WHERE term = ?1")
                .map_err(|e| SlgError::Database(format!("BM25 search prepare: {}", e)))?;

            let rows = stmt
                .query_map(params![term], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f32>(1)?))
                })
                .map_err(|e| SlgError::Database(format!("BM25 search query: {}", e)))?;

            for row in rows {
                let (hash, tf) =
                    row.map_err(|e| SlgError::Database(format!("BM25 search row: {}", e)))?;

                // BM25 score component: IDF * (tf * (k1 + 1)) / (tf + k1 * (1 - b + b * dl/avgdl))
                // We use tf directly (already normalized), so dl/avgdl ≈ 1
                let score = idf * (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B));

                *doc_scores.entry(hash).or_insert(0.0) += score;
            }
        }

        // Sort by score descending
        let mut results: Vec<(String, f32)> = doc_scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slg_core::types::CommitIntent;

    fn make_doc(hash: &str, message: &str) -> CommitDoc {
        CommitDoc {
            hash: hash.to_string(),
            short_hash: hash[..7.min(hash.len())].to_string(),
            message: message.to_string(),
            body: None,
            diff_summary: String::new(),
            author: "Test".to_string(),
            timestamp: 0,
            files_changed: vec![],
            insertions: 0,
            deletions: 0,
            linked_issues: vec![],
            linked_prs: vec![],
            intent: CommitIntent::Fix,
            risk_score: 0.0,
            branch: "main".to_string(),
            injection_flagged: false,
            secrets_redacted: 0,
        }
    }

    #[test]
    fn test_tokenize() {
        let tokens = BM25Index::tokenize("fix: resolve the login crash");
        assert!(tokens.contains(&"fix".to_string()));
        assert!(tokens.contains(&"resolve".to_string()));
        assert!(tokens.contains(&"login".to_string()));
        assert!(tokens.contains(&"crash".to_string()));
        // "the" is a stopword
        assert!(!tokens.contains(&"the".to_string()));
    }

    #[test]
    fn test_tokenize_dedup() {
        let tokens = BM25Index::tokenize("fix fix fix crash crash");
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_index_and_search() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = IndexStore::open(&db_path).unwrap();

        let docs = vec![
            make_doc("aaa1111", "fix: resolve login crash on mobile"),
            make_doc("bbb2222", "feat: add new dashboard widget"),
            make_doc("ccc3333", "fix: patch authentication bypass vulnerability"),
        ];

        for doc in &docs {
            store.store_commit(doc, &vec![0.0f32; 384]).unwrap();
            BM25Index::index_commit(&store, doc).unwrap();
        }

        let results = BM25Index::search(&store, "login crash", 10).unwrap();
        assert!(!results.is_empty());
        // First result should be the login crash commit
        assert_eq!(results[0].0, "aaa1111");
    }

    #[test]
    fn test_search_empty_query() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = IndexStore::open(&db_path).unwrap();

        let results = BM25Index::search(&store, "", 10).unwrap();
        assert!(results.is_empty());
    }
}
