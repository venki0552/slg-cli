# VS Code Extension

The slg VS Code extension (`slg-sh.slg`) gives you slg's git-history intelligence directly inside your editor — automatic indexing, a live status bar, branch-change detection, and automatic MCP registration with AI agents.

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
- Internet access (first activation only, to download the slg binary)

---

## Installation

**From the VS Code Marketplace** (when published):

```
ext install slg-sh.slg
```

**From the VSIX file** (manual install):

```bash
# Build first (see Building from Source below)
code --install-extension slg-0.1.0.vsix
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

1. **Ensure binary** — checks whether the slg binary is installed and matches the expected version (`0.1.0`). If not, downloads it for the current platform (see [Binary Management](#binary-management)).
2. **Create status bar** — shows the slg status indicator in the bottom-left of the VS Code window (see [Status Bar](#status-bar)).
3. **Background indexing** — if `slg.indexOnActivation` is `true` (default), runs `slg init --background --silent` in the workspace directory. The status bar shows a spinner while indexing is in progress.
4. **Install watchers** — starts a file-system watcher on `.git/HEAD` to detect branch switches (see [Branch Watching](#branch-watching)).
5. **Register MCP** — if `slg.autoRegisterMCP` is `true` (default), writes the slg server entry to every supported AI agent config file found on disk (see [MCP Auto-Registration](#mcp-auto-registration)).
6. **Register commands** — makes the three extension commands available in the Command Palette (see [Commands](#commands)).

---

## Status Bar

The status bar item in the bottom-left of VS Code reflects the current state of the slg index. Click it to run `slg doctor`.

| State        | Display                         | Meaning                              |
| ------------ | ------------------------------- | ------------------------------------ |
| `indexing`   | `⟳ slg: indexing...` (spinner) | Full index build in progress         |
| `reindexing` | `⟳ slg: ↻ <branch>` (spinner)  | Delta reindex on branch switch       |
| `ready`      | `✓ slg: <branch> ✓ <N>MB`      | Index ready; shows branch and size   |
| `error`      | `⚠ slg: ⚠ <message>`           | An error occurred; click for details |
| `mcp_down`   | `✗ slg: MCP ✗`                 | MCP server health check failed       |
| `no_index`   | `⊘ slg: not indexed`           | No index found; run `slg init`      |

The extension polls `slg _health` every **30 seconds** to keep the status bar up to date.

---

## Commands

Three commands are available in the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

| Command                     | Command ID     | Description                                                                   |
| --------------------------- | -------------- | ----------------------------------------------------------------------------- |
| **slg: Run Doctor**        | `slg.doctor`  | Runs `slg doctor` and shows a notification with any issues found             |
| **slg: Show Status**       | `slg.status`  | Opens an output channel and prints the full `slg doctor` output              |
| **slg: Reindex Workspace** | `slg.reindex` | Manually triggers `slg init --background --silent` for the current workspace |

---

## Branch Watching

The extension watches `.git/HEAD` for changes. When a branch switch is detected, it automatically runs `slg reindex` in the background to update the index for the new branch:

1. File-system watcher on `.git/HEAD` fires.
2. Extension compares the current branch name with the previously known branch.
3. If changed, status bar transitions to `reindexing` state and `slg reindex` is run.
4. Once complete, status bar transitions back to `ready`.

This means the slg index is always current for whatever branch you are on, with no manual steps needed.

---

## Binary Management

The extension manages the slg binary automatically. On first activation (or when the installed version is outdated), it:

1. Detects the current platform and architecture.
2. Downloads the appropriate binary from the GitHub releases page.
3. Verifies the binary's **SHA-256 checksum** against the `.sha256` file published alongside the release.
4. Marks the binary executable (Unix only) and caches it in VS Code's global extension storage.

Supported platforms:

| Platform | Architecture          | Binary name               |
| -------- | --------------------- | ------------------------- |
| Linux    | x86_64                | `slg-linux-x86_64`       |
| Linux    | ARM64                 | `slg-linux-aarch64`      |
| macOS    | ARM64 (Apple Silicon) | `slg-darwin-arm64`       |
| macOS    | x86_64 (Intel)        | `slg-darwin-x86_64`      |
| Windows  | x86_64                | `slg-windows-x86_64.exe` |

If the platform is not supported, an error notification is shown and the extension does not activate further.

The binary is stored in VS Code's global extension storage — not in `PATH` — so it does not interfere with any system-installed slg binary.

---

## MCP Auto-Registration

When `slg.autoRegisterMCP` is `true` (default), the extension automatically adds the slg MCP server entry to every AI agent config file whose **parent directory exists** on disk. This means the agent must already be installed; the extension will not create directories.

| Agent       | Config file written                    |
| ----------- | -------------------------------------- |
| Claude Code | `~/.claude/claude_desktop_config.json` |
| Cursor      | `~/.cursor/mcp.json`                   |
| Windsurf    | `~/.windsurf/mcp.json`                 |

The extension merges the slg entry into the existing config — it does not overwrite other MCP servers. If the entry is already present and correct, it is left unchanged.

To disable auto-registration:

```json
{ "slg.autoRegisterMCP": false }
```

For manual configuration instructions, see [docs/mcp.md](mcp.md#manual-agent-configuration).

---

## Settings Reference

All settings are under the `slg.` namespace:

| Setting                  | Type                            | Default | Description                                                            |
| ------------------------ | ------------------------------- | ------- | ---------------------------------------------------------------------- |
| `slg.autoRegisterMCP`   | boolean                         | `true`  | Auto-register slg MCP with Claude Code, Cursor, and Windsurf          |
| `slg.cleanupAfterDays`  | number                          | `7`     | Delete stale branch indices after N days of inactivity                 |
| `slg.outputFormat`      | `"text"` \| `"xml"` \| `"json"` | `"xml"` | Default output format for MCP responses                                |
| `slg.enableReranker`    | boolean                         | `false` | Enable cross-encoder reranking (~50 ms added latency, better accuracy) |
| `slg.indexOnActivation` | boolean                         | `true`  | Automatically index the workspace when VS Code opens                   |
| `slg.showStatusBar`     | boolean                         | `true`  | Show the slg status bar item                                          |

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
# produces: slg-0.1.0.vsix

# Install locally
code --install-extension slg-0.1.0.vsix
```

To develop with hot-reload:

```bash
cd plugin
npm run watch
# Then press F5 in VS Code to open the Extension Development Host
```
