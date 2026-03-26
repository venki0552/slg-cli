use crate::types::OutputFormat;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// User configuration stored at ~/.slg/config.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlgConfig {
    /// Delete stale branch indices after N days of inactivity
    #[serde(default = "default_cleanup_days")]
    pub cleanup_after_days: u64,
    /// Max tokens in response output
    #[serde(default = "default_max_response_tokens")]
    pub max_response_tokens: usize,
    /// Default number of search results
    #[serde(default = "default_result_limit")]
    pub default_result_limit: u32,
    /// Embedding model name
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    /// Default output format
    #[serde(default)]
    pub default_output_format: OutputFormat,
    /// Enable cross-encoder reranker (adds ~50ms)
    #[serde(default)]
    pub enable_reranker: bool,
    /// MCP rate limit requests per minute
    #[serde(default = "default_mcp_rate_limit")]
    pub mcp_rate_limit_rpm: u32,
    /// MCP max output bytes per response
    #[serde(default = "default_mcp_output_max")]
    pub mcp_output_max_bytes: usize,
    /// MCP request timeout in seconds
    #[serde(default = "default_mcp_timeout")]
    pub mcp_timeout_secs: u64,
    /// LLM configuration (Phase 2)
    #[serde(default)]
    pub llm: Option<LlmConfig>,
}

/// LLM provider configuration — API keys NEVER stored here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model: String,
    /// Environment variable name to read API key from (e.g. "ANTHROPIC_API_KEY")
    pub api_key_env: String,
    /// Base URL for local providers (Ollama, LM Studio)
    pub base_url: Option<String>,
    #[serde(default = "default_llm_timeout")]
    pub timeout_secs: u64,
}

/// Supported LLM providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LlmProvider {
    Anthropic,
    OpenAI,
    Gemini,
    Ollama,
    LmStudio,
    ClaudeCode,
    None,
}

fn default_cleanup_days() -> u64 {
    7
}
fn default_max_response_tokens() -> usize {
    4096
}
fn default_result_limit() -> u32 {
    3
}
fn default_embedding_model() -> String {
    "all-MiniLM-L6-v2".to_string()
}
fn default_mcp_rate_limit() -> u32 {
    60
}
fn default_mcp_output_max() -> usize {
    50_000
}
fn default_mcp_timeout() -> u64 {
    5
}
fn default_llm_timeout() -> u64 {
    30
}

impl Default for SlgConfig {
    fn default() -> Self {
        Self {
            cleanup_after_days: default_cleanup_days(),
            max_response_tokens: default_max_response_tokens(),
            default_result_limit: default_result_limit(),
            embedding_model: default_embedding_model(),
            default_output_format: OutputFormat::Text,
            enable_reranker: false,
            mcp_rate_limit_rpm: default_mcp_rate_limit(),
            mcp_output_max_bytes: default_mcp_output_max(),
            mcp_timeout_secs: default_mcp_timeout(),
            llm: None,
        }
    }
}

impl SlgConfig {
    /// Load config from ~/.slg/config.toml.
    /// Falls back to defaults if file not found.
    /// Never fails — always returns a valid config.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: SlgConfig = toml::from_str(&content).unwrap_or_default();
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Write config to ~/.slg/config.toml.
    /// Creates parent directories if needed.
    /// Never writes API keys even if somehow present.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;

        // Set file permissions to owner-only on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    /// Returns the path to ~/.slg/config.toml
    pub fn config_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".slg")
            .join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SlgConfig::default();
        assert_eq!(config.cleanup_after_days, 7);
        assert_eq!(config.max_response_tokens, 4096);
        assert_eq!(config.default_result_limit, 3);
        assert_eq!(config.embedding_model, "all-MiniLM-L6-v2");
        assert_eq!(config.default_output_format, OutputFormat::Text);
        assert!(!config.enable_reranker);
        assert_eq!(config.mcp_rate_limit_rpm, 60);
        assert_eq!(config.mcp_output_max_bytes, 50_000);
        assert_eq!(config.mcp_timeout_secs, 5);
        assert!(config.llm.is_none());
    }

    #[test]
    fn test_config_roundtrip() {
        let config = SlgConfig::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: SlgConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.cleanup_after_days, config.cleanup_after_days);
        assert_eq!(deserialized.max_response_tokens, config.max_response_tokens);
    }

    #[test]
    fn test_config_path() {
        let path = SlgConfig::config_path();
        assert!(path.to_string_lossy().contains(".slg"));
        assert!(path.to_string_lossy().contains("config.toml"));
    }
}
