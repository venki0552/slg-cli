use sha2::{Digest, Sha256};
use slg_core::errors::SlgError;
use std::path::{Path, PathBuf};

/// Walk up from `start` to find the directory containing `.git/`.
/// Returns Err(NotAGitRepo) if no `.git/` found at filesystem root.
pub fn find_git_root(start: &Path) -> Result<PathBuf, SlgError> {
    let mut current = start.to_path_buf();
    if current.is_file() {
        current = current.parent().map(|p| p.to_path_buf()).unwrap_or(current);
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
                return Err(SlgError::NotAGitRepo);
            }
        }
    }
}

/// Read current branch from git repo.
/// Returns "HEAD-DETACHED-{short_hash}" for detached HEAD.
pub fn get_current_branch(repo_path: &Path) -> Result<String, SlgError> {
    let repo = git2::Repository::open(repo_path)
        .map_err(|e| SlgError::Git(format!("Failed to open repo: {}", e)))?;

    let head = repo
        .head()
        .map_err(|e| SlgError::Git(format!("Failed to read HEAD: {}", e)))?;

    if head.is_branch() {
        if let Some(name) = head.shorthand() {
            return Ok(name.to_string());
        }
    }

    // Detached HEAD — return short hash
    let oid = head
        .target()
        .ok_or_else(|| SlgError::Git("HEAD has no target".to_string()))?;
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

/// Resolve a git ref (HEAD, HEAD~N, branch name, tag, short hash) to a full commit SHA.
/// Uses git2's revparse to handle all ref formats.
pub fn resolve_ref(repo_path: &Path, refspec: &str) -> Result<String, SlgError> {
    let repo = git2::Repository::open(repo_path)
        .map_err(|e| SlgError::Git(format!("Failed to open repo: {}", e)))?;

    let obj = repo
        .revparse_single(refspec)
        .map_err(|e| SlgError::Git(format!("Failed to resolve ref '{}': {}", refspec, e)))?;

    let commit = obj.peel_to_commit().map_err(|e| {
        SlgError::Git(format!(
            "Ref '{}' does not point to a commit: {}",
            refspec, e
        ))
    })?;

    Ok(commit.id().to_string())
}

/// List commit hashes in the range base..head (exclusive of base, inclusive of head).
/// Returns commits in reverse chronological order.
pub fn list_commits_in_range(
    repo_path: &Path,
    base: &str,
    head: &str,
) -> Result<Vec<String>, SlgError> {
    let repo = git2::Repository::open(repo_path)
        .map_err(|e| SlgError::Git(format!("Failed to open repo: {}", e)))?;

    let base_oid = git2::Oid::from_str(base)
        .map_err(|e| SlgError::Git(format!("Invalid base hash '{}': {}", base, e)))?;
    let head_oid = git2::Oid::from_str(head)
        .map_err(|e| SlgError::Git(format!("Invalid head hash '{}': {}", head, e)))?;

    let mut revwalk = repo
        .revwalk()
        .map_err(|e| SlgError::Git(format!("Failed to create revwalk: {}", e)))?;
    revwalk
        .push(head_oid)
        .map_err(|e| SlgError::Git(format!("Failed to push head oid: {}", e)))?;
    revwalk
        .hide(base_oid)
        .map_err(|e| SlgError::Git(format!("Failed to hide base oid: {}", e)))?;

    let mut commits = Vec::new();
    for oid_result in revwalk {
        let oid = oid_result.map_err(|e| SlgError::Git(format!("Revwalk error: {}", e)))?;
        commits.push(oid.to_string());
    }

    Ok(commits)
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
