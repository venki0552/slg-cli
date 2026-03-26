use serde::{Deserialize, Serialize};
use std::fmt;

/// Core indexed unit representing a single git commit.
/// All fields are sanitized before storage — raw data never stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDoc {
    /// Full 40-char SHA hash
    pub hash: String,
    /// 7-char display hash
    pub short_hash: String,
    /// Sanitized commit subject line
    pub message: String,
    /// Sanitized full message body
    pub body: Option<String>,
    /// Per-file intent summaries, NOT raw diff
    pub diff_summary: String,
    /// Author display name only — email NEVER stored
    pub author: String,
    /// Unix epoch seconds
    pub timestamp: i64,
    /// File paths touched by this commit
    pub files_changed: Vec<String>,
    /// Lines added
    pub insertions: u32,
    /// Lines removed
    pub deletions: u32,
    /// Parsed from "fixes #234", "closes #45"
    pub linked_issues: Vec<String>,
    /// Parsed from "PR #123"
    pub linked_prs: Vec<String>,
    /// Detected from message prefix + diff
    pub intent: CommitIntent,
    /// 0.0–1.0, computed from file sensitivity + churn + deletion ratio
    pub risk_score: f32,
    /// Which branch this was indexed from
    pub branch: String,
    /// Scanner detected potential injection
    pub injection_flagged: bool,
    /// Count of secrets redacted (not what they were)
    pub secrets_redacted: u32,
}

/// Detected intent from commit message prefix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommitIntent {
    Fix,
    Feature,
    Refactor,
    Perf,
    Security,
    Docs,
    Test,
    Chore,
    Revert,
    Unknown,
}

impl CommitIntent {
    /// Detect intent from a commit message.
    pub fn from_message(message: &str) -> Self {
        let lower = message.trim().to_lowercase();

        // Check prefix patterns with colon
        let colon_prefixes: &[(&[&str], CommitIntent)] = &[
            (&["fix:", "bugfix:", "hotfix:", "patch:"], CommitIntent::Fix),
            (
                &["feat:", "feature:", "add:", "new:"],
                CommitIntent::Feature,
            ),
            (
                &["refactor:", "cleanup:", "reorganize:"],
                CommitIntent::Refactor,
            ),
            (
                &["perf:", "performance:", "optimize:", "speed:"],
                CommitIntent::Perf,
            ),
            (
                &["security:", "sec:", "vuln:", "cve:"],
                CommitIntent::Security,
            ),
            (
                &["docs:", "doc:", "readme:", "comment:"],
                CommitIntent::Docs,
            ),
            (&["test:", "spec:", "coverage:"], CommitIntent::Test),
            (
                &["chore:", "build:", "ci:", "deps:", "bump:", "style:"],
                CommitIntent::Chore,
            ),
            (&["revert:", "rollback:", "undo:"], CommitIntent::Revert),
        ];

        for (prefixes, intent) in colon_prefixes {
            for prefix in *prefixes {
                if lower.starts_with(prefix) {
                    return intent.clone();
                }
            }
        }

        // Check scoped conventional commit: "feat(scope):", "fix(scope):"
        if let Some(paren_pos) = lower.find('(') {
            let before_paren = &lower[..paren_pos];
            let after_close = lower.find("):").map(|p| p + 2);
            if after_close.is_some() {
                let colon_check = format!("{}:", before_paren);
                for (prefixes, intent) in colon_prefixes {
                    for prefix in *prefixes {
                        if colon_check == *prefix {
                            return intent.clone();
                        }
                    }
                }
            }
        }

        // Check word-based patterns (without colon)
        let word_prefixes: &[(&[&str], CommitIntent)] = &[
            (&["fix ", "fixed ", "fixes "], CommitIntent::Fix),
            (
                &["add ", "added ", "adds ", "implement ", "implemented "],
                CommitIntent::Feature,
            ),
            (
                &[
                    "update ",
                    "updated ",
                    "refactor ",
                    "refactored ",
                    "clean up ",
                ],
                CommitIntent::Refactor,
            ),
            (
                &["remove ", "removed ", "delete ", "deleted "],
                CommitIntent::Refactor,
            ),
            (&["revert ", "reverted "], CommitIntent::Revert),
        ];

        for (prefixes, intent) in word_prefixes {
            for prefix in *prefixes {
                if lower.starts_with(prefix) {
                    return intent.clone();
                }
            }
        }

        CommitIntent::Unknown
    }
}

