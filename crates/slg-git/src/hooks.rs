use slg_core::errors::SlgError;
use std::path::Path;
use tracing::debug;

const HOOK_HEADER: &str = "# slg semantic index hook — slg.sh — DO NOT EDIT THIS BLOCK";
const HOOK_FOOTER: &str = "# end slg hook";

/// Hook definitions: (filename, content).
fn hook_definitions() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "post-checkout",
            "slg reindex --delta-only --background --silent 2>/dev/null &",
        ),
        (
            "post-commit",
            "slg reindex --delta-only --background --silent 2>/dev/null &",
        ),
        (
            "post-merge",
            "slg reindex --delta-only --background --silent 2>/dev/null &",
        ),
        (
            "post-rewrite",
            "slg reindex --delta-only --background --silent 2>/dev/null &",
        ),
    ]
}

/// Build the slg block for a hook.
fn build_hook_block(content: &str) -> String {
    format!("{}\n{}\n{}", HOOK_HEADER, content, HOOK_FOOTER)
}

/// Install git hooks for slg. Returns list of hooks installed/updated.
pub fn install_hooks(repo_path: &Path) -> Result<Vec<String>, SlgError> {
    let hooks_dir = repo_path.join(".git").join("hooks");
    if !hooks_dir.exists() {
        std::fs::create_dir_all(&hooks_dir)
            .map_err(|e| SlgError::Git(format!("Failed to create hooks dir: {}", e)))?;
    }

    let mut installed = Vec::new();

    for (name, content) in hook_definitions() {
        let hook_path = hooks_dir.join(name);
        let block = build_hook_block(content);

        if hook_path.exists() {
            let existing = std::fs::read_to_string(&hook_path).unwrap_or_default();

            if existing.contains(HOOK_HEADER) {
                // Update existing slg block
                let updated = replace_slg_block(&existing, &block);
                std::fs::write(&hook_path, updated)
                    .map_err(|e| SlgError::Git(format!("Failed to update hook {}: {}", name, e)))?;
                debug!("Updated hook: {}", name);
            } else {
                // Append slg block to existing hook
                let appended = format!("{}\n\n{}\n", existing.trim_end(), block);
                std::fs::write(&hook_path, appended).map_err(|e| {
                    SlgError::Git(format!("Failed to append to hook {}: {}", name, e))
                })?;
                debug!("Appended to existing hook: {}", name);
            }
        } else {
            // Create new hook file
            let new_content = format!("#!/bin/sh\n{}\n", block);
            std::fs::write(&hook_path, new_content)
                .map_err(|e| SlgError::Git(format!("Failed to create hook {}: {}", name, e)))?;
            debug!("Created hook: {}", name);
        }

        // chmod 755 on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755));
        }

        installed.push(name.to_string());
    }

    Ok(installed)
}

/// Remove slg blocks from all hook files.
pub fn remove_hooks(repo_path: &Path) -> Result<(), SlgError> {
    let hooks_dir = repo_path.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return Ok(());
    }

    for (name, _) in hook_definitions() {
        let hook_path = hooks_dir.join(name);
        if !hook_path.exists() {
            continue;
        }

        let existing = std::fs::read_to_string(&hook_path).unwrap_or_default();
        if !existing.contains(HOOK_HEADER) {
            continue;
        }

        let cleaned = remove_slg_block(&existing);
        let trimmed = cleaned.trim();

        if trimmed.is_empty() || trimmed == "#!/bin/sh" {
            // Lore was the only content — remove the file
            let _ = std::fs::remove_file(&hook_path);
            debug!("Removed hook file: {}", name);
        } else {
            std::fs::write(&hook_path, cleaned)
                .map_err(|e| SlgError::Git(format!("Failed to clean hook {}: {}", name, e)))?;
            debug!("Removed slg block from hook: {}", name);
        }
    }

    Ok(())
}

/// Check if all 4 hooks have the slg block installed.
pub fn hooks_installed(repo_path: &Path) -> bool {
    let hooks_dir = repo_path.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return false;
    }

    for (name, _) in hook_definitions() {
        let hook_path = hooks_dir.join(name);
        if !hook_path.exists() {
            return false;
        }
        let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
        if !content.contains(HOOK_HEADER) {
            return false;
        }
    }

    true
}

/// Replace the slg block within existing content.
fn replace_slg_block(content: &str, new_block: &str) -> String {
    if let (Some(start), Some(end)) = (content.find(HOOK_HEADER), content.find(HOOK_FOOTER)) {
        let before = &content[..start];
        let after = &content[end + HOOK_FOOTER.len()..];
        format!("{}{}{}", before, new_block, after)
    } else {
        content.to_string()
    }
}

/// Remove the slg block from content.
fn remove_slg_block(content: &str) -> String {
    if let (Some(start), Some(end)) = (content.find(HOOK_HEADER), content.find(HOOK_FOOTER)) {
        let before = &content[..start];
        let after = &content[end + HOOK_FOOTER.len()..];
        format!("{}{}", before.trim_end(), after)
    } else {
        content.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();

        // Create initial commit
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree_oid = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();

        let path = dir.path().to_path_buf();
        (dir, path)
    }

    #[test]
    fn test_install_hooks() {
        let (_dir, path) = create_test_repo();
        let installed = install_hooks(&path).unwrap();
        assert_eq!(installed.len(), 4);
        assert!(hooks_installed(&path));
    }

    #[test]
    fn test_remove_hooks() {
        let (_dir, path) = create_test_repo();
        install_hooks(&path).unwrap();
        assert!(hooks_installed(&path));

        remove_hooks(&path).unwrap();
        assert!(!hooks_installed(&path));
    }

    #[test]
    fn test_install_preserves_existing_hooks() {
        let (_dir, path) = create_test_repo();
        let hooks_dir = path.join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();

        // Create an existing hook with user content
        let hook_path = hooks_dir.join("post-commit");
        std::fs::write(&hook_path, "#!/bin/sh\necho 'my custom hook'\n").unwrap();

        install_hooks(&path).unwrap();

        let content = std::fs::read_to_string(&hook_path).unwrap();
        assert!(
            content.contains("my custom hook"),
            "Existing content preserved"
        );
        assert!(content.contains(HOOK_HEADER), "Lore block added");
    }

    #[test]
    fn test_hooks_not_installed() {
        let (_dir, path) = create_test_repo();
        assert!(!hooks_installed(&path));
    }
}
