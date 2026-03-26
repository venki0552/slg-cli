# Contributing to lore

Thank you for your interest in contributing to lore. This document explains how the project is structured, how to run it locally, how to run the full test suite, and what the conventions are for pull requests.

---

## Table of Contents

- [Project Structure](#project-structure)
- [Prerequisites](#prerequisites)
- [Building Locally](#building-locally)
- [Running Tests](#running-tests)
- [Code Quality Checks](#code-quality-checks)
- [Running the Binary Locally](#running-the-binary-locally)
- [Understanding the Codebase](#understanding-the-codebase)
- [Making Changes](#making-changes)
- [Pull Request Process](#pull-request-process)
- [Security](#security)
- [What's Left To Build](#whats-left-to-build)

---

## Project Structure

```
lore-cli/
├── Cargo.toml                  ← Rust workspace root (7 member crates)
├── Cargo.lock
├── crates/
│   ├── lore-core/              ← Shared types (CommitDoc, SearchResult), config, errors
│   ├── lore-security/          ← Sanitizer, secret redactor, path guard, output guard
│   ├── lore-git/               ← git ingestion (libgit2), hooks, shell integration
│   ├── lore-index/             ← SQLite store, embedder, BM25, RRF hybrid search
│   ├── lore-output/            ← XML / JSON / text formatters, token budget
│   ├── lore-mcp/               ← JSON-RPC 2.0 MCP server, rate limiter
│   └── lore-cli/               ← Binary entry point (clap), all command handlers
├── plugin/                     ← VS Code extension (TypeScript)
├── tests/
│   ├── security/               ← Security invariant tests (NEVER skip these)
│   └── benchmarks/             ← Benchmark framework and tasks
├── configs/                    ← Sample MCP config files for Claude, Cursor, Windsurf
├── scripts/                    ← Utility scripts
└── .github/workflows/          ← CI (ci.yml), release (release.yml), benchmarks (benchmark.yml)
```

The crate dependency order is:

```
lore-core  ←  lore-security  ←  lore-git  ←  lore-index  ←  lore-output  ←  lore-mcp  ←  lore-cli
```

`lore-core` has no internal dependencies. Every other crate can depend on crates to its left but not to its right.

---

## Prerequisites

- **Rust** 1.75 or later — install via [rustup](https://rustup.rs)
- **Git** — required at runtime for lore to work
- **Node.js** 20+ — only needed if working on the VS Code extension in `plugin/`

Check your versions:

```bash
rustc --version
cargo --version
git --version
node --version   # only for plugin work
```

---

## Building Locally

```bash
# Clone the repository
git clone https://github.com/venki0552/lore-cli
cd lore-cli

# Build in debug mode (faster compile, slower runtime)
cargo build

# Build in release mode (slower compile, faster runtime — matches production)
cargo build --release
```

The binary ends up at:

- `target/debug/lore` (debug)
- `target/release/lore` (release)
- Windows adds `.exe`

### Building the VS Code Extension

```bash
cd plugin
npm install
npm run compile
```

---

## Running Tests

```bash
# Run all tests across all crates
cargo test --workspace

# Run tests for a single crate
cargo test -p lore-security
cargo test -p lore-index
cargo test -p lore-core

# Run a specific test by name
cargo test -p lore-security test_injection

# Run with output visible (useful for debugging)
cargo test --workspace -- --nocapture

# Run security invariant tests (same as CI)
cargo test -p lore-security --test test_injection
cargo test -p lore-security --test test_secrets
cargo test -p lore-security --test test_paths
cargo test -p lore-security --test test_output
```

The security invariant tests live in `tests/security/` and test properties that must always hold — they are run separately in CI and must never be disabled.

---

## Code Quality Checks

CI runs all of these and will fail on any error or warning. Run them locally before pushing to avoid failed CI runs.

```bash
# Lint — zero warnings allowed (same flag as CI uses)
cargo clippy --all -- -D warnings

# Check formatting (does not modify files)
cargo fmt --all --check

# Fix formatting in-place
cargo fmt --all

# Security audit — checks dependencies for known CVEs
cargo install cargo-audit --locked   # first time only
cargo audit
```

**Important:** The CI runs on Linux. Clippy may report lints on Linux that it does not report on macOS or Windows. If you are on a non-Linux machine, running clippy in WSL (Windows Subsystem for Linux) before pushing will catch all CI lint errors.

---

## Running the Binary Locally

After building, you can run lore against any git repository — including this repository itself:

```bash
# Run against this repo
./target/release/lore init
./target/release/lore why "why does the BM25 index exist"
./target/release/lore status
./target/release/lore doctor

# Enable debug logging (logs go to stderr, not stdout)
LORE_LOG=debug ./target/release/lore why "test query"

# Use different output formats
./target/release/lore why "authentication" --format json
./target/release/lore why "authentication" --format xml

# Test the MCP server manually (send JSON-RPC over stdin)
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | ./target/release/lore serve
```

---

## Understanding the Codebase

### Key Files to Read First

If you are new to the codebase, start with these files in order:

1. `crates/lore-core/src/types.rs` — `CommitDoc`, `SearchResult`, `CommitIntent`. These are the core data types everything else operates on.
2. `crates/lore-core/src/errors.rs` — `LoreError` enum. All errors in the project use this type.
3. `crates/lore-security/src/redactor.rs` — how secrets are detected and redacted before storage.
4. `crates/lore-security/src/paths.rs` — how all index paths are constructed safely.
5. `crates/lore-git/src/ingestion.rs` — how commits are walked and converted to `CommitDoc`.
6. `crates/lore-index/src/search.rs` — the full hybrid search pipeline.
7. `crates/lore-mcp/src/server.rs` — the MCP JSON-RPC 2.0 server loop.
8. `crates/lore-cli/src/main.rs` — clap setup and command dispatch.

### How a `lore why` Query Flows

```
User types: lore why "why is auth rate limited?"

1. lore-cli/commands/why.rs
   a. Validates query length (<= 500 chars)
   b. Finds git root via lore-git::detector::find_git_root()
   c. Computes repo hash → locates index path via lore-security::paths::safe_index_path()
   d. Opens IndexStore and Embedder

2. lore-index::search::search()
   a. Embeds the query with all-MiniLM-L6-v2 (384-dim vector)
   b. Vector search: cosine similarity against all stored vectors
   c. BM25 search: tokenize query, TF-IDF score against inverted index
   d. RRF fusion: merge both ranked lists using Reciprocal Rank Fusion (k=60)
   e. Apply boosts: recency (+20%), exact match (+50%), security intent (+30%)
   f. Apply filters: --since, --author, --module
   g. Apply token budget: truncate to max_tokens (always keeps >= 1 result)

3. lore-security::output_guard::OutputGuard
   a. Final scan of assembled output for injection patterns
   b. If flagged, log to security.log and sanitize

4. lore-output — format and print
   a. text: colored terminal output
   b. xml: CDATA-wrapped XML
   c. json: serde_json structured output
```

### How `lore init` Works

1. Finds the git root
2. Runs `lore index` (full branch walk via libgit2) — streams `CommitDoc` through a channel, each sanitized and redacted before storage
3. Installs git hooks: `post-commit`, `post-checkout`, `post-merge`, `post-rewrite` — each calls `lore reindex --delta-only --background --silent &`
4. Installs shell integration (detects zsh/bash/fish/PowerShell, appends a guarded block to the RC file)
5. Creates `~/.lore/` directory structure with restricted permissions

---

## Making Changes

### Adding a New Command

1. Create `crates/lore-cli/src/commands/<your_command>.rs` following the pattern of existing commands (e.g., `why.rs`)
2. Add `pub mod <your_command>;` to `crates/lore-cli/src/commands/mod.rs`
3. Add a variant to the `Commands` enum in `crates/lore-cli/src/main.rs`
4. Add a match arm in `main()` that dispatches to your new command
5. Write tests in the same file under `#[cfg(test)]`

### Adding a New MCP Tool

1. Add a new `ToolDefinition` in `crates/lore-mcp/src/tools.rs` `get_tool_definitions()`
2. Add a handler arm in `crates/lore-mcp/src/server.rs` in the `tools/call` dispatch
3. Update the tool count assertion in `tools.rs` tests

### Modifying the Index Schema

If you change the SQLite schema in `crates/lore-index/src/store.rs`:

1. Bump the schema version constant
2. Add a migration path in `create_schema()` or handle `SchemaMismatch` errors
3. Update any queries that reference changed columns
4. Add a test covering old → new migration

### Modifying Security Logic

The `lore-security` crate has invariant tests that exercise boundaries directly. Any change to:

- `sanitizer.rs` — update `tests/security/test_injection.rs`
- `redactor.rs` — update `tests/security/test_secrets.rs`
- `paths.rs` — update `tests/security/test_paths.rs`
- `output_guard.rs` — update `tests/security/test_output.rs`

These tests must continue to pass. They are never opt-in — CI runs them as a separate required job.

---

## Pull Request Process

1. **Fork** the repository and create your branch from `main`
2. **Branch naming**: use `fix/<description>`, `feat/<description>`, or `chore/<description>`
3. **Before pushing**, run locally:
   ```bash
   cargo test --workspace
   cargo clippy --all -- -D warnings
   cargo fmt --all --check
   ```
4. **Write tests** — new commands need at least unit tests. Security-related changes need invariant tests.
5. **Keep commits focused** — one logical change per commit with a conventional commit message prefix (`fix:`, `feat:`, `chore:`, `docs:`, `test:`, `refactor:`, `perf:`)
6. **Open a PR** against `main` with a clear description of what changed and why
7. CI must pass (test, clippy, fmt, audit, security tests, plugin compile) before merge

### Commit Message Format

```
<type>: <short description>

<optional body explaining why, not what>
```

Types: `fix`, `feat`, `chore`, `docs`, `test`, `refactor`, `perf`, `security`, `ci`, `style`

Examples:

```
feat: add --until flag to lore why for date range filtering
fix: handle empty commit messages without panicking
security: add Stripe test key pattern to redactor
test: add invariant test for path traversal with unicode
```

---

## Security

If you find a security vulnerability, **do not open a public issue**. Instead, open a [GitHub Security Advisory](https://github.com/venki0552/lore-cli/security/advisories/new) or email the maintainers directly.

Security issues include:

- Any way to make lore write to the git repository
- Any way to bypass secret redaction before storage
- Any injection pattern that survives the sanitizer and reaches output
- Any path traversal that escapes `~/.lore/indices/`
- Any way to make the MCP server perform destructive operations

---

## What's Left To Build

These areas are either stubbed, incomplete, or planned for future phases:

### LLM Integration (Phase 2)

- `lore commit` — history-aware commit message generation
- `lore pr` — PR description generation
- `lore review` — pre-push review
- LLM provider auto-detection (Ollama, Claude Code CLI, Anthropic API, OpenAI API, Gemini API, LM Studio)
- The `lore-llm` crate is scaffolded in the architecture but not yet created

### Cross-Encoder Reranker

- `--rerank` flag is wired through the CLI but `lore-index/src/reranker.rs` is a stub
- Needs a DeBERTa or similar cross-encoder model for re-ranking top-k vector results

### Hardening

- Full benchmark suite with real OSS repos and published recall@k numbers
- Adversarial test suite with injected commit messages
- Homebrew formula
- crates.io publish
- VS Code Marketplace publish

### Known Rough Edges

- `lore diff` uses store lookups for semantic context but the diff-between-refs resolution is basic
- The `--rerank` path is wired but the reranker itself returns results unchanged
- Shell integration installs an RC file block but does not yet provide shell completions
- `lore doctor --fix-all` detects issues but does not yet auto-fix all of them (hooks and index are fixed; shell integration fix is manual)

If you want to pick up any of these, open an issue first to discuss the approach before writing code.

---

## License

By contributing to lore, you agree that your contributions will be licensed under the same terms as the project: MIT OR Apache-2.0.