impl fmt::Display for CommitIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommitIntent::Fix => write!(f, "Fix"),
            CommitIntent::Feature => write!(f, "Feature"),
            CommitIntent::Refactor => write!(f, "Refactor"),
            CommitIntent::Perf => write!(f, "Perf"),
            CommitIntent::Security => write!(f, "Security"),
            CommitIntent::Docs => write!(f, "Docs"),
            CommitIntent::Test => write!(f, "Test"),
            CommitIntent::Chore => write!(f, "Chore"),
            CommitIntent::Revert => write!(f, "Revert"),
            CommitIntent::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Query output unit — a search result with scoring metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Full sanitized commit
    pub commit: CommitDoc,
    /// 0.0–1.0, final fused RRF score
    pub relevance: f32,
    /// Raw semantic similarity score
    pub vector_score: f32,
    /// Raw lexical match score
    pub bm25_score: f32,
    /// Final rank position (1-based)
    pub rank: u32,
    /// BM25 terms that matched
    pub matched_terms: Vec<String>,
    /// Estimated tokens this result consumes
    pub token_count: u32,
    /// Cross-encoder score if reranker used
    pub rerank_score: Option<f32>,
}

/// Per-branch index state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    /// SHA256 of git remote URL (stable repo ID)
    pub repo_hash: String,
    /// Sanitized branch name
    pub branch: String,
    /// "main" or "master"
    pub base_branch: String,
    /// Total indexed commits
    pub commit_count: u64,
    /// Hash of newest indexed commit
    pub last_commit: String,
    /// When index was created (unix epoch)
    pub indexed_at: i64,
    /// When index was last queried (for cleanup)
    pub last_accessed: i64,
    /// Embedding model used
    pub model_version: String,
    /// Schema version for migrations
    pub index_version: u32,
    /// Storage used by this index
    pub size_bytes: u64,
    /// True if this is a branch delta over main
    pub is_delta: bool,
}

/// Output format for CLI results.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Xml,
    Json,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Text => write!(f, "text"),
            OutputFormat::Xml => write!(f, "xml"),
            OutputFormat::Json => write!(f, "json"),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(OutputFormat::Text),
            "xml" => Ok(OutputFormat::Xml),
            "json" => Ok(OutputFormat::Json),
            _ => Err(format!(
                "Unknown output format: '{}'. Use text, xml, or json.",
                s
            )),
        }
    }
}

/// Risk level derived from risk score.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl From<f32> for RiskLevel {
    fn from(score: f32) -> Self {
        if score < 0.3 {
            RiskLevel::Low
        } else if score < 0.7 {
            RiskLevel::Medium
        } else {
            RiskLevel::High
        }
    }
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_intent_from_message() {
        assert_eq!(
            CommitIntent::from_message("fix: resolve crash"),
            CommitIntent::Fix
        );
        assert_eq!(
            CommitIntent::from_message("feat: add login"),
            CommitIntent::Feature
        );
        assert_eq!(
            CommitIntent::from_message("feat(auth): add login"),
            CommitIntent::Feature
        );
        assert_eq!(
            CommitIntent::from_message("refactor: clean up code"),
            CommitIntent::Refactor
        );
        assert_eq!(
            CommitIntent::from_message("perf: optimize query"),
            CommitIntent::Perf
        );
        assert_eq!(
            CommitIntent::from_message("security: patch XSS"),
            CommitIntent::Security
        );
        assert_eq!(
            CommitIntent::from_message("docs: update README"),
            CommitIntent::Docs
        );
        assert_eq!(
            CommitIntent::from_message("test: add unit tests"),
            CommitIntent::Test
        );
        assert_eq!(
            CommitIntent::from_message("chore: update deps"),
            CommitIntent::Chore
        );
        assert_eq!(
            CommitIntent::from_message("revert: undo last change"),
            CommitIntent::Revert
        );
        assert_eq!(
            CommitIntent::from_message("build: fix CI"),
            CommitIntent::Chore
        );
        assert_eq!(
            CommitIntent::from_message("ci: add workflow"),
            CommitIntent::Chore
        );
        assert_eq!(
            CommitIntent::from_message("fix login crash"),
            CommitIntent::Fix
        );
        assert_eq!(
            CommitIntent::from_message("add new feature"),
            CommitIntent::Feature
        );
        assert_eq!(
            CommitIntent::from_message("random message"),
            CommitIntent::Unknown
        );
    }

    #[test]
    fn test_risk_level_from_score() {
        assert_eq!(RiskLevel::from(0.0), RiskLevel::Low);
        assert_eq!(RiskLevel::from(0.29), RiskLevel::Low);
        assert_eq!(RiskLevel::from(0.3), RiskLevel::Medium);
        assert_eq!(RiskLevel::from(0.69), RiskLevel::Medium);
        assert_eq!(RiskLevel::from(0.7), RiskLevel::High);
        assert_eq!(RiskLevel::from(1.0), RiskLevel::High);
    }

    #[test]
    fn test_output_format_parse() {
        assert_eq!("text".parse::<OutputFormat>().unwrap(), OutputFormat::Text);
        assert_eq!("xml".parse::<OutputFormat>().unwrap(), OutputFormat::Xml);
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!("XML".parse::<OutputFormat>().unwrap(), OutputFormat::Xml);
        assert!("invalid".parse::<OutputFormat>().is_err());
    }
}
