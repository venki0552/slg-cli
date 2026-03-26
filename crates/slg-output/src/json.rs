use slg_core::types::SearchResult;
use serde_json::json;

/// Format search results as JSON.
pub fn format_json(results: &[SearchResult], query: &str, latency_ms: u64) -> String {
    let total_tokens: u32 = results.iter().map(|r| r.token_count).sum();

    let json_results: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let risk = if r.commit.risk_score > 0.7 {
                "high"
            } else if r.commit.risk_score > 0.3 {
                "medium"
            } else {
                "low"
            };

            json!({
                "rank": r.rank,
                "relevance": format!("{:.2}", r.relevance),
                "vector_score": format!("{:.2}", r.vector_score),
                "bm25_score": format!("{:.2}", r.bm25_score),
                "token_count": r.token_count,
                "matched_terms": r.matched_terms,
                "metadata": {
                    "hash": r.commit.hash,
                    "short_hash": r.commit.short_hash,
                    "author": r.commit.author,
                    "timestamp": r.commit.timestamp,
                    "intent": format!("{}", r.commit.intent),
                    "risk_level": risk,
                    "injection_flagged": r.commit.injection_flagged,
                    "linked_issues": r.commit.linked_issues,
                },
                "data": {
                    "message": r.commit.message,
                    "body": r.commit.body,
                    "diff_summary": r.commit.diff_summary,
                    "files_changed": r.commit.files_changed,
                }
            })
        })
        .collect();

    let output = json!({
        "query": query,
        "count": results.len(),
        "total_tokens": total_tokens,
        "latency_ms": latency_ms,
        "results": json_results,
    });

    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use slg_core::types::{CommitDoc, CommitIntent};

    #[test]
    fn test_format_json() {
        let results = vec![SearchResult {
            commit: CommitDoc {
                hash: "abc123".to_string(),
                short_hash: "abc".to_string(),
                message: "fix: test".to_string(),
                body: None,
                diff_summary: "changed".to_string(),
                author: "Alice".to_string(),
                timestamp: 1700000000,
                files_changed: vec![],
                insertions: 0,
                deletions: 0,
                linked_issues: vec![],
                linked_prs: vec![],
                intent: CommitIntent::Fix,
                risk_score: 0.1,
                branch: "main".to_string(),
                injection_flagged: false,
                secrets_redacted: 0,
            },
            relevance: 0.9,
            vector_score: 0.8,
            bm25_score: 0.5,
            rank: 1,
            matched_terms: vec!["fix".to_string()],
            token_count: 25,
            rerank_score: None,
        }];

        let json_str = format_json(&results, "why?", 10);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["query"], "why?");
        assert_eq!(parsed["results"][0]["rank"], 1);
    }
}
