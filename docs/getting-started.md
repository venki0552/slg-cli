# Getting Started

This guide takes you from zero to running your first `slg why` query in under five minutes.

---

## 1. Install the binary

Pick the method that matches how you work.

### Option A — npx (no install required)

If you have Node.js 18+ installed, this is the fastest way to get started and works identically on Linux, macOS, and Windows:

```bash
# Run any slg command directly — binary is downloaded on first use
npx slg-cli init
npx slg-cli why "why was the retry limit set to 3"
```

The binary is cached at `~/.slg/bin/slg` after the first download. Every subsequent `npx slg-cli` call is instant — it just execs the cached binary.

To permanently add `slg` to your PATH so you can drop the `npx slg-cli` prefix:

```bash
npx slg-cli install
# Follow the PATH instructions it prints
```

Or install globally via npm:

```bash
npm install -g slg-cli
slg init  # now usable everywhere without npx
```

---

### Option B — Download a pre-built binary

Go to the [latest GitHub release](https://github.com/venki0552/slg/releases/latest) and download the file for your platform:

| Platform                       | File to download         |
| ------------------------------ | ------------------------ |
| Linux x86_64                   | `slg-linux-x86_64`       |
| Linux ARM64                    | `slg-linux-aarch64`      |
| macOS Apple Silicon (M1/M2/M3) | `slg-darwin-arm64`       |
| macOS Intel                    | `slg-darwin-x86_64`      |
| Windows x86_64                 | `slg-windows-x86_64.exe` |

Each release also ships a `.sha256` file. **Verify the checksum before running the binary.**

::: code-group

```bash [Linux / macOS]
# Download (replace filename with your platform's binary)
curl -LO https://github.com/venki0552/slg/releases/latest/download/slg-linux-x86_64
curl -LO https://github.com/venki0552/slg/releases/latest/download/slg-linux-x86_64.sha256

# Verify checksum
sha256sum -c slg-linux-x86_64.sha256

# Install
chmod +x slg-linux-x86_64
sudo mv slg-linux-x86_64 /usr/local/bin/slg

# Confirm
slg --version
```

```powershell [Windows (PowerShell)]
# Download
Invoke-WebRequest -Uri https://github.com/venki0552/slg/releases/latest/download/slg-windows-x86_64.exe -OutFile slg.exe
Invoke-WebRequest -Uri https://github.com/venki0552/slg/releases/latest/download/slg-windows-x86_64.exe.sha256 -OutFile slg.exe.sha256

# Verify checksum
$expected = (Get-Content slg.exe.sha256).Split()[0]
$actual   = (Get-FileHash slg.exe -Algorithm SHA256).Hash.ToLower()
if ($expected -eq $actual) { Write-Host "Checksum OK" } else { Write-Error "Checksum MISMATCH" }

# Move to a folder on your PATH (e.g. C:\Tools\)
Move-Item slg.exe C:\Tools\slg.exe

# Confirm
slg --version
```

:::

### Option C — VS Code extension (zero manual steps)

Install the **slg** VS Code extension from the Marketplace. On first activation it automatically downloads the correct binary for your platform, verifies its SHA-256 checksum, and runs `slg init` in the background. You don't need to touch a terminal.

### Option D — Build from source

Requires [Rust 1.75+](https://rustup.rs/) and Git.

```bash
git clone https://github.com/venki0552/slg
cd slg
cargo build --release

# The binary is at:
./target/release/slg        # Linux / macOS
.\target\release\slg.exe    # Windows

# Optionally copy it to your PATH
sudo cp target/release/slg /usr/local/bin/slg
```

---

## 2. Initialize slg in your repo

Navigate to any git repository (needs at least a few commits):

```bash
cd /path/to/your/repo
slg init
```

`slg init` does three things:

1. **Downloads the embedding model** — `all-MiniLM-L6-v2` (~90 MB) is saved to `~/.slg/models/` and cached for all repos. This only happens once.
2. **Indexes your git history** — walks every commit on the current branch, redacts secrets, embeds each commit, and stores the index at `~/.slg/indices/<repo-hash>/`.
3. **Installs git hooks** — a `post-commit` hook keeps the index up to date automatically as you commit.

For large repos (10 000+ commits) the initial index can take a minute or two. Run it in the background while you work:

```bash
slg init --background
```

---

## 3. Ask your first question

```bash
slg why "why was the retry limit set to 3"
```

slg searches your git history semantically and returns the most relevant commits with their context — not a guess, actual git history.

More examples:

```bash
# Find who owns a file and why
slg blame src/auth.rs

# Find which commit likely introduced a bug
slg bisect "login fails after the JWT refactor"

# Browse history grouped by intent (feat, fix, refactor…)
slg log "authentication"

# Check what's indexed and how much space it uses
slg status

# Full health check
slg doctor
```

---

## 4. Connect your AI agent (optional)

Run `slg serve` to start the MCP server, then add it to your agent's config:

::: code-group

```json [Claude Code (~/.claude/claude_desktop_config.json)]
{
	"mcpServers": {
		"slg": { "command": "slg", "args": ["serve"] }
	}
}
```

```json [Cursor (~/.cursor/mcp.json)]
{
	"mcpServers": {
		"slg": { "command": "slg", "args": ["serve"] }
	}
}
```

```json [Windsurf (~/.windsurf/mcp.json)]
{
	"mcpServers": {
		"slg": { "command": "slg", "args": ["serve"] }
	}
}
```

```json [GitHub Copilot (.vscode/mcp.json)]
{
	"servers": {
		"slg": { "type": "stdio", "command": "slg", "args": ["serve"] }
	}
}
```

:::

Restart your agent. It will discover 5 read-only slg tools: `slg_why`, `slg_blame`, `slg_log`, `slg_bisect`, and `slg_status`.

> **VS Code extension** handles this automatically if `slg.autoRegisterMCP` is enabled (default: `true`).

See [MCP Integration](mcp.md) for the full tool reference.

---

## 5. Keep the index current

slg installs a `post-commit` hook that indexes new commits automatically. If you switch branches manually:

```bash
slg reindex       # fast delta-only reindex for the current branch
```

For CI pipelines:

```bash
slg sync          # same as reindex, designed for non-interactive use
```

---

## Troubleshooting

**`command not found: slg`**
The binary is not on your `PATH`. Either move it to `/usr/local/bin/` or add its directory to `PATH`.

**`slg: index not found`**
You haven't run `slg init` in this repo yet, or you're in a directory that isn't a git repository.

**Model download fails**
slg downloads `all-MiniLM-L6-v2` from HuggingFace on first run. Check your internet connection, then retry with:

```bash
slg init --force
```

**Something else looks wrong**

```bash
slg doctor
```

`slg doctor` checks your binary version, model, index, hooks, and MCP config and prints exactly what needs fixing.

---

## Next steps

- [Command reference](commands.md) — every flag for every command
- [MCP Integration](mcp.md) — full tool schemas for AI agents
- [Configuration](configuration.md) — tune token limits, rate limits, cleanup policy
