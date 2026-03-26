# lore Commands

Reference for every `lore` command. All commands must be run inside a git repository (or one of its subdirectories) unless noted otherwise.

---

## Global Flags

These flags work on every command:

| Flag | Default | Description |
| --- | --- | --- |
| `--format <fmt>` | `text` | Output format: `text`, `xml`, or `json` |
| `--max-tokens <N>` | `4096` | Truncate output to N tokens (always returns at least 1 result) |
| `--silent` | off | Suppress all non-result output (progress, labels, hints) |

---

## `lore init`

Initialize lore for the current git repository. This is the only command you need to run once per repo. It:

1. Creates `~/.lore/` directory structure
2. Downloads the `all-MiniLM-L6-v2` embedding model (~90 MB, one-time download)
3. Indexes all commits on the current branch
4. Installs git hooks (`post-commit`, `post-checkout`, `post-merge`, `post-rewrite`)
5. Optionally installs shell integration

```bash
lore init
```

**Flags:**

| Flag | Description |
| --- | --- |
| `--global` | Also install shell integration into your shell RC file (zsh/bash/fish/PowerShell) |
| `--background` | Run the initial indexing in the background, return immediately |
| `--mcp-only` | Only register MCP configuration; skip hooks and indexing |
| `--hooks-only` | Only install git hooks; skip indexing and MCP |
| `--shell-only` | Only install shell integration; skip everything else |
| `--silent` | Suppress all output (used internally by hooks) |

**Example output:**

```
lore init
Git root: /home/user/myproject
Repo hash: a3f19c2d

✓ Created /home/user/.lore/
✓ Git hooks installed

Indexing branch 'main'...
⠴ Indexing commits [842]
✓ Indexed 842 commits

Index path: /home/user/.lore/indices/a3f19c2d/main.db
→ Run `lore why "your question"` to search git history
```

**After `lore init`, future commits are indexed automatically** via the installed git hooks. You do not need to run `lore index` again unless you want to force a full re-index.

---

## `lore index`

Explicitly run a full index of the current branch. Walks all commits through libgit2, sanitizes, embeds, and stores them. Use this if you want to force a complete re-index.

```bash
lore index
lore index --background
```

**Flags:**

| Flag | Description |
| --- | --- |
| `--background` | Run indexing in background, return immediately |
| `--silent` | Suppress progress output |

**When to use:** After resetting the index, switching to a new machine, or if `lore doctor` reports index issues.

---

## `lore reindex`

Perform a delta-only reindex — only indexes commits that aren't already in the store. Much faster than a full index. This is what the installed git hooks call automatically after each commit, checkout, merge, and rewrite.

```bash
lore reindex
lore reindex --delta-only
```

**Flags:**

| Flag | Description |
| --- | --- |
| `--delta-only` | Only index new commits (default behavior, included for explicitness) |
| `--background` | Run in background |
| `--silent` | Suppress output |

**When to use:** Manually trigger when you've pulled commits and want the index updated immediately without waiting for a hook.

---

## `lore why`

Semantic search over your indexed git history. Answers the question "why does this code exist?" by finding the commits most relevant to your query using hybrid vector + BM25 search.

```bash
lore why "why is the retry limit set to 3?"
lore why "JWT expiry"
lore why "rate limiting added to auth endpoint" --limit 5
lore why "database migrations" --since 2025-01-01
lore why "refactoring payments module" --author "Alice"
lore why "config changes" --module src/config
lore why "security patches" --format json --max-tokens 2000
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `<query>` | Yes | Free-text semantic query (max 500 characters) |

**Flags:**

| Flag | Default | Description |
| --- | --- | --- |
| `--limit <N>` | `3` | Number of results to return (max limited by token budget) |
| `--since <date>` | — | Only include commits after this date. Format: `YYYY-MM-DD` |
| `--author <name>` | — | Filter results to this author (substring match) |
| `--module <path>` | — | Filter to commits that touched files under this path |
| `--max-tokens <N>` | `4096` | Truncate output to N tokens |
| `--rerank` | off | Enable cross-encoder reranking for higher precision (~50ms extra) |
| `--format` | `text` | `text`, `xml`, or `json` |

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
lore why — "retry limit set to 3"  (3 results, 47ms)

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

## `lore blame`

Semantic ownership analysis for a file or function. Finds which commits introduced or meaningfully changed the target, ranked by relevance. More useful than `git blame` for understanding *why* something is the way it is.

```bash
lore blame src/auth/session.rs
lore blame src/payments/processor.ts --fn charge_card
lore blame src/api/routes.py --risk
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `<file>` | Yes | File path to analyze (relative to repo root) |

