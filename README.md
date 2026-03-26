# slg — Semantic Lore for Git

**Semantic git intelligence for LLM agents.**

slg (Semantic Lore for Git) is a single Rust binary that transforms your git history into a queryable semantic knowledge base. It serves precise, token-efficient context to LLM agents via MCP (Model Context Protocol) — with zero cloud dependency, zero data egress, and zero git mutation.

[![CI](https://github.com/venki0552/slg/actions/workflows/ci.yml/badge.svg)](https://github.com/venki0552/slg/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](https://github.com/venki0552/slg/blob/main/LICENSE-MIT)

---

```
Agent without slg — "why was the retry limit set to 3?":
  reads config, middleware, services  →  ~25,000 tokens, 45s, hallucinates

Agent with slg — same question:
  slg why "retry limit 3"            →  200 tokens, <200ms, ground truth
```

slg reduces agent token usage by ~95% for history questions by indexing commits with hybrid vector + BM25 search and serving results via a local MCP server.

---

## Installation

### npx (no install required)

If you have Node.js 18+:

```bash
npx slg-cli init
npx slg-cli why "your question"
```

The binary is downloaded once, SHA-256 verified, and cached at `~/.slg/bin/slg`. Every subsequent call is instant. To add `slg` to your PATH permanently:

```bash
npx slg-cli install      # downloads + prints PATH setup instructions
# or
npm install -g slg-cli   # installs the proxy globally
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/venki0552/slg/releases):

| Platform        | Binary                   |
| --------------- | ------------------------ |
| Linux x86_64    | `slg-linux-x86_64`       |
| Linux aarch64   | `slg-linux-aarch64`      |
| macOS ARM (M1+) | `slg-darwin-arm64`       |
| macOS Intel     | `slg-darwin-x86_64`      |
| Windows x86_64  | `slg-windows-x86_64.exe` |

Each binary ships with a `.sha256` checksum file.

```bash
# Linux / macOS
sha256sum -c slg-linux-x86_64.sha256
chmod +x slg-linux-x86_64
sudo mv slg-linux-x86_64 /usr/local/bin/slg
```

### Build from Source

Requires **Rust 1.75+** and **Git**.

```bash
git clone https://github.com/venki0552/slg
cd slg
cargo build --release
# output: target/release/slg
```

---

## Quick Start

```bash
cd /path/to/your/repo
slg init                        # index history, install git hooks, register MCP
slg why "your question here"    # search git history semantically
slg doctor                      # check setup health
slg serve                       # start MCP server for AI agents
```

On first `slg init` the `all-MiniLM-L6-v2` embedding model (~90 MB) is downloaded to `~/.slg/models/` and cached.

See the **[Getting Started guide](docs/getting-started.md)** for step-by-step installation on all platforms, AI agent setup, and troubleshooting.

---

## Documentation

Full reference documentation is in the [`docs/`](docs/commands) folder:

| Document                                      | Description                                                       |
| --------------------------------------------- | ----------------------------------------------------------------- |
| [Getting Started](docs/getting-started.md)    | Installation on all platforms, first run, AI agent setup          |
| [Commands](docs/commands.md)                  | Every CLI command, all flags, and examples                        |
| [MCP Integration](docs/mcp.md)                | Connecting slg to Claude Code, Cursor, Windsurf, and other agents |
| [Configuration](docs/configuration.md)        | `~/.slg/config.toml` reference and VS Code settings               |
| [Architecture](docs/architecture.md)          | Crate structure, data model, and search pipeline                  |
| [Security](docs/security.md)                  | Threat model, secret redaction, and injection defense             |
| [VS Code Extension](docs/vscode-extension.md) | Building and using the VS Code plugin                             |

---

## Commands at a Glance

| Command                    | Description                                 |
| -------------------------- | ------------------------------------------- |
| `slg init`                 | Index repo, install git hooks, register MCP |
| `slg why <query>`          | Semantic search over git history            |
| `slg blame <file>`         | Semantic ownership of a file or function    |
| `slg bisect <bug>`         | Find which commit likely introduced a bug   |
| `slg log <query>`          | Intent-grouped commit history search        |
| `slg diff [base] [head]`   | Intent-level diff between two refs          |
| `slg revert-risk <commit>` | Blast radius analysis before reverting      |
| `slg status`               | Index stats and storage breakdown           |
| `slg doctor`               | Diagnose and fix slg setup issues           |
| `slg serve`                | Start the MCP server (stdio JSON-RPC 2.0)   |
| `slg cleanup`              | Remove stale branch indices                 |
| `slg sync`                 | Reindex trigger for CI                      |

See [docs/commands.md](docs/commands.md) for all flags and examples.

---

## Development

```bash
cargo test --workspace          # run all tests
cargo clippy --all -- -D warnings
cargo fmt --all

# try slg against this repo
cargo build --release
./target/release/slg init
./target/release/slg why "why was BM25 added"

SLG_LOG=debug ./target/release/slg why "test query"
```

---

## License

Licensed under either of [MIT](https://github.com/venki0552/slg/blob/main/LICENSE-MIT) or [Apache 2.0](https://github.com/venki0552/slg/blob/main/LICENSE-APACHE) at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
