use lore_core::errors::LoreError;
use std::path::PathBuf;
use tracing::debug;

const SHELL_BLOCK_HEADER: &str = "# lore — semantic git intelligence — DO NOT EDIT THIS BLOCK";
const SHELL_BLOCK_FOOTER: &str = "# end lore";

/// Supported shell types.
#[derive(Debug, Clone, PartialEq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
    PowerShell,
    Unknown,
}

/// Detect the current user's shell from environment.
pub fn detect_shell() -> Shell {
    if let Ok(shell) = std::env::var("SHELL") {
        if shell.contains("zsh") {
            return Shell::Zsh;
        }
        if shell.contains("bash") {
            return Shell::Bash;
        }
        if shell.contains("fish") {
            return Shell::Fish;
        }
    }

    // On Windows, check for PowerShell
    if cfg!(windows) {
        if std::env::var("PSModulePath").is_ok() {
            return Shell::PowerShell;
        }
    }

    Shell::Unknown
}

/// Get the RC file path for the given shell.
pub fn shell_rc_path(shell: &Shell) -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    match shell {
        Shell::Zsh => Some(home.join(".zshrc")),
        Shell::Bash => {
            // macOS uses .bash_profile, Linux uses .bashrc
            let profile = home.join(".bash_profile");
            if profile.exists() || cfg!(target_os = "macos") {
                Some(profile)
            } else {
                Some(home.join(".bashrc"))
            }
        }
        Shell::Fish => Some(home.join(".config").join("fish").join("config.fish")),
        Shell::PowerShell => {
            // PowerShell profile
            if let Ok(profile) = std::env::var("PROFILE") {
                Some(PathBuf::from(profile))
            } else {
                let docs = home.join("Documents").join("PowerShell").join("Microsoft.PowerShell_profile.ps1");
                Some(docs)
            }
        }
        Shell::Unknown => None,
    }
}

/// Install shell integration. Returns true if installed, false if already present.
pub fn install_shell_integration(shell: &Shell) -> Result<bool, LoreError> {
    let rc_path = shell_rc_path(shell).ok_or_else(|| {
        LoreError::Config("Could not determine shell RC file path".to_string())
    })?;

    // Check if already installed
    if rc_path.exists() {
        let content = std::fs::read_to_string(&rc_path).unwrap_or_default();
        if content.contains(SHELL_BLOCK_HEADER) {
            debug!("Shell integration already installed in {:?}", rc_path);
            return Ok(false);
        }
    }

    let block = shell_block(shell);

    // Append to rc file
    let existing = if rc_path.exists() {
        std::fs::read_to_string(&rc_path).unwrap_or_default()
    } else {
        // Ensure parent directory exists
        if let Some(parent) = rc_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        String::new()
    };

    let new_content = format!("{}\n\n{}\n", existing.trim_end(), block);
    std::fs::write(&rc_path, new_content).map_err(|e| {
        LoreError::Git(format!("Failed to write shell RC: {}", e))
    })?;

    debug!("Installed shell integration in {:?}", rc_path);
    Ok(true)
}

/// Check if shell integration is already installed.
pub fn shell_integration_installed(shell: &Shell) -> bool {
    let rc_path = match shell_rc_path(shell) {
        Some(p) => p,
        None => return false,
    };

    if !rc_path.exists() {
        return false;
    }

    let content = std::fs::read_to_string(&rc_path).unwrap_or_default();
    content.contains(SHELL_BLOCK_HEADER)
}

/// Build the shell integration block for a given shell.
fn shell_block(shell: &Shell) -> String {
    match shell {
        Shell::Zsh => format!(
            r#"{header}
_lore_chpwd() {{
  if [ -d ".git" ]; then
    local _lore_hash
    _lore_hash=$(lore _repo-hash 2>/dev/null)
    if [ -n "$_lore_hash" ] && [ ! -f "$HOME/.lore/indices/$_lore_hash/main.db" ]; then
      lore index --background --silent 2>/dev/null &
    fi
  fi
}}
autoload -U add-zsh-hook
add-zsh-hook chpwd _lore_chpwd
{footer}"#,
            header = SHELL_BLOCK_HEADER,
            footer = SHELL_BLOCK_FOOTER,
        ),
        Shell::Bash => format!(
            r#"{header}
_lore_chpwd() {{
  if [ -d ".git" ]; then
    local _lore_hash
    _lore_hash=$(lore _repo-hash 2>/dev/null)
    if [ -n "$_lore_hash" ] && [ ! -f "$HOME/.lore/indices/$_lore_hash/main.db" ]; then
      lore index --background --silent 2>/dev/null &
    fi
  fi
}}
PROMPT_COMMAND="_lore_chpwd; $PROMPT_COMMAND"
{footer}"#,
            header = SHELL_BLOCK_HEADER,
            footer = SHELL_BLOCK_FOOTER,
        ),
        Shell::Fish => format!(
            r#"{header}
function _lore_chpwd --on-variable PWD
  if test -d ".git"
    set -l _lore_hash (lore _repo-hash 2>/dev/null)
    if test -n "$_lore_hash"; and not test -f "$HOME/.lore/indices/$_lore_hash/main.db"
      lore index --background --silent 2>/dev/null &
    end
  end
end
{footer}"#,
            header = SHELL_BLOCK_HEADER,
            footer = SHELL_BLOCK_FOOTER,
        ),
        Shell::PowerShell | Shell::Unknown => format!(
            r#"{header}
# lore auto-index on directory change (PowerShell)
# Not yet supported — run lore index manually
{footer}"#,
            header = SHELL_BLOCK_HEADER,
            footer = SHELL_BLOCK_FOOTER,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_shell() {
        // Just verify it doesn't panic
        let _ = detect_shell();
    }

    #[test]
    fn test_shell_rc_path() {
        // Verify known shells return a path
        if let Some(path) = shell_rc_path(&Shell::Zsh) {
            assert!(path.to_string_lossy().contains(".zshrc"));
        }
        if let Some(path) = shell_rc_path(&Shell::Bash) {
            let s = path.to_string_lossy();
            assert!(s.contains(".bashrc") || s.contains(".bash_profile"));
        }
    }

    #[test]
    fn test_shell_block_zsh() {
        let block = shell_block(&Shell::Zsh);
        assert!(block.contains(SHELL_BLOCK_HEADER));
        assert!(block.contains(SHELL_BLOCK_FOOTER));
        assert!(block.contains("add-zsh-hook"));
    }

    #[test]
    fn test_shell_block_bash() {
        let block = shell_block(&Shell::Bash);
        assert!(block.contains("PROMPT_COMMAND"));
    }

    #[test]
    fn test_shell_block_fish() {
        let block = shell_block(&Shell::Fish);
        assert!(block.contains("--on-variable PWD"));
    }

    #[test]
    fn test_unknown_shell_rc_path() {
        assert!(shell_rc_path(&Shell::Unknown).is_none());
    }
}
