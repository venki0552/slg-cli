use git2::{Commit, DiffOptions, Repository, Sort};
use regex::Regex;
use slg_core::errors::SlgError;
use slg_core::types::{CommitDoc, CommitIntent};
use slg_security::redactor::SecretRedactor;
use slg_security::sanitizer::CommitSanitizer;
use tokio::sync::mpsc;
use tracing::debug;

/// Index all commits on a branch, streaming results through channel.
/// Returns the count of indexed commits.
pub async fn index_full_branch(
    repo_path: &std::path::Path,
    branch: &str,
    sanitizer: &CommitSanitizer,
    redactor: &SecretRedactor,
    tx: mpsc::Sender<CommitDoc>,
) -> Result<u64, SlgError> {
    let repo = Repository::open(repo_path)
        .map_err(|e| SlgError::Git(format!("Failed to open repo: {}", e)))?;

    let reference = repo
        .find_reference(&format!("refs/heads/{}", branch))
        .map_err(|e| SlgError::Git(format!("Branch '{}' not found: {}", branch, e)))?;

    let oid = reference
        .target()
        .ok_or_else(|| SlgError::Git(format!("Branch '{}' has no target", branch)))?;

    let mut revwalk = repo
        .revwalk()
        .map_err(|e| SlgError::Git(format!("Failed to create revwalk: {}", e)))?;
    revwalk
        .push(oid)
        .map_err(|e| SlgError::Git(format!("Failed to push oid to revwalk: {}", e)))?;
    revwalk
        .set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .map_err(|e| SlgError::Git(format!("Failed to set sorting: {}", e)))?;

    let mut count: u64 = 0;

    for oid_result in revwalk {
        let oid = oid_result.map_err(|e| SlgError::Git(format!("Revwalk error: {}", e)))?;

        let commit = repo
            .find_commit(oid)
            .map_err(|e| SlgError::Git(format!("Failed to find commit {}: {}", oid, e)))?;

        if skip_binary_commit(&repo, &commit) {
            continue;
        }

        let raw_doc = build_raw_commit_doc(&repo, &commit, branch)?;
        let mut sanitized = sanitizer.sanitize(raw_doc);

        // SECURITY: Redact secrets from diff_summary before storage
        let (redacted_diff, secret_count) = redactor.redact(&sanitized.diff_summary);
        if secret_count > 0 {
            sanitized.diff_summary = redacted_diff;
            sanitized.secrets_redacted += secret_count;
        }

        // Also redact from body
        if let Some(body) = &sanitized.body {
            let (redacted_body, body_secrets) = redactor.redact(body);
            if body_secrets > 0 {
                sanitized.body = Some(redacted_body);
                sanitized.secrets_redacted += body_secrets;
            }
        }

        if tx.send(sanitized).await.is_err() {
            debug!("Receiver dropped, stopping ingestion");
            break;
        }

        count += 1;
    }

    Ok(count)
}

/// Build a raw CommitDoc from a git2 commit (before sanitization).
pub fn build_raw_commit_doc(
    repo: &Repository,
    commit: &Commit,
    branch: &str,
) -> Result<CommitDoc, SlgError> {
    let hash = commit.id().to_string();
    let short_hash = hash[..7.min(hash.len())].to_string();
    let message = commit.summary().unwrap_or("").to_string();
    let body = commit.body().map(|s| s.to_string());
    let author = commit.author().name().unwrap_or("unknown").to_string();
    let timestamp = commit.time().seconds();
    let full_message = format!(
        "{}{}",
        message,
        body.as_deref()
            .map(|b| format!("\n{}", b))
            .unwrap_or_default()
    );

    let (files_changed, insertions, deletions, diff_summary) = build_diff_info(repo, commit)?;

    let linked_issues = parse_issue_refs(&full_message);
    let linked_prs = parse_pr_refs(&full_message);
    let intent = CommitIntent::from_message(&message);
    let risk_score = calculate_risk_score(&files_changed, insertions, deletions);

    Ok(CommitDoc {
        hash,
        short_hash,
        message,
        body,
        diff_summary,
        author,
        timestamp,
        files_changed,
        insertions,
        deletions,
        linked_issues,
        linked_prs,
        intent,
        risk_score,
        branch: branch.to_string(),
        injection_flagged: false,
        secrets_redacted: 0,
    })
}

