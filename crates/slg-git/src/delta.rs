use git2::Repository;
use slg_core::errors::SlgError;

/// Get commits in `feature_branch` not in `base_branch`.
/// Uses merge-base to find the divergence point.
pub fn get_delta_commits(
    repo: &Repository,
    base_branch: &str,
    feature_branch: &str,
) -> Result<Vec<String>, SlgError> {
    let base_ref = repo
        .find_reference(&format!("refs/heads/{}", base_branch))
        .map_err(|e| SlgError::Git(format!("Base branch '{}' not found: {}", base_branch, e)))?;

    let feature_ref = repo
        .find_reference(&format!("refs/heads/{}", feature_branch))
        .map_err(|e| {
            SlgError::Git(format!(
                "Feature branch '{}' not found: {}",
                feature_branch, e
            ))
        })?;

    let base_oid = base_ref
        .target()
        .ok_or_else(|| SlgError::Git("Base branch has no target".to_string()))?;
    let feature_oid = feature_ref
        .target()
        .ok_or_else(|| SlgError::Git("Feature branch has no target".to_string()))?;

    let merge_base = repo
        .merge_base(base_oid, feature_oid)
        .map_err(|e| SlgError::Git(format!("Failed to find merge base: {}", e)))?;

    let mut revwalk = repo
        .revwalk()
        .map_err(|e| SlgError::Git(format!("Failed to create revwalk: {}", e)))?;
    revwalk
        .push(feature_oid)
        .map_err(|e| SlgError::Git(format!("Failed to push feature oid: {}", e)))?;
    revwalk
        .hide(merge_base)
        .map_err(|e| SlgError::Git(format!("Failed to hide merge base: {}", e)))?;

    let mut commits = Vec::new();
    for oid_result in revwalk {
        let oid = oid_result.map_err(|e| SlgError::Git(format!("Revwalk error: {}", e)))?;
        commits.push(oid.to_string());
    }

    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_repo_with_branches() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();

        // Create initial commit on main
        let tree_oid = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let main_commit = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Initial commit",
                &repo.find_tree(tree_oid).unwrap(),
                &[],
            )
            .unwrap();

        // Scope borrows so we can move repo out
        {
            let base_commit = repo.find_commit(main_commit).unwrap();
            repo.branch("feature", &base_commit, false).unwrap();

            // Add a commit on feature branch
            repo.set_head("refs/heads/feature").unwrap();
            let tree2_oid = repo.index().unwrap().write_tree().unwrap();
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Feature commit",
                &repo.find_tree(tree2_oid).unwrap(),
                &[&base_commit],
            )
            .unwrap();
        }

        // Switch back to main
        repo.set_head("refs/heads/main").unwrap_or_else(|_| {
            repo.set_head("refs/heads/master").unwrap();
        });

        (dir, repo)
    }

    #[test]
    fn test_delta_commits() {
        let (_dir, repo) = create_repo_with_branches();
        // Determine base branch name
        let base = if repo.find_reference("refs/heads/main").is_ok() {
            "main"
        } else {
            "master"
        };

        let deltas = get_delta_commits(&repo, base, "feature").unwrap();
        assert_eq!(deltas.len(), 1, "Feature branch has 1 commit ahead of base");
    }
}
