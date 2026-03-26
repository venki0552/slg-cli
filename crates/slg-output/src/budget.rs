use slg_core::types::SearchResult;

/// Estimate token count for a text string.
/// Uses ~4 chars per token heuristic (approximates cl100k_base).
pub fn count_tokens(text: &str) -> u32 {
    (text.len() as f64 / 4.0).ceil() as u32
}

/// Estimate total tokens for a search result.
pub fn estimate_result_tokens(result: &SearchResult) -> u32 {
    let text_len = result.commit.message.len()
        + result.commit.body.as_ref().map_or(0, |b| b.len())
        + result.commit.diff_summary.len()
        + result.commit.author.len()
        + result.commit.hash.len();
    count_tokens(&"x".repeat(text_len))
}

/// Apply token budget: include results in rank order until budget exhausted.
/// Always includes at least 1 result (even if it exceeds budget).
/// Updates token_count fields.
pub fn apply_token_budget(mut results: Vec<SearchResult>, max_tokens: usize) -> Vec<SearchResult> {
    if results.is_empty() {
        return results;
    }

    // Update token counts
    for r in &mut results {
        r.token_count = estimate_result_tokens(r);
    }

    let mut total: usize = 0;
    let mut budgeted = Vec::new();

    for r in results {
        let tokens = r.token_count as usize;

        if budgeted.is_empty() || total + tokens <= max_tokens {
            total += tokens;
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
    use slg_core::types::{CommitDoc, CommitIntent};

    fn make_result(msg_len: usize, rank: u32) -> SearchResult {
        SearchResult {
            commit: CommitDoc {
                hash: "abc123".to_string(),
                short_hash: "abc".to_string(),
                message: "x".repeat(msg_len),
                body: None,
                diff_summary: String::new(),
                author: "A".to_string(),
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
            },
            relevance: 0.5,
            vector_score: 0.3,
            bm25_score: 0.2,
            rank,
            matched_terms: vec![],
            token_count: 0,
            rerank_score: None,
        }
    }

    #[test]
    fn test_count_tokens() {
        assert_eq!(count_tokens("hello world"), 3); // 11 chars / 4 = 2.75 → 3
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn test_budget_always_one() {
        let results = vec![make_result(10000, 1)];
        let budgeted = apply_token_budget(results, 10);
        assert_eq!(budgeted.len(), 1);
    }

    #[test]
    fn test_budget_limits() {
        let results = vec![
            make_result(100, 1),
            make_result(100, 2),
            make_result(100, 3),
        ];
        let budgeted = apply_token_budget(results, 70);
        // Each result ~(100 + 1 + 6)/4 ≈ 27 tokens. Budget 70 → fits 2
        assert_eq!(budgeted.len(), 2);
    }
}
