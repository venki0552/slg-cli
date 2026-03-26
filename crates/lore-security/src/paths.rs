use lore_core::errors::LoreError;
use std::path::PathBuf;

/// SECURITY: Sanitize branch name — only allow safe characters.
/// This is the ONLY way to construct index paths.
fn sanitize_branch_name(branch: &str) -> String {
    // Step 1: Remove all non-allowlisted characters
    // SECURITY: dots are replaced with '_' to prevent ".ssh", ".." from surviving
    let sanitized: String = branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else if c == '.' {
                '_'
            } else {
                '\0' // will be filtered
            }
        })
        .filter(|c| *c != '\0')
        .collect();

    // Step 2: Max 64 chars, truncate if longer
    let sanitized = if sanitized.len() > 64 {
        sanitized[..64].to_string()
    } else {
        sanitized
    };

    // Step 3: Reject dangerous names
    if sanitized.is_empty() || sanitized == "_" || sanitized == "__" {
        return "unknown-branch".to_string();
    }

    // Step 4: Reject names starting with "-"
    if sanitized.starts_with('-') {
        return format!("b_{}", sanitized.strip_prefix('-').unwrap_or(&sanitized));
    }

    sanitized
}

/// SECURITY: Build a safe index path that cannot escape ~/.lore/indices/<repo_hash>/.
/// This is the ONLY way to get an index path — no other code constructs paths.
pub fn safe_index_path(repo_hash: &str, branch_name: &str) -> Result<PathBuf, LoreError> {
    let sanitized = sanitize_branch_name(branch_name);

    // Build base path
    let base = indices_base(repo_hash);

    // Build candidate path
    let candidate = base.join(format!("{}.db", sanitized));

    // SECURITY: Validate candidate is under base
    // Use lexical comparison first, then verify after canonicalization if possible
    let candidate_str = candidate.to_string_lossy().replace('\\', "/");
    let base_str = base.to_string_lossy().replace('\\', "/");

    if !candidate_str.starts_with(&*base_str) {
        return Err(LoreError::PathTraversal(format!(
            "Path '{}' escapes index directory '{}'",
            candidate.display(),
            base.display()
        )));
    }

    // Additional check: no ".." components in the path
    for component in candidate.components() {
        if let std::path::Component::ParentDir = component {
            return Err(LoreError::PathTraversal(
                "Path contains parent directory reference (..)".to_string(),
            ));
        }
    }

    Ok(candidate)
}

/// Returns ~/.lore/ — creates with restricted permissions if not exists.
pub fn lore_home() -> PathBuf {
    let home = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".lore");

    if !home.exists() {
        let _ = std::fs::create_dir_all(&home);
        set_restricted_permissions(&home);
    }

    home
}

/// Returns ~/.lore/models/ — creates if not exists.
pub fn models_dir() -> PathBuf {
    let dir = lore_home().join("models");
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
    dir
}

/// Returns ~/.lore/indices/<repo_hash>/ — creates with restricted permissions if not exists.
pub fn indices_base(repo_hash: &str) -> PathBuf {
    // SECURITY: repo_hash is SHA256 hex, only safe chars
    let dir = lore_home().join("indices").join(repo_hash);
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
        set_restricted_permissions(&dir);
    }
    dir
}

/// Returns ~/.lore/security.log
pub fn security_log_path() -> PathBuf {
    lore_home().join("security.log")
}

/// Set directory permissions to owner-only (0o700 on Unix).
fn set_restricted_permissions(path: &PathBuf) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    // On Windows, the user's home directory already provides access control
    let _ = path; // suppress unused warning on Windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_branch_name() {
        assert_eq!(sanitize_branch_name("main"), "main");
        assert_eq!(sanitize_branch_name("feature/auth"), "featureauth");
        assert_eq!(sanitize_branch_name("fix-bug-123"), "fix-bug-123");
        assert_eq!(sanitize_branch_name("my_branch.v2"), "my_branch_v2");
    }

    #[test]
    fn test_path_traversal_branch_blocked() {
        let result = safe_index_path("abc123", "../../.ssh/authorized_keys").unwrap();
        let result_str = result.to_string_lossy();
        assert!(result_str.contains("abc123"));
        assert!(!result_str.contains(".ssh"));
        assert!(!result_str.contains(".."));
    }

    #[test]
    fn test_path_traversal_absolute_blocked() {
        let result = safe_index_path("abc123", "/etc/passwd").unwrap();
        let result_str = result.to_string_lossy();
        // Path must stay under the indices directory for repo abc123
        assert!(result_str.contains("abc123"));
        // The slash separators from the input must not survive as path components
        let file_name = result.file_name().unwrap().to_string_lossy();
        assert!(!file_name.contains('/'));
        assert!(!file_name.contains('\\'));
        // Must be a .db file inside the repo hash directory
        assert!(file_name.ends_with(".db"));
        assert!(result.parent().unwrap().ends_with("abc123"));
    }

    #[test]
    fn test_unicode_path_traversal_blocked() {
        let result = safe_index_path("abc123", "..%2F..%2F.ssh").unwrap();
        let result_str = result.to_string_lossy();
        assert!(result_str.contains("abc123"));
        assert!(!result_str.contains(".ssh"));
    }

    #[test]
    fn test_empty_branch_name_handled() {
        let result = safe_index_path("abc123", "").unwrap();
        let result_str = result.to_string_lossy();
        assert!(result_str.contains("unknown-branch"));
    }

    #[test]
    fn test_long_branch_name_truncated() {
        let long_name = "a".repeat(200);
        let sanitized = sanitize_branch_name(&long_name);
        assert!(sanitized.len() <= 64);
    }

    #[test]
    fn test_dot_branch_names_rejected() {
        assert_eq!(sanitize_branch_name("."), "unknown-branch");
        assert_eq!(sanitize_branch_name(".."), "unknown-branch");
    }

    #[test]
    fn test_dot_prefix_handled() {
        let result = sanitize_branch_name(".hidden");
        assert!(!result.starts_with('.'));
        assert!(result.starts_with('_'));
    }

    #[test]
    fn test_dash_prefix_handled() {
        let result = sanitize_branch_name("-badname");
        assert!(!result.starts_with('-'));
    }

    #[test]
    fn test_no_parent_dir_components() {
        let result = safe_index_path("abc123", "valid-name");
        assert!(result.is_ok());
        let path = result.unwrap();
        for component in path.components() {
            assert!(
                !matches!(component, std::path::Component::ParentDir),
                "Path should not contain parent directory references"
            );
        }
    }

    #[test]
    fn test_lore_home_path() {
        let home = lore_home();
        assert!(home.to_string_lossy().contains(".lore"));
    }

    #[test]
    fn test_security_log_path() {
        let path = security_log_path();
        assert!(path.to_string_lossy().contains("security.log"));
    }
}
