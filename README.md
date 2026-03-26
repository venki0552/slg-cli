# lore

**Semantic git intelligence for LLM agents.**

lore is a single Rust binary that transforms your git history into a queryable semantic knowledge base. It serves precise, token-efficient context to LLM agents via MCP (Model Context Protocol) — with zero cloud dependency, zero data egress, and zero git mutation.

[![CI](https://github.com/venki0552/lore-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/venki0552/lore-cli/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE-MIT)

---

```
Agent without lore — "why was the retry limit set to 3?":
  reads config, middleware, services  →  ~25,000 tokens, 45s, hallucinates

Agent with lore — same question:
  lore why "retry limit 3"            →  200 tokens, <200ms, ground truth
```

lore reduces agent token usage by ~95% for history questions by indexing commits with hybrid vector + BM25 search and serving results via a local MCP server.

---

## Installation

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/venki0552/lore-cli/releases):

| Platform | Binary |
| --- | --- |
| Linux x86\_64 | `lore-linux-x86_64` |
| Linux aarch64 | `lore-linux-aarch64` |
| macOS ARM (M1+) | `lore-darwin-arm64` |
| macOS Intel | `lore-darwin-x86_64` |
| Windows x86\_64 | `lore-windows-x86_64.exe` |

Each binary ships with a `.sha256` checksum file.

```bash
# Linux / macOS
sha256sum -c lore-linux-x86_64.sha256
chmod +x lore-linux-x86_64
sudo mv lore-linux-x86_64 /usr/local/bin/lore
```

### Build from Source

Requires **Rust 1.75+** and **Git**.

```bash
git clone https://github.com/venki0552/lore-cli
cd lore-cli
cargo build --release
# output: target/release/lore
```

---

## Quick Start

```bash
cd /path/to/your/repo
lore init          # index history, install git hooks, register MCP
lore why "your question here"
lore doctor        # check setup health
lore serve         # start MCP server for AI agents
```

On first `lore init` the `all-MiniLM-L6-v2` embedding model (~90 MB) is downloaded to `~/.lore/models/` and cached.

---

## Documentation

Full reference documentation is in the [`docs/`](docs/) folder:

| Document | Description |
| --- | --- |
| [Commands](docs/commands.md) | Every CLI command, all flags, and examples |
| [MCP Integration](docs/mcp.md) | Connecting lore to Claude Code, Cursor, Windsurf, and other agents |
| [Configuration](docs/configuration.md) | `~/.lore/config.toml` reference and VS Code settings |
| [Architecture](docs/architecture.md) | Crate structure, data model, and search pipeline |
| [Security](docs/security.md) | Threat model, secret redaction, and injection defense |
| [VS Code Extension](docs/vscode-extension.md) | Building and using the VS Code plugin |

---

## Commands at a Glance

| Command | Description |
| --- | --- |
| `lore init` | Index repo, install git hooks, register MCP |
| `lore why <query>` | Semantic search over git history |
| `lore blame <file>` | Semantic ownership of a file or function |
| `lore bisect <bug>` | Find which commit likely introduced a bug |
| `lore log <query>` | Intent-grouped commit history search |
| `lore diff [base] [head]` | Intent-level diff between two refs |
| `lore revert-risk <commit>` | Blast radius analysis before reverting |
| `lore status` | Index stats and storage breakdown |
| `lore doctor` | Diagnose and fix lore setup issues |
| `lore serve` | Start the MCP server (stdio JSON-RPC 2.0) |
| `lore cleanup` | Remove stale branch indices |
| `lore sync` | Reindex trigger for CI |

See [docs/commands.md](docs/commands.md) for all flags and examples.

---

## Development

```bash
cargo test --workspace          # run all tests
cargo clippy --all -- -D warnings
cargo fmt --all

# try lore against this repo
cargo build --release
./target/release/lore init
./target/release/lore why "why was BM25 added"

LORE_LOG=debug ./target/release/lore why "test query"
```

---

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
