# Getting Started

This guide takes you from zero to running your first `lore why` query in under five minutes.

---

## 1. Install the binary

Pick the method that matches how you work.

### Option A — Download a pre-built binary (fastest)

Go to the [latest GitHub release](https://github.com/venki0552/lore-cli/releases/latest) and download the file for your platform:

| Platform | File to download |
|---|---|
| Linux x86\_64 | `lore-linux-x86_64` |
| Linux ARM64 | `lore-linux-aarch64` |
| macOS Apple Silicon (M1/M2/M3) | `lore-darwin-arm64` |
| macOS Intel | `lore-darwin-x86_64` |
| Windows x86\_64 | `lore-windows-x86_64.exe` |

Each release also ships a `.sha256` file. **Verify the checksum before running the binary.**

::: code-group

```bash [Linux / macOS]
# Download (replace filename with your platform's binary)
curl -LO https://github.com/venki0552/lore-cli/releases/latest/download/lore-linux-x86_64
curl -LO https://github.com/venki0552/lore-cli/releases/latest/download/lore-linux-x86_64.sha256

# Verify checksum
sha256sum -c lore-linux-x86_64.sha256

# Install
chmod +x lore-linux-x86_64
sudo mv lore-linux-x86_64 /usr/local/bin/lore

# Confirm
lore --version
```

```powershell [Windows (PowerShell)]
# Download
Invoke-WebRequest -Uri https://github.com/venki0552/lore-cli/releases/latest/download/lore-windows-x86_64.exe -OutFile lore.exe
Invoke-WebRequest -Uri https://github.com/venki0552/lore-cli/releases/latest/download/lore-windows-x86_64.exe.sha256 -OutFile lore.exe.sha256

# Verify checksum
$expected = (Get-Content lore.exe.sha256).Split()[0]
$actual   = (Get-FileHash lore.exe -Algorithm SHA256).Hash.ToLower()
if ($expected -eq $actual) { Write-Host "Checksum OK" } else { Write-Error "Checksum MISMATCH" }

# Move to a folder on your PATH (e.g. C:\Tools\)
Move-Item lore.exe C:\Tools\lore.exe

# Confirm
lore --version
```

:::

### Option B — VS Code extension (zero manual steps)

Install the **lore** VS Code extension from the Marketplace. On first activation it automatically downloads the correct binary for your platform, verifies its SHA-256 checksum, and runs `lore init` in the background. You don't need to touch a terminal.

### Option C — Build from source

Requires [Rust 1.75+](https://rustup.rs/) and Git.

```bash
git clone https://github.com/venki0552/lore-cli
cd lore-cli
cargo build --release

# The binary is at:
./target/release/lore        # Linux / macOS
.\target\release\lore.exe    # Windows

# Optionally copy it to your PATH
sudo cp target/release/lore /usr/local/bin/lore
```

---

## 2. Initialize lore in your repo

Navigate to any git repository (needs at least a few commits):

```bash
cd /path/to/your/repo
lore init
```

`lore init` does three things:

1. **Downloads the embedding model** — `all-MiniLM-L6-v2` (~90 MB) is saved to `~/.lore/models/` and cached for all repos. This only happens once.
2. **Indexes your git history** — walks every commit on the current branch, redacts secrets, embeds each commit, and stores the index at `~/.lore/indices/<repo-hash>/`.
3. **Installs git hooks** — a `post-commit` hook keeps the index up to date automatically as you commit.

For large repos (10 000+ commits) the initial index can take a minute or two. Run it in the background while you work:

```bash
lore init --background
```

---

## 3. Ask your first question

```bash
lore why "why was the retry limit set to 3"
```

lore searches your git history semantically and returns the most relevant commits with their context — not a guess, actual git history.

More examples:

```bash
# Find who owns a file and why
lore blame src/auth.rs

# Find which commit likely introduced a bug
lore bisect "login fails after the JWT refactor"

# Browse history grouped by intent (feat, fix, refactor…)
lore log "authentication"

# Check what's indexed and how much space it uses
lore status

# Full health check
lore doctor
```

---

## 4. Connect your AI agent (optional)

Run `lore serve` to start the MCP server, then add it to your agent's config:

::: code-group

```json [Claude Code (~/.claude/claude_desktop_config.json)]
{
  "mcpServers": {
    "lore": { "command": "lore", "args": ["serve"] }
  }
}
```

```json [Cursor (~/.cursor/mcp.json)]
{
  "mcpServers": {
    "lore": { "command": "lore", "args": ["serve"] }
  }
}
```

```json [Windsurf (~/.windsurf/mcp.json)]
{
  "mcpServers": {
    "lore": { "command": "lore", "args": ["serve"] }
  }
}
```

```json [GitHub Copilot (.vscode/mcp.json)]
{
  "servers": {
    "lore": { "type": "stdio", "command": "lore", "args": ["serve"] }
  }
}
```

:::

Restart your agent. It will discover 5 read-only lore tools: `lore_why`, `lore_blame`, `lore_log`, `lore_bisect`, and `lore_status`.

> **VS Code extension** handles this automatically if `lore.autoRegisterMCP` is enabled (default: `true`).

See [MCP Integration](mcp.md) for the full tool reference.

---

## 5. Keep the index current

lore installs a `post-commit` hook that indexes new commits automatically. If you switch branches manually:

```bash
lore reindex       # fast delta-only reindex for the current branch
```

For CI pipelines:

```bash
lore sync          # same as reindex, designed for non-interactive use
```

---

## Troubleshooting

**`command not found: lore`**
The binary is not on your `PATH`. Either move it to `/usr/local/bin/` or add its directory to `PATH`.

**`lore: index not found`**
You haven't run `lore init` in this repo yet, or you're in a directory that isn't a git repository.

**Model download fails**
lore downloads `all-MiniLM-L6-v2` from HuggingFace on first run. Check your internet connection, then retry with:
```bash
lore init --force
```

**Something else looks wrong**
```bash
lore doctor
```
`lore doctor` checks your binary version, model, index, hooks, and MCP config and prints exactly what needs fixing.

---

## Next steps

- [Command reference](commands.md) — every flag for every command
- [MCP Integration](mcp.md) — full tool schemas for AI agents
- [Configuration](configuration.md) — tune token limits, rate limits, cleanup policy
