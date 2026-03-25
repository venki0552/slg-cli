# lore

**Semantic git intelligence for LLM agents.**

lore transforms your git history into a queryable semantic knowledge base, serving precise, token-efficient context to LLM agents via MCP (Model Context Protocol).

## What it does

Instead of agents running `git log | grep` and hallucinating about your codebase history, lore provides:

- **Semantic search** over commit history (vector + BM25 hybrid search)
- **Token-efficient** XML/JSON output designed for LLM consumption
- **MCP server** that integrates with Claude Code, Cursor, Windsurf, and others
- **Security-first** design: never mutates git, sanitizes all output, redacts secrets

## Quick Start

```bash
# Initialize lore for your repo
lore init

# Ask why something exists
lore why "Why is the retry limit set to 3?"

# Run health checks
lore doctor

# Start MCP server (for AI agents)
lore serve
```

## Commands

| Command            | Description                                |
| ------------------ | ------------------------------------------ |
| `lore init`        | Initialize lore index for the current repo |
| `lore why <query>` | Semantic search over git history           |
| `lore doctor`      | Run health checks and fix issues           |
| `lore serve`       | Start MCP server (stdio JSON-RPC 2.0)      |

## Architecture

```
lore-cli          ← Binary entry point (clap)
├── lore-core     ← Types, config, errors
├── lore-security ← Sanitizer, redactor, scanner, output guard
├── lore-git      ← Git detection, ingestion, delta indexing
├── lore-index    ← SQLite store, embeddings, BM25, hybrid search
├── lore-output   ← XML/JSON/text formatters, token budgeting
└── lore-mcp      ← JSON-RPC 2.0 MCP server, rate limiting
```

## VS Code Extension

The `plugin/` directory contains a VS Code extension that:

- Automatically downloads and manages the lore binary
- Indexes your workspace on activation
- Watches for branch changes and re-indexes
- Registers lore as an MCP server with detected AI agents
- Shows index status in the status bar

## MCP Integration

lore exposes 5 MCP tools:

| Tool          | Description                                  |
| ------------- | -------------------------------------------- |
| `lore_why`    | Semantic search: "Why does this code exist?" |
| `lore_blame`  | Enhanced blame with semantic context         |
| `lore_log`    | Filtered, token-efficient commit log         |
| `lore_bisect` | Find when a behavior was introduced          |
| `lore_status` | Index health and statistics                  |

### Agent Configuration

Copy the appropriate config to register lore with your AI agent:

**Claude Code** (`~/.claude/claude_desktop_config.json`):

```json
{
	"mcpServers": {
		"lore": { "command": "lore", "args": ["mcp", "start"] }
	}
}
```

**Cursor** (`~/.cursor/mcp.json`):

```json
{
	"mcpServers": {
		"lore": { "command": "lore", "args": ["mcp", "start"] }
	}
}
```

## Output Formats

- **`--format text`** — Human-readable terminal output (default)
- **`--format xml`** — CDATA-isolated XML for LLM agents
- **`--format json`** — Structured JSON

## Security

lore is designed with security as a first-class concern:

- **Never mutates git** — read-only access to `.git/`
- **Prompt injection defense** — all commit messages scanned and sanitized
- **Secret redaction** — API keys, tokens, passwords redacted before output
- **Path traversal protection** — all index paths sanitized
- **CDATA isolation** — XML output prevents injection into LLM context
- **Output size limits** — configurable token budgets prevent context overflow

## Building from Source

```bash
# Build
cargo build --release

# Run tests (137 tests)
cargo test --workspace

# Build VS Code plugin
cd plugin && npm install && npm run compile
```

## Requirements

- Rust 1.75+ (for building)
- Git repository (for indexing)
- Node.js 20+ (for VS Code plugin development)

## License

MIT
