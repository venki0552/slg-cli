# slg Commands

Reference for every `slg` command. All commands must be run inside a git repository (or one of its subdirectories) unless noted otherwise.

---

## Global Flags

These flags work on every command:

| Flag               | Default | Description                                                    |
| ------------------ | ------- | -------------------------------------------------------------- |
| `--format <fmt>`   | `text`  | Output format: `text`, `xml`, or `json`                        |
| `--max-tokens <N>` | `4096`  | Truncate output to N tokens (always returns at least 1 result) |
| `--silent`         | off     | Suppress all non-result output (progress, labels, hints)       |

---

## `slg init`

Initialize slg for the current git repository. This is the only command you need to run once per repo. It:

1. Creates `~/.slg/` directory structure
2. Downloads the `all-MiniLM-L6-v2` embedding model (~90 MB, one-time download)
3. Indexes all commits on the current branch
4. Installs git hooks (`post-commit`, `post-checkout`, `post-merge`, `post-rewrite`)
5. Optionally installs shell integration

```bash
slg init
```

**Flags:**

| Flag           | Description                                                                         |
| -------------- | ----------------------------------------------------------------------------------- |
| `--global`     | Also install shell integration into your shell RC file (zsh/bash/fish/PowerShell)   |
| `--background` | Run the initial indexing in the background, return immediately                      |
| `--mcp-only`   | Only register MCP configuration; skip hooks and indexing                            |
| `--hooks-only` | Only install git hooks; skip indexing and MCP                                       |
| `--shell-only` | Only install shell integration; skip everything else                                |
| `--fix-all`    | Automatically fix all detected issues (re-install hooks, recreate index if missing) |
| `--silent`     | Suppress all output (used internally by hooks)                                      |

**Example output:**

```
slg init
Git root: /home/user/myproject
Repo hash: a3f19c2d

✓ Created /home/user/.slg/
✓ Git hooks installed

Indexing branch 'main' at /home/user/myproject
  [embed] Model loaded in 338ms
  Indexed 842 commits [842]

  ┌──────────────────────────────────────────┐
  │          Indexing Analytics               │
  ├──────────────────────────────────────────┤
  │  Total time:          65.2s              │
  │  Commits ingested:      842               │
  │  Commits skipped:         0               │
  │  Commits embedded:      842               │
  │  Commits stored:        842               │
  ├──────────────────────────────────────────┤
  │  Embedding time:       57.1s (87.6%)     │
  │  DB write time:         0.7s ( 1.1%)     │
  │  BM25 index time:      42.3s (64.9%)     │
  │  Pipeline overhead:    -34.9s             │
  ├──────────────────────────────────────────┤
  │  Embed throughput:     14.7 commits/s     │
  │  Overall rate:         12.9 commits/s     │
  │  Avg embed/commit:     67.8ms             │
  │  Avg write/commit:      0.8ms             │
  │  Avg BM25/commit:      50.2ms             │
  └──────────────────────────────────────────┘

Index stored at: /home/user/.slg/indices/a3f19c2d.../main.db
→ Run `slg why "your question"` to search git history
```

The **pipeline overhead** line is negative because the 3 stages (git ingestion, ONNX embedding, DB write) run concurrently as a streaming pipeline — embedding and BM25 indexing overlap in real time, so their measured times sum to more than wall-clock elapsed.

**After `slg init`, future commits are indexed automatically** via the installed git hooks. You do not need to run `slg index` again unless you want to force a full re-index.

Explicitly run a full index of the current branch. Walks all commits through libgit2, sanitizes, embeds, and stores them. Use this if you want to force a complete re-index. Prints the same analytics table as `slg init` when complete.

```bash
slg index
slg index --background
```

**Flags:**

| Flag           | Description                                    |
| -------------- | ---------------------------------------------- |
| `--background` | Run indexing in background, return immediately |
| `--silent`     | Suppress progress output                       |

**When to use:** After resetting the index, switching to a new machine, or if `slg doctor` reports index issues.

> **Detached HEAD:** If the repo is in detached HEAD state, the branch name used for the index is `HEAD-DETACHED-{short_hash}`. Indexing proceeds normally but the index won't be shared with a named branch.

---

## `slg reindex`

