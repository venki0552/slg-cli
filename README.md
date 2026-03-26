# lore

**Semantic git intelligence for LLM agents.**

lore is a single Rust binary that transforms your git history into a queryable semantic knowledge base. It serves precise, token-efficient context to LLM agents via MCP (Model Context Protocol) — with zero cloud dependency, zero data egress, and zero git mutation.

[![CI](https://github.com/venki0552/lore-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/venki0552/lore-cli/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE-MIT)

---

## The Problem

When an LLM agent needs to understand _why_ a decision was made in your codebase, it typically reads 5–20 source files and still guesses:

```
Agent today — "why was the retry limit set to 3?":
  reads config files           →  2,400 tokens
  reads middleware              →  4,100 tokens
  reads several services        →  8,200 tokens
  runs grep, finds nothing      →  600 tokens
  hallucinates an answer        →  WRONG
  Total: ~25,000 tokens, 45 seconds, bad answer

Agent with lore — same question:
  lore why "retry limit 3"     →  200 tokens, <200ms, ground truth from git history
  Total: 200 tokens, correct answer
```

lore reduces agent token usage by ~95% for history-related questions by indexing your git commits with hybrid vector + BM25 search and serving results through a local MCP server.

---

## What lore Does

- **Indexes your git history** into a local SQLite database using the `all-MiniLM-L6-v2` embedding model (runs fully offline after first download)
- **Hybrid search** combines vector similarity (cosine) and BM25 lexical ranking, fused with Reciprocal Rank Fusion (RRF)
- **Serves results via MCP** — a JSON-RPC 2.0 server over stdio that plugs into Claude Code, Cursor, Windsurf, and any MCP-compatible agent
- **Security-first by design** — all commit messages are sanitized, secrets are redacted before storage, output is CDATA-isolated, and lore never mutates your git repository

## What lore Does NOT Do

- It does not make LLM API calls — retrieval commands work offline with zero LLM dependency
- It does not modify git history, commit files, or change branches
- It does not send any data to a server or cloud service — all data stays in `~/.lore/` on your machine
- It is not a coding assistant — it feeds context _to_ assistants

---

## Installation

### Pre-built Binaries (recommended)

Download the latest release from [GitHub Releases](https://github.com/venki0552/lore-cli/releases):

| Platform        | Binary                    |
| --------------- | ------------------------- |
| Linux x86_64    | `lore-linux-x86_64`       |
| Linux aarch64   | `lore-linux-aarch64`      |
| macOS ARM (M1+) | `lore-darwin-arm64`       |
| macOS Intel     | `lore-darwin-x86_64`      |
| Windows x86_64  | `lore-windows-x86_64.exe` |

Each binary ships with a `.sha256` checksum file. After downloading, verify and install:

```bash
# Linux / macOS
sha256sum -c lore-linux-x86_64.sha256
chmod +x lore-linux-x86_64
sudo mv lore-linux-x86_64 /usr/local/bin/lore

# Windows (PowerShell)
# Move lore-windows-x86_64.exe to a folder on your PATH and rename to lore.exe
```

### Building from Source

Requirements: **Rust 1.75+**, **Git**

```bash
git clone https://github.com/venki0552/lore-cli
cd lore-cli
cargo build --release
# Binary is at: target/release/lore  (or lore.exe on Windows)
```

---

## Quick Start

```bash
# 1. Go to any git repository
cd /path/to/your/repo

# 2. Initialize lore (indexes your history, installs git hooks, registers MCP)
lore init

# 3. Search your git history semantically
lore why "Why is the retry limit set to 3?"

# 4. Check index health
lore doctor

# 5. Start the MCP server (for AI agent integration)
lore serve
```

On first `lore init`, the `all-MiniLM-L6-v2` embedding model (~90 MB) is downloaded to `~/.lore/models/` and cached for all future uses.

---

## Commands

### Core Commands

| Command                         | Description                                                |
| ------------------------------- | ---------------------------------------------------------- |
| `lore init`                     | Index repo, install git hooks, register MCP                |
| `lore index`                    | Explicitly run a full index of the current branch          |
| `lore reindex`                  | Delta-only reindex (fast, used by git hooks)               |
| `lore why <query>`              | Semantic search over git history                           |
| `lore blame <file>`             | Semantic ownership — who changed this file and why         |
| `lore bisect <bug_description>` | Find which commit likely introduced a bug                  |
| `lore log <query>`              | Intent-grouped semantic git log                            |
| `lore diff [base] [head]`       | Intent-level diff between two refs (default: HEAD~1..HEAD) |
| `lore revert-risk <commit>`     | Blast radius analysis before reverting a commit            |
| `lore status`                   | Show indexed commits, storage, and MCP state               |
| `lore cleanup`                  | Remove stale branch indices (default: older than 7 days)   |
| `lore doctor`                   | Diagnose and optionally fix lore setup issues              |
| `lore serve`                    | Start the MCP server (stdio JSON-RPC 2.0)                  |
| `lore mcp`                      | Alias for `lore serve`                                     |
| `lore sync`                     | Manually trigger reindex (for CI use)                      |

### Command Flags

All commands support:

| Flag             | Description                                    |
| ---------------- | ---------------------------------------------- |
| `--format <fmt>` | Output format: `text` (default), `xml`, `json` |
| `--max-tokens N` | Limit output to N tokens (default: 4096)       |
| `--silent`       | Suppress non-result output (used by git hooks) |

**`lore init` specific flags:**

| Flag           | Description                                    |
| -------------- | ---------------------------------------------- |
| `--global`     | Install globally with shell integration        |
| `--background` | Run the initial index in the background        |
| `--mcp-only`   | Only register MCP, skip hooks                  |
| `--hooks-only` | Only install git hooks                         |
| `--shell-only` | Only install shell integration (zsh/bash/fish) |

**`lore why` specific flags:**

| Flag              | Description                            |
| ----------------- | -------------------------------------- |
| `--limit N`       | Number of results (default: 3)         |
| `--since <date>`  | Filter commits after ISO date          |
| `--author <name>` | Filter by author name                  |
| `--module <path>` | Filter to commits touching a path      |
| `--rerank`        | Enable cross-encoder reranking (~50ms) |

**`lore cleanup` specific flags:**

| Flag             | Description                                   |
| ---------------- | --------------------------------------------- |
| `--older-than N` | Remove indices older than N days (default: 7) |
| `--dry-run`      | Show what would be deleted without deleting   |

**`lore diff` specific flags:**

| Flag              | Description                       |
| ----------------- | --------------------------------- |
| `--breaking-only` | Show only breaking-change commits |

---

## Output Formats

lore supports three output formats controlled by `--format`:

- **`text`** (default) — Human-readable terminal output with color
- **`xml`** — CDATA-isolated XML designed for safe LLM consumption, prevents prompt injection
- **`json`** — Structured JSON for programmatic use

Example:

```bash
lore why "authentication changes" --format json --max-tokens 2000
```

---

## MCP Integration

lore runs as a local MCP (Model Context Protocol) server, exposing 5 read-only tools to AI agents:

| MCP Tool      | Description                                                   |
| ------------- | ------------------------------------------------------------- |
| `lore_why`    | Semantic search over git history: "Why does this code exist?" |
| `lore_blame`  | Find semantic ownership of a file or function                 |
| `lore_log`    | Search git history grouped by intent                          |
| `lore_bisect` | Find which commit likely introduced a bug                     |
| `lore_status` | Get current index status and statistics                       |

The MCP server enforces:

- **Rate limiting**: 60 requests per minute
- **Output cap**: 50,000 bytes per response
- **Request timeout**: 5 seconds per call
- **Read-only**: no tool can modify git or the filesystem

### Connecting Your AI Agent

Config files for each supported agent are in the `configs/` directory of this repository. Copy the relevant one:

**Claude Code** — add to `~/.claude/claude_desktop_config.json`:

```json
{
	"mcpServers": {
		"lore": { "command": "lore", "args": ["serve"] }
	}
}
```

**Cursor** — add to `~/.cursor/mcp.json`:

```json
{
	"mcpServers": {
		"lore": { "command": "lore", "args": ["serve"] }
	}
}
```

**Windsurf** — add to `~/.codeium/windsurf/mcp_config.json`:

```json
{
	"mcpServers": {
		"lore": { "command": "lore", "args": ["serve"] }
	}
}
```

After adding the config, restart your agent. It will auto-discover the MCP tools.

---

## VS Code Extension

The `plugin/` directory contains a VS Code extension (TypeScript) that:

- Automatically downloads and manages the correct platform binary for your OS
- Indexes your workspace on activation
- Watches for branch changes and triggers a delta reindex automatically
- Registers lore as an MCP server with detected AI agents
- Shows live index status in the VS Code status bar
- Surfaces `lore doctor` output in the editor UI

To build the extension locally:

```bash
cd plugin
npm install
npm run compile
```

---

## Configuration

lore is configurable via `~/.lore/config.toml`. All fields are optional — defaults are shown below:

```toml
# Delete stale branch indices after N days of inactivity
cleanup_after_days = 7

# Default max tokens in response output
max_response_tokens = 4096

# Default number of search results
default_result_limit = 3

# Embedding model (currently only all-MiniLM-L6-v2 is supported)
embedding_model = "all-MiniLM-L6-v2"

# Default output format: "text", "xml", or "json"
default_output_format = "text"

# Enable cross-encoder reranker (adds ~50ms per query)
enable_reranker = false

# MCP server rate limit (requests per minute)
mcp_rate_limit_rpm = 60

# MCP server max output bytes per response
mcp_output_max_bytes = 50000

# MCP request timeout in seconds
mcp_timeout_secs = 5
```

---

## Data Storage

lore stores all data locally under `~/.lore/`:

```
~/.lore/
├── models/                    ← Embedding model cache (~90 MB, downloaded once)
├── indices/
│   └── <repo-hash>/
│       ├── main.db            ← Index for the main branch
│       ├── feature-xyz.db     ← Index per branch (created on switch)
│       └── ...
├── config.toml                ← Optional user configuration
└── security.log               ← Security events (injections blocked, secrets redacted)
```

Index directories are created with `0700` permissions on Unix (owner-only). No data ever leaves your machine.

---

## How Indexing Works

1. **Ingestion** — lore walks your git history using `libgit2`, building a `CommitDoc` for each commit
2. **Security pass** — each commit message and diff summary is sanitized (injection patterns removed) and redacted (secrets replaced with `[REDACTED-...]` labels)
3. **Embedding** — each `CommitDoc` is embedded using the `all-MiniLM-L6-v2` model (384-dimensional vectors)
4. **BM25** — tokens are indexed into an inverted BM25 index stored in SQLite
5. **Storage** — vectors and metadata are stored in a per-branch SQLite database under `~/.lore/indices/`

### How Search Works

Each query runs through a hybrid pipeline:

1. **Vector search** — cosine similarity against all stored embeddings
2. **BM25 search** — lexical keyword match with TF-IDF weighting (k1=1.5, b=0.75)
3. **RRF fusion** — Reciprocal Rank Fusion (k=60) merges both ranked lists
4. **Boosts** — recent commits (+20%), exact token match (+50%), security-tagged commits (+30%)
5. **Filters** — `--since`, `--author`, `--module` applied post-fusion
6. **Token budget** — results are truncated to fit `--max-tokens` (always returns at least 1 result)
7. **Output guard** — final output is scanned for injection patterns before being returned

---

## Security Design

lore treats security as a first-class requirement, not an afterthought:

| Threat               | Mitigation                                                                  |
| -------------------- | --------------------------------------------------------------------------- |
| Git mutation         | lore is 100% read-only — no git commands that write are ever called         |
| Prompt injection     | All commit messages sanitized; `<`, `>`, `]]>` stripped from all output     |
| Secret leakage       | 15+ secret patterns detected and replaced before any data reaches the index |
| Path traversal       | All index paths constructed through `safe_index_path()` with prefix checks  |
| CDATA injection      | XML output wraps all commit data in CDATA, `]]>` sequences are escaped      |
| Context overflow     | Configurable token budget enforced on all responses                         |
| MCP abuse            | 60 req/min rate limit, 50KB output cap, 5s request timeout                  |
| Directory permission | `~/.lore/indices/` created with `0700` (owner-only) on Unix                 |

Secret patterns detected and redacted before storage include: AWS access keys, GitHub PATs, Anthropic/OpenAI API keys, Google API keys, Stripe keys, Twilio SIDs, private key blocks, JWT tokens, database connection strings with credentials, and generic `key=value` credential patterns.

---

## Architecture

lore is a Rust workspace with 7 crates:

```
lore-cli          ← Binary entry point (clap argument parsing, command dispatch)
├── lore-core     ← Shared types (CommitDoc, SearchResult), config, errors
├── lore-security ← Sanitizer, secret redactor, injection scanner, path guard, output guard
├── lore-git      ← Git ingestion (libgit2), delta indexing, hook installer, shell integration
├── lore-index    ← SQLite store, all-MiniLM-L6-v2 embedder, BM25 index, RRF hybrid search
├── lore-output   ← XML/JSON/text formatters, token budget enforcement
└── lore-mcp      ← JSON-RPC 2.0 MCP server, rate limiter, auto-init
```

### Key Data Type

Every indexed commit becomes a `CommitDoc`:

```
hash, short_hash     — full and 7-char SHA
message, body        — sanitized commit subject and body
diff_summary         — per-file intent summaries (NOT raw diff lines)
author               — display name only (email never stored)
timestamp            — Unix epoch seconds
files_changed        — list of file paths touched
insertions, deletions— line counts
linked_issues, prs   — parsed from "fixes #234", "PR #123"
intent               — Fix | Feature | Refactor | Perf | Security | Docs | Test | Chore | Revert | Unknown
risk_score           — 0.0–1.0 (from file sensitivity + churn + deletion ratio)
branch               — which branch this was indexed from
injection_flagged    — whether injection patterns were detected
secrets_redacted     — count of secrets redacted (what they were is never stored)
```

---

## CI / CD

Three GitHub Actions workflows run automatically:

| Workflow        | Trigger                 | What it does                                                                                                                   |
| --------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `ci.yml`        | Every PR + push to main | `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`, `cargo audit`, security invariant tests, VS Code plugin compile |
| `release.yml`   | Push a `v*` tag         | Builds for 5 platforms, creates GitHub Release with checksums                                                                  |
| `benchmark.yml` | PR to main              | Regression guard — fails if search quality drops >5%                                                                           |

---

## What's Implemented (v0.1.0)

All retrieval commands are implemented and tested:

- `lore init` — full setup: index, git hooks (post-commit, post-checkout, post-merge, post-rewrite), shell integration (zsh, bash, fish, PowerShell)
- `lore index` / `lore reindex` — full and delta indexing
- `lore why` — hybrid vector+BM25 semantic search with all filters
- `lore blame` — semantic file/function ownership
- `lore bisect` — semantic bug-introduction search
- `lore log` — intent-grouped commit history search
- `lore diff` — intent-level diff between two git refs
- `lore revert-risk` — blast radius analysis
- `lore status` — index stats and storage breakdown
- `lore cleanup` — stale index pruning
- `lore doctor` — health checks (binary version, lore home, models dir, git repo, index, hooks)
- `lore serve` / `lore mcp` — MCP server with 5 tools, rate limiting, timeout, output cap
- `lore sync` — CI-friendly reindex trigger
- VS Code extension — binary management, workspace indexing, branch watching, MCP registration, status bar

## What's Not Yet Implemented

The following are planned for future phases and scaffolded but not functional:

- `lore commit` — history-aware commit message generation (requires LLM)
- `lore pr` — PR description generation (requires LLM)
- `lore review` — pre-push review (requires LLM)
- LLM provider auto-detection and configuration (Anthropic, OpenAI, Gemini, Ollama, LM Studio, Claude Code CLI)
- Cross-encoder reranker (`--rerank` flag parses but reranker is a stub)
- Benchmark suite results (framework exists, real-world runs pending)
- Homebrew formula and crates.io publish
- VS Code Marketplace publish

---

## Running Locally

```bash
# Clone
git clone https://github.com/venki0552/lore-cli
cd lore-cli

# Run all tests
cargo test --workspace

# Check for lint errors (same as CI)
cargo clippy --all -- -D warnings

# Check formatting (same as CI)
cargo fmt --all --check

# Fix formatting
cargo fmt --all

# Build release binary
cargo build --release
# Binary: target/release/lore

# Run a command against this repo itself
./target/release/lore init
./target/release/lore why "why was BM25 added"

# Enable debug logging
LORE_LOG=debug ./target/release/lore why "test query"

# Run security invariant tests specifically
cargo test -p lore-security

# Build VS Code extension
cd plugin && npm install && npm run compile
```

---

## Environment Variables

| Variable   | Description                                                            |
| ---------- | ---------------------------------------------------------------------- |
| `LORE_LOG` | Tracing log level: `error`, `warn` (default), `info`, `debug`, `trace` |

Logs are written to **stderr** only, never to stdout (which is reserved for MCP JSON-RPC output).

---

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
