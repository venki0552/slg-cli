# slg Documentation — Semantic Lore for Git

Welcome to the slg documentation. Use the links below to navigate.

| Document                                 | Description                                                       |
| ---------------------------------------- | ----------------------------------------------------------------- |
| [Commands](commands.md)                  | Every CLI command, all flags, examples                            |
| [MCP Integration](mcp.md)                | Connecting slg to Claude Code, Cursor, Windsurf, and other agents |
| [Configuration](configuration.md)        | `~/.slg/config.toml` reference                                    |
| [Architecture](architecture.md)          | Crate structure, data model, search pipeline                      |
| [Security](security.md)                  | Threat model, secret redaction, injection defense                 |
| [VS Code Extension](vscode-extension.md) | Building and using the VS Code plugin                             |

---

## Quick Reference

```bash
slg init                     # Set up slg for this repo
slg why "your question"      # Search git history semantically
slg blame <file>             # Who understands this file and why
slg bisect "bug description" # Find which commit introduced a bug
slg log "topic"              # Intent-grouped commit history
slg diff HEAD~5 HEAD         # Semantic diff between two refs
slg revert-risk <hash>       # Blast radius before reverting
slg status                   # Index health and storage stats
slg doctor                   # Diagnose and fix issues
slg serve                    # Start MCP server for AI agents
slg cleanup                  # Remove stale branch indices
```

All retrieval commands work fully offline — no LLM API key required.
