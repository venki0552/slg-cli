use lore_core::errors::LoreError;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Walk up from `start` to find the directory containing `.git/`.
/// Returns Err(NotAGitRepo) if no `.git/` found at filesystem root.
pub fn find_git_root(start: &Path) -> Result<PathBuf, LoreError> {
    let mut current = start.to_path_buf();
    if current.is_file() {
        current = current
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(current);
    }

    loop {
        if current.join(".git").exists() {
            return Ok(current);
        }
        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => {
                return Err(LoreError::NotAGitRepo);
            }
        }
    }
}

/// Read current branch from git repo.
/// Returns "HEAD-DETACHED-{short_hash}" for detached HEAD.
pub fn get_current_branch(repo_path: &Path) -> Result<String, LoreError> {
    let repo = git2::Repository::open(repo_path)
        .map_err(|e| LoreError::Git(format!("Failed to open repo: {}", e)))?;

    let head = repo
        .head()
        .map_err(|e| LoreError::Git(format!("Failed to read HEAD: {}", e)))?;

    if head.is_branch() {
        if let Some(name) = head.shorthand() {
            return Ok(name.to_string());
        }
    }

    // Detached HEAD — return short hash
    let oid = head.target().ok_or_else(|| {
        LoreError::Git("HEAD has no target".to_string())
    })?;
    let short = &oid.to_string()[..7];
    Ok(format!("HEAD-DETACHED-{}", short))
}

/// Get the remote "origin" URL if available.
pub fn get_remote_url(repo_path: &Path) -> Option<String> {
    let repo = git2::Repository::open(repo_path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    remote.url().map(|s| s.to_string())
}

/// Compute a stable repo identifier.
/// If remote URL found: SHA256(remote_url), hex-encoded lowercase.
/// If no remote: SHA256(absolute_repo_path), hex-encoded lowercase.
pub fn compute_repo_hash(repo_path: &Path) -> String {
    let input = match get_remote_url(repo_path) {
        Some(url) => url,
        None => repo_path
            .canonicalize()
            .unwrap_or_else(|_| repo_path.to_path_buf())
            .to_string_lossy()
            .to_string(),
    };

    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Detect the base branch for a repo ("main" or "master").
pub fn detect_base_branch(repo_path: &Path) -> String {
    let repo = match git2::Repository::open(repo_path) {
        Ok(r) => r,
        Err(_) => return "main".to_string(),
    };

    if repo.find_branch("main", git2::BranchType::Local).is_ok() {
        return "main".to_string();
    }
    if repo.find_branch("master", git2::BranchType::Local).is_ok() {
        return "master".to_string();
    }
    "main".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, git2::Repository) {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();

        // Create initial commit so HEAD exists
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree_oid = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Initial commit",
            &repo.find_tree(tree_oid).unwrap(),
            &[],
        )
        .unwrap();

        (dir, repo)
    }

    #[test]
    fn test_find_git_root() {
        let (dir, _repo) = create_test_repo();
        let result = find_git_root(dir.path());
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn test_find_git_root_not_found() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("not_a_repo");
        std::fs::create_dir_all(&sub).unwrap();
        let result = find_git_root(&sub);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_current_branch() {
        let (dir, _repo) = create_test_repo();
        let branch = get_current_branch(dir.path()).unwrap();
        // Should be "main" or "master" depending on git config
        assert!(!branch.is_empty());
    }

    #[test]
    fn test_compute_repo_hash_stable() {
        let (dir, _repo) = create_test_repo();
        let hash1 = compute_repo_hash(dir.path());
        let hash2 = compute_repo_hash(dir.path());
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 hex
    }

    #[test]
    fn test_detect_base_branch() {
        let (dir, _repo) = create_test_repo();
        let branch = detect_base_branch(dir.path());
        // Default branch: could be "main" or "master"
        assert!(branch == "main" || branch == "master");
    }
}
