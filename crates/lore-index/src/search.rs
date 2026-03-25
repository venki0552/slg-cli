use lore_core::errors::LoreError;
use lore_core::types::{CommitIntent, OutputFormat, SearchResult};
use std::collections::HashMap;
use tracing::debug;

use crate::bm25::BM25Index;
use crate::embedder::Embedder;
use crate::store::IndexStore;

const MAX_QUERY_LEN: usize = 500;
const RRF_K: f32 = 60.0;
const RECENCY_BOOST: f32 = 1.2;
const EXACT_MATCH_BOOST: f32 = 1.5;
const SECURITY_BOOST: f32 = 1.3;
const THIRTY_DAYS_SECS: i64 = 30 * 24 * 3600;

/// Search options controlling filtering and output.
pub struct SearchOptions {
    pub limit: u32,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub author: Option<String>,
    pub module: Option<String>,
    pub max_tokens: usize,
    pub enable_reranker: bool,
    pub format: OutputFormat,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 3,
            since: None,
            until: None,
            author: None,
            module: None,
            max_tokens: 4096,
            enable_reranker: false,
            format: OutputFormat::Xml,
        }
    }
}

/// Full hybrid search pipeline: vector + BM25 + RRF fusion + filters + boosts.
pub async fn search(
    query: &str,
    store: &IndexStore,
    embedder: &Embedder,
    options: &SearchOptions,
) -> Result<Vec<SearchResult>, LoreError> {
    // 1. Validate query
    let query = query.trim();
    if query.is_empty() {
        return Err(LoreError::EmptyQuery);
    }
    if query.len() > MAX_QUERY_LEN {
        return Err(LoreError::QueryTooLong);
    }

    // 2. Embed query
    let query_vector = embedder.embed_query(query)?;

    // 3. Vector search: top 20 candidates
    let vector_results = store.vector_search(&query_vector, 20)?;
    debug!("Vector search returned {} results", vector_results.len());

    // 4. BM25 search: top 20 candidates
    let bm25_results = BM25Index::search(store, query, 20)?;
    debug!("BM25 search returned {} results", bm25_results.len());

    // 5. RRF fusion
    let fused = rrf_fusion(&vector_results, &bm25_results, RRF_K);
    debug!("RRF fusion produced {} candidates", fused.len());

    // Build score maps for vector and BM25
    let vector_scores: HashMap<&str, f32> = vector_results
        .iter()
        .map(|(h, s)| (h.as_str(), *s))
        .collect();
    let bm25_scores: HashMap<&str, f32> = bm25_results
        .iter()
        .map(|(h, s)| (h.as_str(), *s))
        .collect();

    // 6–8. Fetch docs, apply filters and boosts
    let now = chrono::Utc::now().timestamp();
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    let security_keywords = ["security", "vuln", "cve", "exploit", "attack", "auth"];
    let query_has_security = query_words
        .iter()
        .any(|w| security_keywords.iter().any(|k| w.contains(k)));

    let fetch_count = (options.limit * 2) as usize;
    let mut results = Vec::new();
    let mut rank = 1u32;

    for (hash, rrf_score) in fused.iter().take(fetch_count) {
        let doc = match store.get_commit(hash)? {
            Some(d) => d,
            None => continue,
        };

        // Apply filters
        if let Some(since) = options.since {
            if doc.timestamp < since {
                continue;
            }
        }
        if let Some(until) = options.until {
            if doc.timestamp > until {
                continue;
            }
        }
        if let Some(ref author) = options.author {
            if !doc.author.to_lowercase().contains(&author.to_lowercase()) {
                continue;
            }
        }
        if let Some(ref module) = options.module {
            if !doc.files_changed.iter().any(|f| f.starts_with(module.as_str())) {
                continue;
            }
        }

        // Apply boosts
        let mut boosted_score = *rrf_score;

        // Recency boost
        if doc.timestamp > (now - THIRTY_DAYS_SECS) {
            boosted_score *= RECENCY_BOOST;
        }

        // Exact match boost
        let msg_lower = doc.message.to_lowercase();
        if query_words.iter().any(|w| msg_lower.contains(w)) {
            boosted_score *= EXACT_MATCH_BOOST;
        }

        // Security boost
        if doc.intent == CommitIntent::Security && query_has_security {
            boosted_score *= SECURITY_BOOST;
        }

        let v_score = vector_scores.get(hash.as_str()).copied().unwrap_or(0.0);
        let b_score = bm25_scores.get(hash.as_str()).copied().unwrap_or(0.0);

        // Estimate token count (~4 chars per token)
        let text_len = doc.message.len() + doc.diff_summary.len();
        let token_count = (text_len / 4).max(1) as u32;

        // Matched BM25 terms
        let matched_terms = query_words
            .iter()
            .filter(|w| msg_lower.contains(**w))
            .map(|w| w.to_string())
            .collect();

        results.push(SearchResult {
            commit: doc,
            relevance: boosted_score,
            vector_score: v_score,
            bm25_score: b_score,
            rank,
            matched_terms,
            token_count,
            rerank_score: None,
        });
        rank += 1;
    }

    // Sort by boosted relevance
    results.sort_by(|a, b| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Re-assign ranks after sorting
    for (i, r) in results.iter_mut().enumerate() {
        r.rank = (i + 1) as u32;
    }

    // 10. Apply token budget
    results = apply_token_budget(results, options.max_tokens);

    // Truncate to limit
    results.truncate(options.limit as usize);

    Ok(results)
}

/// Reciprocal Rank Fusion of vector and BM25 results.
pub fn rrf_fusion(
    vector_results: &[(String, f32)],
    bm25_results: &[(String, f32)],
    k: f32,
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    for (rank, (hash, _)) in vector_results.iter().enumerate() {
        let rrf = 1.0 / (k + (rank + 1) as f32);
        *scores.entry(hash.clone()).or_insert(0.0) += rrf;
    }

    for (rank, (hash, _)) in bm25_results.iter().enumerate() {
        let rrf = 1.0 / (k + (rank + 1) as f32);
        *scores.entry(hash.clone()).or_insert(0.0) += rrf;
    }

    let mut results: Vec<(String, f32)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}

/// Apply token budget: include results in rank order until budget exhausted.
/// Always includes at least 1 result.
fn apply_token_budget(
    results: Vec<SearchResult>,
    max_tokens: usize,
) -> Vec<SearchResult> {
    if results.is_empty() {
        return results;
    }

    let mut total_tokens: usize = 0;
    let mut budgeted = Vec::new();

    for r in results {
        let tokens = r.token_count as usize;

        if budgeted.is_empty() {
            // Always include at least 1 result
            total_tokens += tokens;
            budgeted.push(r);
        } else if total_tokens + tokens <= max_tokens {
            total_tokens += tokens;
            budgeted.push(r);
        } else {
            break;
        }
    }

    budgeted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_fusion_basic() {
        let vector = vec![
            ("aaa".to_string(), 0.9),
            ("bbb".to_string(), 0.8),
            ("ccc".to_string(), 0.7),
        ];
        let bm25 = vec![
            ("bbb".to_string(), 5.0),
            ("ddd".to_string(), 4.0),
            ("aaa".to_string(), 3.0),
        ];

        let fused = rrf_fusion(&vector, &bm25, 60.0);

        // Both aaa and bbb appear in both lists, so they should have higher scores
        let aaa_score = fused.iter().find(|(h, _)| h == "aaa").unwrap().1;
        let bbb_score = fused.iter().find(|(h, _)| h == "bbb").unwrap().1;
        let ddd_score = fused.iter().find(|(h, _)| h == "ddd").unwrap().1;

        assert!(bbb_score > ddd_score);
        assert!(aaa_score > ddd_score);
    }

    #[test]
    fn test_rrf_fusion_empty() {
        let fused = rrf_fusion(&[], &[], 60.0);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_token_budget_always_includes_one() {
        use lore_core::types::CommitDoc;

        let doc = CommitDoc {
            hash: "a".to_string(),
            short_hash: "a".to_string(),
            message: "big commit".to_string(),
            body: None,
            diff_summary: "x".repeat(10000),
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
        };

        let results = vec![SearchResult {
            commit: doc,
            relevance: 0.9,
            vector_score: 0.8,
            bm25_score: 0.5,
            rank: 1,
            matched_terms: vec![],
            token_count: 9999,
            rerank_score: None,
        }];

        let budgeted = apply_token_budget(results, 100);
        assert_eq!(budgeted.len(), 1); // Always at least 1
    }

    #[test]
    fn test_token_budget_limits() {
        use lore_core::types::CommitDoc;

        let make = |hash: &str, tokens: u32| {
            let doc = CommitDoc {
                hash: hash.to_string(),
                short_hash: hash.to_string(),
                message: "m".to_string(),
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
            };
            SearchResult {
                commit: doc,
                relevance: 0.5,
                vector_score: 0.3,
                bm25_score: 0.2,
                rank: 0,
                matched_terms: vec![],
                token_count: tokens,
                rerank_score: None,
            }
        };

        let results = vec![make("a", 100), make("b", 100), make("c", 100)];
        let budgeted = apply_token_budget(results, 200);
        assert_eq!(budgeted.len(), 2);
    }
}