/// Build diff info: (files_changed, insertions, deletions, diff_summary).
fn build_diff_info(
    repo: &Repository,
    commit: &Commit,
) -> Result<(Vec<String>, u32, u32, String), SlgError> {
    let tree = commit
        .tree()
        .map_err(|e| SlgError::Git(format!("Failed to get tree: {}", e)))?;

    let parent_tree = if commit.parent_count() > 0 {
        commit.parent(0).ok().and_then(|p| p.tree().ok())
    } else {
        None
    };

    let mut opts = DiffOptions::new();
    opts.context_lines(0);

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
        .map_err(|e| SlgError::Git(format!("Failed to create diff: {}", e)))?;

    let stats = diff
        .stats()
        .map_err(|e| SlgError::Git(format!("Failed to get diff stats: {}", e)))?;

    let insertions = stats.insertions() as u32;
    let deletions = stats.deletions() as u32;

    let mut files = Vec::new();
    let mut summary_parts = Vec::new();

    diff.foreach(
        &mut |delta, _| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());

            let status = match delta.status() {
                git2::Delta::Added => "added",
                git2::Delta::Deleted => "removed",
                git2::Delta::Modified => "modified",
                git2::Delta::Renamed => "renamed",
                git2::Delta::Copied => "copied",
                _ => "changed",
            };

            if !files.contains(&path) {
                files.push(path.clone());
            }

            if summary_parts.len() < 20 {
                let entry = format!("{}: {}", path, status);
                // Max 100 chars per file summary
                let truncated = if entry.len() > 100 {
                    format!("{}...", &entry[..97])
                } else {
                    entry
                };
                summary_parts.push(truncated);
            }

            true
        },
        None,
        None,
        None,
    )
    .map_err(|e| SlgError::Git(format!("Diff iteration failed: {}", e)))?;

    let extra = files.len().saturating_sub(20);
    let mut summary = summary_parts.join("\n");
    if extra > 0 {
        summary.push_str(&format!("\n...and {} more files", extra));
    }

    // Total max: 2000 chars
    if summary.len() > 2000 {
        let mut end = 2000;
        while !summary.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        summary.truncate(end);
    }

    Ok((files, insertions, deletions, summary))
}

/// Parse issue references from text.
pub fn parse_issue_refs(text: &str) -> Vec<String> {
    let re = Regex::new(r"(?:(?:fix(?:e[sd])?|close[sd]?|resolve[sd]?|refs?)\s+)?#(\d+)")
        .unwrap_or_else(|_| Regex::new(r"#(\d+)").unwrap());

    let mut issues: Vec<String> = re
        .captures_iter(text)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect();
    issues.sort();
    issues.dedup();
    issues
}

/// Parse PR references from text (GitHub-style).
fn parse_pr_refs(text: &str) -> Vec<String> {
    // PRs are typically referenced as "PR #123" or "(#123)"
    let re = Regex::new(r"(?i)(?:PR\s+)?#(\d+)").unwrap_or_else(|_| Regex::new(r"#(\d+)").unwrap());

    let mut prs: Vec<String> = re
        .captures_iter(text)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect();
    prs.sort();
    prs.dedup();
    prs
}

/// Calculate risk score based on file paths and change size.
/// Returns 0.0–1.0.
pub fn calculate_risk_score(files: &[String], insertions: u32, deletions: u32) -> f32 {
    let mut score: f32 = 0.0;

    for file in files {
        let lower = file.to_lowercase();
        if lower.contains("auth/")
            || lower.contains("security/")
            || lower.contains("crypto/")
            || lower.contains("cert/")
            || lower.contains("token/")
        {
            score += 0.3;
        } else if lower.contains("config/")
            || lower.contains("settings/")
            || lower.contains("env/")
            || lower.contains(".env")
        {
            score += 0.2;
        } else if lower.contains("test/") || lower.contains("spec/") || lower.contains("tests/") {
            score -= 0.2;
        } else if lower.contains("docs/") || lower.contains("readme") || lower.contains("doc/") {
            score -= 0.3;
        }
    }

    // Change size modifiers
    if deletions > insertions * 2 {
        score += 0.2;
    }
    if insertions + deletions > 500 {
        score += 0.1;
    }

    score.clamp(0.0, 1.0)
}

/// Return true if commit only touches binary files.
pub fn skip_binary_commit(repo: &Repository, commit: &Commit) -> bool {
    let tree = match commit.tree() {
        Ok(t) => t,
        Err(_) => return false,
    };

    let parent_tree = if commit.parent_count() > 0 {
        commit.parent(0).ok().and_then(|p| p.tree().ok())
    } else {
        None
    };

    let diff = match repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None) {
        Ok(d) => d,
        Err(_) => return false,
    };

    let mut has_text = false;
    let mut has_any = false;

    let _ = diff.foreach(
        &mut |delta, _| {
            has_any = true;
            if !delta.new_file().is_binary() && !delta.old_file().is_binary() {
                has_text = true;
                return false; // stop iterating
            }
            true
        },
        None,
        None,
        None,
    );

    has_any && !has_text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_issue_refs() {
        let text = "fix: resolve login crash. Fixes #123. Also refs #45";
        let issues = parse_issue_refs(text);
        assert!(issues.contains(&"123".to_string()));
        assert!(issues.contains(&"45".to_string()));
    }

    #[test]
    fn test_parse_issue_refs_dedup() {
        let text = "Fixes #99. Closes #99. #99 again.";
        let issues = parse_issue_refs(text);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0], "99");
    }

    #[test]
    fn test_calculate_risk_score_security() {
        let files = vec!["src/auth/token.rs".to_string()];
        let score = calculate_risk_score(&files, 10, 5);
        assert!(score > 0.0);
    }

    #[test]
    fn test_calculate_risk_score_docs() {
        let files = vec!["docs/README.md".to_string()];
        let score = calculate_risk_score(&files, 10, 5);
        assert_eq!(score, 0.0); // clamped at 0.0
    }

    #[test]
    fn test_calculate_risk_score_large_deletion() {
        let files = vec!["src/main.rs".to_string()];
        let score = calculate_risk_score(&files, 10, 100);
        assert!(score >= 0.2);
    }

    #[test]
    fn test_calculate_risk_score_clamped() {
        let files = vec!["docs/readme.md".to_string(), "docs/guide.md".to_string()];
        let score = calculate_risk_score(&files, 1, 1);
        assert_eq!(score, 0.0); // clamped, not negative
    }
}