Perform a delta-only reindex — only indexes commits that aren't already in the store. Much faster than a full index. This is what the installed git hooks call automatically after each commit, checkout, merge, and rewrite.

```bash
slg reindex
slg reindex --delta-only
```

**Flags:**

| Flag           | Description                                                          |
| -------------- | -------------------------------------------------------------------- |
| `--delta-only` | Only index new commits (default behavior, included for explicitness) |
| `--background` | Run in background                                                    |
| `--silent`     | Suppress output                                                      |

**When to use:** Manually trigger when you've pulled commits and want the index updated immediately without waiting for a hook.

---

## `slg why`

Semantic search over your indexed git history. Answers the question "why does this code exist?" by finding the commits most relevant to your query using hybrid vector + BM25 search.

```bash
slg why "why is the retry limit set to 3?"
slg why "JWT expiry"
slg why "rate limiting added to auth endpoint" --limit 5
slg why "database migrations" --since 2025-01-01
slg why "refactoring payments module" --author "Alice"
slg why "config changes" --module src/config
slg why "security patches" --format json --max-tokens 2000
```

**Arguments:**

| Argument  | Required | Description                                   |
| --------- | -------- | --------------------------------------------- |
| `<query>` | Yes      | Free-text semantic query (max 500 characters) |

**Flags:**

| Flag               | Default | Description                                                       |
| ------------------ | ------- | ----------------------------------------------------------------- |
| `--limit <N>`      | `3`     | Number of results to return (max limited by token budget)         |
| `--since <date>`   | —       | Only include commits after this date. Format: `YYYY-MM-DD`        |
| `--author <name>`  | —       | Filter results to this author (substring match)                   |
| `--module <path>`  | —       | Filter to commits that touched files under this path              |
| `--max-tokens <N>` | `4096`  | Truncate output to N tokens                                       |
| `--rerank`         | off     | Enable cross-encoder reranking for higher precision (~50ms extra) |
| `--format`         | `text`  | `text`, `xml`, or `json`                                          |

**How search works:**

1. Your query is embedded into a 384-dimensional vector using `all-MiniLM-L6-v2`
2. Vector similarity search finds semantically close commits
3. BM25 lexical search finds keyword matches
4. Both lists are merged using Reciprocal Rank Fusion (RRF, k=60)
5. Boosts are applied: recent commits (+20%), exact token match (+50%), security-tagged commits (+30%)
6. Filters (`--since`, `--author`, `--module`) narrow the result set
7. Token budget is enforced — at least 1 result is always returned

**Example output (text format):**

```
slg why — "retry limit set to 3"  (3 results, 47ms)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
#1  a3f19c2  fix: cap retry attempts at 3 to prevent thundering herd
    Author: Alice Chen  ·  2024-11-14
    Intent: Fix  ·  Risk: 0.32
    Files: src/http/client.rs, src/config/defaults.rs
    ─────────────────────────────────────────
    http/client.rs: reduced default retry count from 10 to 3 after
    production incident. config/defaults.rs: added RETRY_MAX constant
    with comment referencing incident post-mortem.
```

---

## `slg blame`

Semantic ownership analysis for a file or function. Finds which commits introduced or meaningfully changed the target, ranked by relevance. More useful than `git blame` for understanding _why_ something is the way it is.

```bash
slg blame src/auth/session.rs
slg blame src/payments/processor.ts --fn charge_card
slg blame src/api/routes.py --risk
```

**Arguments:**

| Argument | Required | Description                                  |
| -------- | -------- | -------------------------------------------- |
| `<file>` | Yes      | File path to analyze (relative to repo root) |

**Flags:**

| Flag               | Description                                       |
| ------------------ | ------------------------------------------------- |
| `--fn <name>`      | Focus on a specific function name within the file |
| `--risk`           | Include risk scores in output                     |
| `--format`         | `text`, `xml`, or `json`                          |
| `--max-tokens <N>` | Truncate output                                   |

**What it does internally:** Builds a search query like `"changes to file src/auth/session.rs"` (or `"changes to function charge_card in file src/payments/processor.ts"`), then runs the full hybrid search filtered to that file path.

---

## `slg bisect`

Semantic pre-filter for bug hunting. Given a description of a bug, finds the commits most likely to have introduced it. Faster and more targeted than running `git bisect` blind over hundreds of commits.