**Flags:**

| Flag | Description |
| --- | --- |
| `--fn <name>` | Focus on a specific function name within the file |
| `--risk` | Include risk scores in output |
| `--format` | `text`, `xml`, or `json` |
| `--max-tokens <N>` | Truncate output |

**What it does internally:** Builds a search query like `"changes to file src/auth/session.rs"` (or `"changes to function charge_card in file src/payments/processor.ts"`), then runs the full hybrid search filtered to that file path.

---

## `lore bisect`

Semantic pre-filter for bug hunting. Given a description of a bug, finds the commits most likely to have introduced it. Faster and more targeted than running `git bisect` blind over hundreds of commits.

```bash
lore bisect "null pointer in user login flow"
lore bisect "payments return 500 for non-US addresses" --limit 10
lore bisect "websocket connections drop after 30 seconds" --format json
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `<bug_description>` | Yes | Description of the bug (free text) |

**Flags:**

| Flag | Default | Description |
| --- | --- | --- |
| `--limit <N>` | `5` | Max candidate commits to return |
| `--format` | `text` | `text`, `xml`, or `json` |
| `--max-tokens <N>` | `4096` | Truncate output |

**Workflow:** Use the candidates returned by `lore bisect` as the initial suspect range for `git bisect`. This reduces the range from thousands of commits to 5–10 meaningful candidates.

---

## `lore log`

Intent-grouped semantic git log. Searches commit history by topic and groups results by detected intent (Fix, Feature, Refactor, etc.). More useful than `git log --grep` for exploratory history browsing.

```bash
lore log "authentication changes"
lore log "database schema" --since 2025-06-01
lore log "performance optimizations" --limit 20 --by-intent
lore log "security" --format json
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `<query>` | Yes | Search query |

**Flags:**

| Flag | Default | Description |
| --- | --- | --- |
| `--since <date>` | — | Filter commits after `YYYY-MM-DD` |
| `--limit <N>` | `10` | Number of results |
| `--by-intent` | off | Group results by detected intent |
| `--format` | `text` | `text`, `xml`, or `json` |
| `--max-tokens <N>` | `8192` | Truncate output (default is higher than `why` for browsing) |

---

## `lore diff`

Intent-level diff between two git refs. Instead of raw line-by-line output, shows the semantic purpose of each commit in the range. Useful for understanding what a PR or release actually *changed* in intent terms.

```bash
lore diff                        # defaults to HEAD~1..HEAD
lore diff HEAD~5 HEAD
lore diff v1.0.0 v1.1.0
lore diff main feature/payments
lore diff abc1234 def5678 --breaking-only
```

**Arguments:**

| Argument | Default | Description |
| --- | --- | --- |
| `<base>` | `HEAD~1` | Base ref (older end of range) |
| `<head>` | `HEAD` | Head ref (newer end of range) |

**Flags:**

| Flag | Description |
| --- | --- |
| `--breaking-only` | Only show commits tagged as breaking changes |
| `--silent` | Suppress non-result output |
| `--format` | `text`, `xml`, or `json` |

**Note:** If `base` and `head` resolve to the same commit, lore returns an explicit "no changes" message rather than an empty result.

---

## `lore revert-risk`

Blast radius analysis before reverting a commit. Finds all other commits that are semantically related to the target, giving you a picture of the potential fallout if you revert.

```bash
lore revert-risk a3f19c2
lore revert-risk HEAD
lore revert-risk v2.3.1 --format json
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `<commit>` | Yes | Commit hash (full or 7+ chars), branch name, tag, or ref like `HEAD` |

**Flags:**

| Flag | Description |
| --- | --- |
| `--silent` | Suppress non-result output |
| `--format` | `text`, `xml`, or `json` |
| `--max-tokens <N>` | Truncate output |

**Note:** Symbolic refs (`HEAD`, `HEAD~1`, tag names, branch names) are automatically resolved to their commit hash. Both short (7+ char) and full hashes are accepted directly.

---

## `lore status`

Show the current index status for the repository, including storage usage across all indexed branches.

```bash
lore status
lore status --format json
```

**Flags:**

| Flag | Description |
| --- | --- |
| `--format` | `text`, `xml`, or `json` |

**Example output (text format):**

```
lore status

