# VS Code Extension

The lore VS Code extension (`lore-sh.lore`) gives you lore's git-history intelligence directly inside your editor — automatic indexing, a live status bar, branch-change detection, and automatic MCP registration with AI agents.

---

## Contents

- [Requirements](#requirements)
- [Installation](#installation)
- [Activation](#activation)
- [Lifecycle on Startup](#lifecycle-on-startup)
- [Status Bar](#status-bar)
- [Commands](#commands)
- [Branch Watching](#branch-watching)
- [Binary Management](#binary-management)
- [MCP Auto-Registration](#mcp-auto-registration)
- [Settings Reference](#settings-reference)
- [Building from Source](#building-from-source)

---

## Requirements

- VS Code 1.85.0 or later
- A workspace with a `.git` directory
- Internet access (first activation only, to download the lore binary)

---

## Installation

**From the VS Code Marketplace** (when published):

```
ext install lore-sh.lore
```

**From the VSIX file** (manual install):

```bash
# Build first (see Building from Source below)
code --install-extension lore-0.1.0.vsix
```

---

## Activation

The extension activates automatically when:

- A workspace is opened that contains a `.git` directory (`workspaceContains:.git`), or
- VS Code finishes starting up (`onStartupFinished`)

No manual activation is required.

---

## Lifecycle on Startup

When the extension activates, it performs these steps in order:

1. **Ensure binary** — checks whether the lore binary is installed and matches the expected version (`0.1.0`). If not, downloads it for the current platform (see [Binary Management](#binary-management)).
2. **Create status bar** — shows the lore status indicator in the bottom-left of the VS Code window (see [Status Bar](#status-bar)).
3. **Background indexing** — if `lore.indexOnActivation` is `true` (default), runs `lore init --background --silent` in the workspace directory. The status bar shows a spinner while indexing is in progress.
4. **Install watchers** — starts a file-system watcher on `.git/HEAD` to detect branch switches (see [Branch Watching](#branch-watching)).
5. **Register MCP** — if `lore.autoRegisterMCP` is `true` (default), writes the lore server entry to every supported AI agent config file found on disk (see [MCP Auto-Registration](#mcp-auto-registration)).
6. **Register commands** — makes the three extension commands available in the Command Palette (see [Commands](#commands)).

---

## Status Bar

The status bar item in the bottom-left of VS Code reflects the current state of the lore index. Click it to run `lore doctor`.

| State        | Display                         | Meaning                              |
| ------------ | ------------------------------- | ------------------------------------ |
| `indexing`   | `⟳ lore: indexing...` (spinner) | Full index build in progress         |
| `reindexing` | `⟳ lore: ↻ <branch>` (spinner)  | Delta reindex on branch switch       |
| `ready`      | `✓ lore: <branch> ✓ <N>MB`      | Index ready; shows branch and size   |
| `error`      | `⚠ lore: ⚠ <message>`           | An error occurred; click for details |
| `mcp_down`   | `✗ lore: MCP ✗`                 | MCP server health check failed       |
| `no_index`   | `⊘ lore: not indexed`           | No index found; run `lore init`      |

The extension polls `lore _health` every **30 seconds** to keep the status bar up to date.

---

## Commands

Three commands are available in the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

| Command                     | Command ID     | Description                                                                   |
| --------------------------- | -------------- | ----------------------------------------------------------------------------- |
| **lore: Run Doctor**        | `lore.doctor`  | Runs `lore doctor` and shows a notification with any issues found             |
| **lore: Show Status**       | `lore.status`  | Opens an output channel and prints the full `lore doctor` output              |
| **lore: Reindex Workspace** | `lore.reindex` | Manually triggers `lore init --background --silent` for the current workspace |

---

## Branch Watching

The extension watches `.git/HEAD` for changes. When a branch switch is detected, it automatically runs `lore reindex` in the background to update the index for the new branch:

1. File-system watcher on `.git/HEAD` fires.
2. Extension compares the current branch name with the previously known branch.
3. If changed, status bar transitions to `reindexing` state and `lore reindex` is run.
4. Once complete, status bar transitions back to `ready`.

This means the lore index is always current for whatever branch you are on, with no manual steps needed.

---

## Binary Management

The extension manages the lore binary automatically. On first activation (or when the installed version is outdated), it:

1. Detects the current platform and architecture.
2. Downloads the appropriate binary from the GitHub releases page.
3. Verifies the binary's **SHA-256 checksum** against the `.sha256` file published alongside the release.
4. Marks the binary executable (Unix only) and caches it in VS Code's global extension storage.

Supported platforms:

| Platform | Architecture          | Binary name               |
| -------- | --------------------- | ------------------------- |
| Linux    | x86_64                | `lore-linux-x86_64`       |
| Linux    | ARM64                 | `lore-linux-aarch64`      |
| macOS    | ARM64 (Apple Silicon) | `lore-darwin-arm64`       |
| macOS    | x86_64 (Intel)        | `lore-darwin-x86_64`      |
| Windows  | x86_64                | `lore-windows-x86_64.exe` |

If the platform is not supported, an error notification is shown and the extension does not activate further.

The binary is stored in VS Code's global extension storage — not in `PATH` — so it does not interfere with any system-installed lore binary.

---

## MCP Auto-Registration

When `lore.autoRegisterMCP` is `true` (default), the extension automatically adds the lore MCP server entry to every AI agent config file whose **parent directory exists** on disk. This means the agent must already be installed; the extension will not create directories.

| Agent       | Config file written                    |
| ----------- | -------------------------------------- |
| Claude Code | `~/.claude/claude_desktop_config.json` |
| Cursor      | `~/.cursor/mcp.json`                   |
| Windsurf    | `~/.windsurf/mcp.json`                 |

The extension merges the lore entry into the existing config — it does not overwrite other MCP servers. If the entry is already present and correct, it is left unchanged.

To disable auto-registration:

```json
{ "lore.autoRegisterMCP": false }
```

For manual configuration instructions, see [docs/mcp.md](mcp.md#manual-agent-configuration).

---

## Settings Reference

All settings are under the `lore.` namespace:

| Setting                  | Type                            | Default | Description                                                            |
| ------------------------ | ------------------------------- | ------- | ---------------------------------------------------------------------- |
| `lore.autoRegisterMCP`   | boolean                         | `true`  | Auto-register lore MCP with Claude Code, Cursor, and Windsurf          |
| `lore.cleanupAfterDays`  | number                          | `7`     | Delete stale branch indices after N days of inactivity                 |
| `lore.outputFormat`      | `"text"` \| `"xml"` \| `"json"` | `"xml"` | Default output format for MCP responses                                |
| `lore.enableReranker`    | boolean                         | `false` | Enable cross-encoder reranking (~50 ms added latency, better accuracy) |
| `lore.indexOnActivation` | boolean                         | `true`  | Automatically index the workspace when VS Code opens                   |
| `lore.showStatusBar`     | boolean                         | `true`  | Show the lore status bar item                                          |

---

## Building from Source

```bash
# Prerequisites: Node.js 18+, npm

cd plugin
npm install
npm run compile

# Package as VSIX
npm install -g @vscode/vsce
vsce package
# produces: lore-0.1.0.vsix

# Install locally
code --install-extension lore-0.1.0.vsix
```

To develop with hot-reload:

```bash
cd plugin
npm run watch
# Then press F5 in VS Code to open the Extension Development Host
```
