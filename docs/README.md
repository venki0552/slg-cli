# lore Documentation

Welcome to the lore documentation. Use the links below to navigate.

| Document | Description |
| --- | --- |
| [Commands](commands.md) | Every CLI command, all flags, examples |
| [MCP Integration](mcp.md) | Connecting lore to Claude Code, Cursor, Windsurf, and other agents |
| [Configuration](configuration.md) | `~/.lore/config.toml` reference |
| [Architecture](architecture.md) | Crate structure, data model, search pipeline |
| [Security](security.md) | Threat model, secret redaction, injection defense |
| [VS Code Extension](vscode-extension.md) | Building and using the VS Code plugin |

---

## Quick Reference

```bash
lore init                     # Set up lore for this repo
lore why "your question"      # Search git history semantically
lore blame <file>             # Who understands this file and why
lore bisect "bug description" # Find which commit introduced a bug
lore log "topic"              # Intent-grouped commit history
lore diff HEAD~5 HEAD         # Semantic diff between two refs
lore revert-risk <hash>       # Blast radius before reverting
lore status                   # Index health and storage stats
lore doctor                   # Diagnose and fix issues
lore serve                    # Start MCP server for AI agents
lore cleanup                  # Remove stale branch indices
```

All retrieval commands work fully offline — no LLM API key required.