```bash
slg bisect "null pointer in user login flow"
slg bisect "payments return 500 for non-US addresses" --limit 10
slg bisect "websocket connections drop after 30 seconds" --format json
```

**Arguments:**

| Argument            | Required | Description                        |
| ------------------- | -------- | ---------------------------------- |
| `<bug_description>` | Yes      | Description of the bug (free text) |

**Flags:**

| Flag               | Default | Description                     |
| ------------------ | ------- | ------------------------------- |
| `--limit <N>`      | `5`     | Max candidate commits to return |
| `--format`         | `text`  | `text`, `xml`, or `json`        |
| `--max-tokens <N>` | `4096`  | Truncate output                 |

**Workflow:** Use the candidates returned by `slg bisect` as the initial suspect range for `git bisect`. This reduces the range from thousands of commits to 5–10 meaningful candidates.

---

## `slg log`

Intent-grouped semantic git log. Searches commit history by topic and groups results by detected intent (Fix, Feature, Refactor, etc.). More useful than `git log --grep` for exploratory history browsing.

```bash
slg log "authentication changes"
slg log "database schema" --since 2025-06-01
slg log "performance optimizations" --limit 20 --by-intent
slg log "security" --format json
```

**Arguments:**

| Argument  | Required | Description  |
| --------- | -------- | ------------ |
| `<query>` | Yes      | Search query |

**Flags:**

| Flag               | Default | Description                                                 |
| ------------------ | ------- | ----------------------------------------------------------- |
| `--since <date>`   | —       | Filter commits after `YYYY-MM-DD`                           |
| `--limit <N>`      | `10`    | Number of results                                           |
| `--by-intent`      | off     | Group results by detected intent                            |
| `--format`         | `text`  | `text`, `xml`, or `json`                                    |
| `--max-tokens <N>` | `8192`  | Truncate output (default is higher than `why` for browsing) |

---

## `slg diff`

Intent-level diff between two git refs. Instead of raw line-by-line output, shows the semantic purpose of each commit in the range. Useful for understanding what a PR or release actually _changed_ in intent terms.

```bash
slg diff                        # defaults to HEAD~1..HEAD
slg diff HEAD~5 HEAD
slg diff v1.0.0 v1.1.0
slg diff main feature/payments
slg diff abc1234 def5678 --breaking-only
```

**Arguments:**

| Argument | Default  | Description                   |
| -------- | -------- | ----------------------------- |
| `<base>` | `HEAD~1` | Base ref (older end of range) |
| `<head>` | `HEAD`   | Head ref (newer end of range) |

**Flags:**

| Flag              | Description                                  |
| ----------------- | -------------------------------------------- |
| `--breaking-only` | Only show commits tagged as breaking changes |
| `--silent`        | Suppress non-result output                   |
| `--format`        | `text`, `xml`, or `json`                     |

**Note:** If `base` and `head` resolve to the same commit, slg returns an explicit "no changes" message rather than an empty result.

---

## `slg revert-risk`

Blast radius analysis before reverting a commit. Finds all other commits that are semantically related to the target, giving you a picture of the potential fallout if you revert.

```bash
slg revert-risk a3f19c2
slg revert-risk HEAD
slg revert-risk v2.3.1 --format json
```

**Arguments:**

| Argument   | Required | Description                                                          |
| ---------- | -------- | -------------------------------------------------------------------- |
| `<commit>` | Yes      | Commit hash (full or 7+ chars), branch name, tag, or ref like `HEAD` |

**Flags:**

| Flag               | Description                |
| ------------------ | -------------------------- |
| `--silent`         | Suppress non-result output |
| `--format`         | `text`, `xml`, or `json`   |
| `--max-tokens <N>` | Truncate output            |

**Note:** Symbolic refs (`HEAD`, `HEAD~1`, tag names, branch names) are automatically resolved to their commit hash. Both short (7+ char) and full hashes are accepted directly.

---

## `slg status`

Show the current index status for the repository, including storage usage across all indexed branches.

```bash
slg status
slg status --format json
```

**Flags:**

| Flag       | Description              |
| ---------- | ------------------------ |
| `--format` | `text`, `xml`, or `json` |

**Example output (text format):**