Repository: /home/user/myproject
Branch:     main
Repo hash:  a3f19c2d

✓ Index active
  Path:    /home/user/.lore/indices/a3f19c2d/main.db
  Commits: 842
  Size:    1.3 MB

Storage: /home/user/.lore/
  Branches indexed: 4
  Total size:       4.8 MB
```

---

## `lore cleanup`

Remove stale branch index files to reclaim disk space. Only deletes `.db` files that haven't been accessed within the threshold. Your git history is never touched.

```bash
lore cleanup                    # remove indices older than 7 days
lore cleanup --older-than 30
lore cleanup --dry-run          # preview without deleting
```

**Flags:**

| Flag | Default | Description |
| --- | --- | --- |
| `--older-than <N>` | `7` | Remove indices not accessed in the last N days |
| `--dry-run` | off | Show what would be deleted without actually deleting |

---

## `lore doctor`

Run a health check on your lore installation. Reports the status of each component and optionally fixes common issues automatically.

```bash
lore doctor
lore doctor --fix-all
```

**Flags:**

| Flag | Description |
| --- | --- |
| `--fix-all` | Automatically fix detected issues (re-installs hooks, recreates index if missing) |

**Checks performed:**

| Check | What it verifies |
| --- | --- |
| Binary version | Current installed version |
| lore home | `~/.lore/` directory exists |
| Models directory | `~/.lore/models/` exists (embedding model downloaded) |
| Git repository | Current directory is inside a git repo |
| Git hooks | `post-commit`, `post-checkout`, `post-merge`, `post-rewrite` hooks installed |
| Index | Index file exists for current branch |
| Shell | Detected shell type (zsh, bash, fish, PowerShell, or unknown) |

**Example output:**

```
lore doctor

✓ lore version: 0.1.0
✓ lore home: /home/user/.lore/
✓ Models directory exists
✓ Git repo: /home/user/myproject
✓ Git hooks installed
✓ Index exists: /home/user/.lore/indices/a3f19c2d/main.db
  Shell: Zsh

All checks passed!
```

---

## `lore serve` / `lore mcp`

Start the MCP server. Reads JSON-RPC 2.0 requests from stdin and writes responses to stdout. Used by AI agents (Claude Code, Cursor, Windsurf). `lore mcp` is an alias for `lore serve`.

```bash
lore serve
lore mcp
```

These commands have no user-facing flags. They are invoked by your agent's MCP configuration, not directly from the terminal. See [MCP Integration](mcp.md) for setup instructions.

**MCP server limits:**
- Rate limit: 60 requests per minute (per process)
- Max output per response: 50,000 bytes
- Request timeout: 5 seconds

---

## `lore sync`

Manually trigger a delta reindex. Equivalent to `lore reindex --delta-only`. Intended for CI pipelines where git hooks are not available.

```bash
lore sync
lore sync --silent
```

**Flags:**

| Flag | Description |
| --- | --- |
| `--silent` | Suppress output |

**CI usage example:**

```yaml
# .github/workflows/some-workflow.yml
- name: Update lore index
  run: lore sync --silent
```

---

## Internal Commands

These commands are used by git hooks and the VS Code extension. They are hidden from `lore --help` and not intended for direct use.

| Command | Description |
| --- | --- |
| `lore _health` | Machine-readable health check (JSON). Used by VS Code status bar. |
| `lore _repo-hash` | Print stable repo hash for current directory. |
| `lore _index-commit <hash>` | Index a single commit by SHA. Called by `post-commit` hook. |
| `lore _index-path` | Print the index file path for the current repo and branch. |

---

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Error (message printed to stderr) |

Errors include: not in a git repository, no index found (run `lore init`), query too long, index schema mismatch (run `lore sync --reindex`), security violation (check `~/.lore/security.log`).
