use slg_core::errors::SlgError;
use slg_core::types::SearchResult;

/// Optional cross-encoder reranker.
/// Phase 1: passthrough (no reranking). Future: load a cross-encoder model.
pub struct Reranker;

impl Reranker {
    /// Create a new reranker (no-op in Phase 1).
    pub fn new() -> Result<Self, SlgError> {
        Ok(Self)
    }

    /// Rerank search results using a cross-encoder model.
    /// Phase 1: returns results unchanged with rerank_score set to relevance.
    pub fn rerank(
        &self,
        _query: &str,
        mut results: Vec<SearchResult>,
    ) -> Result<Vec<SearchResult>, SlgError> {
        // Phase 1: passthrough — just copy relevance to rerank_score
        for r in &mut results {
            r.rerank_score = Some(r.relevance);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slg_core::types::{CommitDoc, CommitIntent};

    #[test]
    fn test_reranker_passthrough() {
        let reranker = Reranker::new().unwrap();

        let doc = CommitDoc {
            hash: "abc".to_string(),
            short_hash: "abc".to_string(),
            message: "test".to_string(),
            body: None,
            diff_summary: String::new(),
            author: "T".to_string(),
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
        };

        let results = vec![SearchResult {
            commit: doc,
            relevance: 0.8,
            vector_score: 0.7,
            bm25_score: 0.5,
            rank: 1,
            matched_terms: vec![],
            token_count: 50,
            rerank_score: None,
        }];

        let reranked = reranker.rerank("test query", results).unwrap();
        assert_eq!(reranked.len(), 1);
        assert_eq!(reranked[0].rerank_score, Some(0.8));
    }
}
