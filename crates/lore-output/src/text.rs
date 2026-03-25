use lore_core::types::SearchResult;

/// Format search results as human-readable text for terminal output.
pub fn format_text(results: &[SearchResult], query: &str, latency_ms: u64) -> String {
    if results.is_empty() {
        return format!("No results found for: {}\n", query);
    }

    let mut out = String::with_capacity(2048);
    out.push_str(&format!(
        "Query: {} ({} results, {}ms)\n",
        query,
        results.len(),
        latency_ms
    ));
    out.push_str(&"─".repeat(60));
    out.push('\n');

    for r in results {
        let risk = if r.commit.risk_score > 0.7 {
            "HIGH"
        } else if r.commit.risk_score > 0.3 {
            "MEDIUM"
        } else {
            "LOW"
        };

        out.push_str(&format!(
            "\n#{} [relevance: {:.2}] {} {}\n",
            r.rank, r.relevance, r.commit.short_hash, r.commit.message
        ));
        out.push_str(&format!(
            "   Author: {} | Intent: {} | Risk: {}\n",
            r.commit.author, r.commit.intent, risk
        ));

        let ts = chrono::DateTime::from_timestamp(r.commit.timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| r.commit.timestamp.to_string());
        out.push_str(&format!("   Date:   {}\n", ts));

        if !r.commit.files_changed.is_empty() {
            let files: Vec<&str> = r
                .commit
                .files_changed
                .iter()
                .take(5)
                .map(|s| s.as_str())
                .collect();
            out.push_str(&format!("   Files:  {}\n", files.join(", ")));
            if r.commit.files_changed.len() > 5 {
                out.push_str(&format!(
                    "           ... and {} more\n",
                    r.commit.files_changed.len() - 5
                ));
            }
        }

        if !r.commit.linked_issues.is_empty() {
            out.push_str(&format!(
                "   Issues: #{}\n",
                r.commit.linked_issues.join(", #")
            ));
        }

        if !r.commit.diff_summary.is_empty() {
            // Truncate diff for display
            let diff = if r.commit.diff_summary.len() > 200 {
                format!("{}...", &r.commit.diff_summary[..200])
            } else {
                r.commit.diff_summary.clone()
            };
            out.push_str(&format!("   Diff:   {}\n", diff));
        }

        if r.commit.injection_flagged {
            out.push_str("   ⚠ INJECTION FLAGGED\n");
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lore_core::types::{CommitDoc, CommitIntent};

    #[test]
    fn test_format_text() {
        let results = vec![SearchResult {
            commit: CommitDoc {
                hash: "abc123def456".to_string(),
                short_hash: "abc123d".to_string(),
                message: "fix: crash on login".to_string(),
                body: None,
                diff_summary: "modified auth module".to_string(),
                author: "Alice".to_string(),
                timestamp: 1700000000,
                files_changed: vec!["src/auth.rs".to_string()],
                insertions: 5,
                deletions: 2,
                linked_issues: vec!["42".to_string()],
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
            matched_terms: vec!["crash".to_string()],
            token_count: 50,
            rerank_score: None,
        }];

        let text = format_text(&results, "why crash?", 42);
        assert!(text.contains("abc123d"));
        assert!(text.contains("crash on login"));
        assert!(text.contains("Alice"));
        assert!(text.contains("#42"));
    }

    #[test]
    fn test_format_text_empty() {
        let text = format_text(&[], "test", 10);
        assert!(text.contains("No results found"));
    }
}
