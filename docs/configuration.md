# Configuration Reference

lore has two levels of configuration: the `~/.lore/config.toml` file (applies to all repos) and VS Code extension settings (applies per workspace/user).

---

## Contents

- [~/.lore/config.toml](#loreconfigntoml)
  - [Core settings](#core-settings)
  - [MCP server settings](#mcp-server-settings)
  - [LLM settings (Phase 2)](#llm-settings-phase-2)
  - [Full example config](#full-example-config)
- [VS Code Extension Settings](#vs-code-extension-settings)
- [Environment Variables](#environment-variables)
- [Precedence](#precedence)

---

## ~/.lore/config.toml

`lore init` creates this file with defaults. Edit it with any text editor. Missing keys always fall back to the documented defaults — the file is never required.

### Core settings

| Key                     | Type                            | Default              | Description                                                                            |
| ----------------------- | ------------------------------- | -------------------- | -------------------------------------------------------------------------------------- |
| `cleanup_after_days`    | integer                         | `7`                  | Delete stale branch indices after this many days of inactivity (set to `0` to disable) |
| `max_response_tokens`   | integer                         | `4096`               | Maximum tokens in any single response from a retrieval command                         |
| `default_result_limit`  | integer                         | `3`                  | Default number of search results returned by `lore why`, `lore log`, etc.              |
| `embedding_model`       | string                          | `"all-MiniLM-L6-v2"` | Embedding model name. Only `all-MiniLM-L6-v2` is currently supported                   |
| `default_output_format` | `"text"` \| `"xml"` \| `"json"` | `"text"`             | Default output format for retrieval commands                                           |
| `enable_reranker`       | boolean                         | `false`              | Enable cross-encoder re-ranking stage (adds ~50 ms latency, improves accuracy)         |

### MCP server settings

| Key                    | Type    | Default | Description                                           |
| ---------------------- | ------- | ------- | ----------------------------------------------------- |
| `mcp_rate_limit_rpm`   | integer | `60`    | Maximum MCP tool calls per minute (token bucket)      |
| `mcp_output_max_bytes` | integer | `50000` | Maximum response size per tool call in bytes (~50 KB) |
| `mcp_timeout_secs`     | integer | `5`     | Per-tool-call timeout in seconds                      |

### LLM settings (Phase 2)

The `[llm]` section configures lore for generation commands (`lore commit`, `lore pr`, `lore review`). This is not required for retrieval commands.

> **API keys are never stored in `config.toml`.** Only the environment variable name that holds the key is stored.

| Key                | Type    | Default | Description                                                                         |
| ------------------ | ------- | ------- | ----------------------------------------------------------------------------------- |
| `llm.provider`     | string  | —       | One of: `Anthropic`, `OpenAI`, `Gemini`, `Ollama`, `LmStudio`, `ClaudeCode`, `None` |
| `llm.model`        | string  | —       | Model identifier (e.g. `claude-sonnet-4-5`, `gpt-4o`)                               |
| `llm.api_key_env`  | string  | —       | Name of the environment variable holding the API key (e.g. `ANTHROPIC_API_KEY`)     |
| `llm.base_url`     | string  | —       | Base URL for local providers (Ollama: `http://localhost:11434`)                     |
| `llm.timeout_secs` | integer | `30`    | LLM request timeout in seconds                                                      |

### Full example config

```toml
# ~/.lore/config.toml

# Core
cleanup_after_days   = 14
max_response_tokens  = 8192
default_result_limit = 5
default_output_format = "xml"
enable_reranker      = false

# MCP server
mcp_rate_limit_rpm   = 60
mcp_output_max_bytes = 50000
mcp_timeout_secs     = 5

# LLM (Phase 2 — optional)
[llm]
provider    = "Anthropic"
model       = "claude-sonnet-4-5"
api_key_env = "ANTHROPIC_API_KEY"
timeout_secs = 30
```

---

## VS Code Extension Settings

Configure these in VS Code via **File → Preferences → Settings** and search for "lore", or edit `settings.json` directly.

| Setting                  | Type                            | Default | Description                                                                                 |
| ------------------------ | ------------------------------- | ------- | ------------------------------------------------------------------------------------------- |
| `lore.autoRegisterMCP`   | boolean                         | `true`  | Auto-register the lore MCP server with Claude Code, Cursor, and Windsurf agent config files |
| `lore.cleanupAfterDays`  | number                          | `7`     | Delete stale branch indices after N days of inactivity                                      |
| `lore.outputFormat`      | `"text"` \| `"xml"` \| `"json"` | `"xml"` | Default output format for MCP responses                                                     |
| `lore.enableReranker`    | boolean                         | `false` | Enable cross-encoder reranking in MCP responses (~50 ms added latency)                      |
| `lore.indexOnActivation` | boolean                         | `true`  | Automatically index the workspace when VS Code opens                                        |
| `lore.showStatusBar`     | boolean                         | `true`  | Show lore index status in the VS Code status bar                                            |

Example `settings.json` snippet:

```json
{
	"lore.autoRegisterMCP": true,
	"lore.outputFormat": "xml",
	"lore.enableReranker": false,
	"lore.indexOnActivation": true,
	"lore.showStatusBar": true,
	"lore.cleanupAfterDays": 14
}
```

---

## Environment Variables

| Variable            | Description                                                                                                          |
| ------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `LORE_LOG`          | Log level filter passed to `tracing-subscriber`. Values: `error`, `warn`, `info`, `debug`, `trace`. Default: `warn`. |
| `LORE_CONFIG`       | Override path to `config.toml` (default: `~/.lore/config.toml`).                                                     |
| `ANTHROPIC_API_KEY` | (Phase 2) Anthropic API key — referenced by `llm.api_key_env`, never stored in config.                               |
| `OPENAI_API_KEY`    | (Phase 2) OpenAI API key — same pattern.                                                                             |

---

## Precedence

When the same setting can be controlled at multiple levels, the order of precedence (highest wins) is:

1. **CLI flag** — e.g. `--limit`, `--format`, `--max-tokens` passed directly to a command
2. **Environment variable** — e.g. `LORE_LOG`
3. **VS Code extension setting** — only applies when commands are invoked from the extension
4. **`~/.lore/config.toml`** — user-wide default
5. **Built-in default** — always available as fallback