```
slg status

Repository: /home/user/myproject
Branch:     main
Repo hash:  a3f19c2d

✓ Index active
  Path:    /home/user/.slg/indices/a3f19c2d/main.db
  Commits: 842
  Size:    1.3 MB

Storage: /home/user/.slg/
  Branches indexed: 4
  Total size:       4.8 MB
```

---

## `slg cleanup`

Remove stale branch index files to reclaim disk space. Only deletes `.db` files that haven't been accessed within the threshold. Your git history is never touched.

```bash
slg cleanup                    # remove indices older than 7 days
slg cleanup --older-than 30
slg cleanup --dry-run          # preview without deleting
```

**Flags:**

| Flag               | Default | Description                                          |
| ------------------ | ------- | ---------------------------------------------------- |
| `--older-than <N>` | `7`     | Remove indices not accessed in the last N days       |
| `--dry-run`        | off     | Show what would be deleted without actually deleting |

---

## `slg doctor`

Run a health check on your slg installation. Reports the status of each component and optionally fixes common issues automatically.

```bash
slg doctor
slg doctor --fix-all
```

**Flags:**

| Flag        | Description                                                                       |
| ----------- | --------------------------------------------------------------------------------- |
| `--fix-all` | Automatically fix detected issues (re-installs hooks, recreates index if missing) |

**Checks performed:**

| Check            | What it verifies                                                             |
| ---------------- | ---------------------------------------------------------------------------- |
| Binary version   | Current installed version                                                    |
| slg home         | `~/.slg/` directory exists                                                   |
| Models directory | `~/.slg/models/` exists (embedding model downloaded)                         |
| Git repository   | Current directory is inside a git repo                                       |
| Git hooks        | `post-commit`, `post-checkout`, `post-merge`, `post-rewrite` hooks installed |
| Index            | Index file exists for current branch                                         |
| Shell            | Detected shell type (zsh, bash, fish, PowerShell, or unknown)                |

**Example output:**

```
slg doctor

✓ slg version: 0.1.0
✓ slg home: /home/user/.slg/
✓ Models directory exists
✓ Git repo: /home/user/myproject
✓ Git hooks installed
✓ Index exists: /home/user/.slg/indices/a3f19c2d/main.db
  Shell: Zsh

All checks passed!
```

---

## `slg serve` / `slg mcp`

Start the MCP server. Reads JSON-RPC 2.0 requests from stdin and writes responses to stdout. Used by AI agents (Claude Code, Cursor, Windsurf). `slg mcp` is an alias for `slg serve`.

```bash
slg serve
slg mcp
```

These commands have no user-facing flags. They are invoked by your agent's MCP configuration, not directly from the terminal. See [MCP Integration](mcp.md) for setup instructions.

**MCP server limits:**

- Rate limit: 60 requests per minute (per process)
- Max output per response: 50,000 bytes
- Request timeout: 15 seconds

---

## `slg sync`

Manually trigger a delta reindex. Equivalent to `slg reindex --delta-only`. Intended for CI pipelines where git hooks are not available.

```bash
slg sync
slg sync --silent
```

**Flags:**

| Flag       | Description     |
| ---------- | --------------- |
| `--silent` | Suppress output |

**CI usage example:**

```yaml
# .github/workflows/some-workflow.yml
- name: Update slg index
  run: slg sync --silent
```

---

## Internal Commands

These commands are used by git hooks and the VS Code extension. They are hidden from `slg --help` and not intended for direct use.

| Command                    | Description                                                       |
| -------------------------- | ----------------------------------------------------------------- |
| `slg _health`              | Machine-readable health check (JSON). Used by VS Code status bar. |
| `slg _repo-hash`           | Print stable repo hash for current directory.                     |
| `slg _index-commit <hash>` | Index a single commit by SHA. Called by `post-commit` hook.       |
| `slg _index-path`          | Print the index file path for the current repo and branch.        |

---

## Exit Codes

| Code | Meaning                           |
| ---- | --------------------------------- |
| `0`  | Success                           |
| `1`  | Error (message printed to stderr) |

Errors include: not in a git repository, no index found (run `slg init`), query too long, index schema mismatch (run `slg sync --reindex`), security violation (check `~/.slg/security.log`).
