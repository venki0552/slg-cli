# Architecture

slg is a single Rust binary plus a VS Code extension that turns git history into a queryable semantic knowledge base. This document covers the crate structure, data model, storage schema, and the hybrid search pipeline.

---

## Contents

- [Design Philosophy](#design-philosophy)
- [Crate Dependency Graph](#crate-dependency-graph)
- [Crate Summaries](#crate-summaries)
- [Data Model — CommitDoc](#data-model--commitdoc)
- [Storage Schema](#storage-schema)
- [Search Pipeline](#search-pipeline)
  - [1. Embedding (Dense Retrieval)](#1-embedding-dense-retrieval)
  - [2. BM25 (Sparse Retrieval)](#2-bm25-sparse-retrieval)
  - [3. RRF Fusion](#3-rrf-fusion)
  - [4. Filters](#4-filters)
  - [5. Boosts](#5-boosts)
  - [6. Token Budget](#6-token-budget)
  - [7. Optional Re-ranking](#7-optional-re-ranking)
- [Indexing Pipeline](#indexing-pipeline)
  - [SQLite Performance Configuration](#sqlite-performance-configuration)
- [MCP Auto-Init](#mcp-auto-init)
- [Git Hook Integration](#git-hook-integration)
  - [Shell Auto-Index](#shell-auto-index)

---

## Design Philosophy

- **Read-only** — slg never modifies the git repository. It only reads git objects and writes to its own `~/.slg/` directory.
- **Local-first** — no data leaves the machine. All models are embedded (no API calls required).
- **Token-efficient** — the search pipeline is designed to return the minimal set of tokens that answer a question, not full file contents.
- **Secure by default** — secrets are redacted before indexing; agent output is CDATA-isolated; all directories are owner-only.

---

## Crate Dependency Graph

```
slg  (binary entry point, CLI parsing)
  ├── slg-core    (shared types, config, errors)
  ├── slg-git     (git history extraction via git2)
  ├── slg-index   (embedding, BM25, SQLite store, search pipeline)
  ├── slg-output  (text / XML / JSON formatters)
  ├── slg-security (secret redaction, path safety)
  └── slg-mcp     (JSON-RPC 2.0 stdio server)

slg-mcp
  └── slg-index   (direct store access for tool calls)

slg-index
  ├── slg-core
  └── slg-security  (redacts before storing)

slg-output
  └── slg-core

slg-git
  └── slg-core

slg-security
  └── slg-core
```

**`slg-core` has no internal dependencies** — it only uses external crates. All other crates depend on `slg-core`.

---

## Crate Summaries

| Crate          | Responsibility                                                                                                |
| -------------- | ------------------------------------------------------------------------------------------------------------- |
| `slg-core`     | Shared `CommitDoc`, `CommitIntent`, `SearchResult`, `SlgConfig`, `SlgError`, `OutputFormat`                   |
| `slg-security` | `SecretRedactor` (14 patterns), `safe_index_path` (path traversal prevention)                                 |
| `slg-git`      | Reads git history via `git2`; extracts diffs, file lists, linked issues/PRs                                   |
| `slg-index`    | `IndexStore` (SQLite WAL), `Embedder` (all-MiniLM-L6-v2), `BM25Index`, `search()` pipeline, `Reranker`        |
| `slg-output`   | `format_text`, `format_xml`, `format_json` for `SearchResult` slices                                          |
| `slg-mcp`      | JSON-RPC 2.0 `stdio` server, 5 tool handlers, rate limiter, MCP auto-init (background indexing on first call) |
| `slg`          | `clap` command definitions; thin wrappers calling the other crates; git hooks; shell completions              |

---

## Data Model — CommitDoc

Every indexed commit is stored as a `CommitDoc` struct:

| Field               | Type             | Description                                                                         |
| ------------------- | ---------------- | ----------------------------------------------------------------------------------- |
| `hash`              | `String`         | Full 40-char SHA-1                                                                  |
| `short_hash`        | `String`         | 7-char display hash                                                                 |
| `message`           | `String`         | Sanitized commit subject line                                                       |
| `body`              | `Option<String>` | Sanitized full message body (if present)                                            |
| `diff_summary`      | `String`         | Per-file intent summaries after secret redaction — raw diffs are **never** stored   |
| `author`            | `String`         | Display name only — email is **never** stored                                       |
| `timestamp`         | `i64`            | Unix epoch seconds                                                                  |
| `files_changed`     | `Vec<String>`    | File paths touched                                                                  |
| `insertions`        | `u32`            | Lines added                                                                         |
| `deletions`         | `u32`            | Lines removed                                                                       |
| `linked_issues`     | `Vec<String>`    | Parsed from "fixes #234", "closes #45"                                              |
| `linked_prs`        | `Vec<String>`    | Parsed from "PR #123"                                                               |
| `intent`            | `CommitIntent`   | Detected from conventional commit prefix + diff                                     |
| `risk_score`        | `f32`            | 0.0–1.0; computed from file sensitivity + churn + deletion ratio                    |
| `branch`            | `String`         | Branch the commit was indexed from. For detached HEAD: `HEAD-DETACHED-{short_hash}` |
| `injection_flagged` | `bool`           | LLM steering pattern detected in commit text                                        |
| `secrets_redacted`  | `u32`            | Count of redacted secret patterns (not the values)                                  |

### CommitIntent variants

`Fix` · `Feature` · `Refactor` · `Perf` · `Security` · `Docs` · `Test` · `Chore` · `Revert` · `Unknown`

Detected from the conventional commit prefix (`fix:`, `feat:`, `refactor:`, etc.) with fallback to diff heuristics.

---

## Storage Schema

Each `(repo, branch)` pair gets its own SQLite database at:

```
~/.slg/indices/<repo_sha256_hash>/<branch_name>.db
```

**`repo_sha256_hash`** is computed as:

- `SHA256(remote origin URL)` if the repo has an `origin` remote — stable across machines that share the same remote.
- `SHA256(absolute local path)` if there is no remote — unique per machine.

**`branch_name`** is sanitized before use as a filename (character allowlist, 64-char cap, path traversal checks). See [Path Security](security.md#path-security) for the full rules.

SQLite is opened in **WAL (Write-Ahead Log)** mode for concurrent reads. Tables:

| Table               | Purpose                                                            |
| ------------------- | ------------------------------------------------------------------ |
| `commits`           | One row per `CommitDoc`; all metadata fields                       |
| `commit_embeddings` | `BLOB` of packed `f32` values (384 dimensions) per hash            |
| `bm25_terms`        | `(hash, term, tf)` — per-document term frequencies                 |
| `bm25_doc_freq`     | `(term, doc_freq, total_docs)` — corpus-level document frequencies |
| `file_signals`      | `(file_path, commit_hash, churn_score)` — per-file churn for blame |
| `meta`              | Key-value pairs; stores `schema_version`, `last_indexed_commit`    |

Indexes:

- `idx_commits_timestamp` — for recency filtering
- `idx_commits_author` — for author filtering
- `idx_commits_intent` — for intent grouping
- `idx_bm25_terms_term` — for BM25 lookups

---

## Search Pipeline

The hybrid search pipeline runs entirely in-process with no network calls:

```
Query string
    │
    ▼
┌─────────────────────────────────────────┐
│  1. Embed query → 384-dim dense vector  │  (all-MiniLM-L6-v2, ONNX)
└───────────────────┬─────────────────────┘
                    │
        ┌───────────┴────────────┐
        ▼                        ▼
┌──────────────┐        ┌─────────────────┐
│ 2. Vector    │        │ 3. BM25 lexical  │
│    search    │        │    search        │
│ top-N cos    │        │ k1=1.5, b=0.75  │
│ similarity   │        │                  │
└──────┬───────┘        └────────┬────────┘
       │                         │
       └──────────┬──────────────┘
                  ▼
        ┌──────────────────┐
        │ 4. RRF fusion    │   1/(k + rank_v) + 1/(k + rank_b)
        │    k = 60.0      │   combines both ranked lists
        └────────┬─────────┘
                 ▼
        ┌──────────────────┐
        │ 5. Filters       │   since, until, author, module
        └────────┬─────────┘
                 ▼
        ┌──────────────────┐
        │ 6. Boosts        │   recency ×1.2, exact match ×1.5,
        │                  │   security query ×1.3
        └────────┬─────────┘
                 ▼
        ┌──────────────────┐
        │ 7. Token budget  │   accumulate until max_tokens reached
        └────────┬─────────┘
                 ▼
        ┌──────────────────┐
        │ 8. Re-rank       │   optional cross-encoder pass
        │    (optional)    │   (enable_reranker = true)
        └────────┬─────────┘
                 ▼
          SearchResult[]
```

### 1. Embedding (Dense Retrieval)

- Model: **all-MiniLM-L6-v2** (384 dimensions, ~23 MB)
- Runtime: ONNX via `ort` crate (CPU inference, no GPU required)
- Stored at: `~/.slg/models/all-MiniLM-L6-v2.onnx`
- Similarity: cosine similarity computed at query time; the query norm is pre-computed once and reused across all candidates (`cosine_similarity_prenorm`)

### 2. BM25 (Sparse Retrieval)

- Parameters: **k1 = 1.5**, **b = 0.75** (standard corpus values)
- Text corpus: `message + body + files_changed + linked_issues`
- Tokenization: lowercase → split on non-alphanumeric → remove stopwords → 2–50 char length filter → deduplicate
- Stopwords: `the a an is are was were be been to of and or in on at for with by this that it its we our i you he she`

### 3. RRF Fusion

Reciprocal Rank Fusion combines the two ranked lists without requiring score normalisation:

$$\text{RRF}(d) = \frac{1}{k + \text{rank}_{vector}(d)} + \frac{1}{k + \text{rank}_{bm25}(d)}$$

where **k = 60.0** (standard RRF constant). Documents appearing in only one list get rank = ∞ for the missing list (contributing 0).

### 4. Filters

Applied after fusion, before boosts:

| Filter   | Activated by                                          |
| -------- | ----------------------------------------------------- |
| `since`  | `--since <date>` / `since` tool parameter             |
| `until`  | `--until <date>`                                      |
| `author` | `--author <name>` / `author` tool parameter           |
| `module` | `--module <path>` (filters by `files_changed` prefix) |

### 5. Boosts

| Boost       | Multiplier | Condition                                                                                 |
| ----------- | ---------- | ----------------------------------------------------------------------------------------- |
| Recency     | ×1.2       | Commit is within the last 30 days                                                         |
| Exact match | ×1.5       | All query words appear verbatim in the commit message                                     |
| Security    | ×1.3       | Query contains security keywords (`security`, `vuln`, `cve`, `exploit`, `attack`, `auth`) |

### 6. Token Budget

Results are accumulated in ranked order until `max_tokens` is reached (default: 4096). This means the caller always receives a response within a predictable size even if there are many high-relevance results.

### 7. Optional Re-ranking

When `enable_reranker = true` in config (or `--rerank` flag), a cross-encoder model scores each candidate against the original query to produce a more precise final ranking. Disabled by default due to added latency.

---

## Indexing Pipeline

`slg init` and `slg index` run a **3-stage concurrent streaming pipeline** — the three stages overlap in real time so git I/O, ONNX inference, and SQLite writes all proceed simultaneously:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Indexing Pipeline (streaming)                     │
│                                                                      │
│  Stage 1: Git Ingestion           chan(512)                          │
│  ─────────────────────────────── ══════════ ──────────────────────  │
│  git2::revwalk → raw commits                                         │
│  SecretRedactor.redact()          → CommitDoc → ...                 │
│  CommitSanitizer.sanitize()                                          │
│  detect intent, risk_score                                           │
│                                                                      │
│  Stage 2: Embedder (single ONNX)  chan(32)                           │
│  ─────────────────────────────── ══════════ ──────────────────────  │
│  filter already-indexed (commit_exists)                              │
│  batch=256 → Embedder.embed_batch()    → (docs, embeddings) → ...   │
│  ort ONNX session, CPU inference                                     │
│                                                                      │
│  Stage 3: DB Writer                                                  │
│  ─────────────────────────────────────────────────────────────────  │
│  IndexStore.store_batch()                                            │
│  BM25Index.index_commit()                                            │
└─────────────────────────────────────────────────────────────────────┘
```

Key properties:

- **Single ONNX session** — multiple sessions cause CPU contention; one session with large batches maximises SIMD throughput.
- **Batch size 256** — text is truncated to 400 chars for diffs and 800 chars total before embedding to fit the model's 256-token window.
- **INSERT OR IGNORE** — `store_batch` uses `INSERT OR IGNORE` instead of a prior `SELECT` existence check, making batch writes idempotent and faster.
- **pipeline overhead is negative** in the analytics output because the three stages run concurrently — embedding and BM25 wall-clock time overlap.

### SQLite Performance Configuration

The index store is opened with these PRAGMAs on every connection:

| PRAGMA         | Value       | Effect                                |
| -------------- | ----------- | ------------------------------------- |
| `journal_mode` | `WAL`       | Concurrent readers while writing      |
| `synchronous`  | `NORMAL`    | Safe with WAL; reduces fsync overhead |
| `cache_size`   | `-64000`    | 64 MB page cache in RAM               |
| `mmap_size`    | `268435456` | 256 MB memory-mapped I/O              |
| `temp_store`   | `MEMORY`    | Temporary tables stay in RAM          |
| `page_size`    | `4096`      | Matches typical OS page size          |

All hot-path SQL statements use `prepare_cached()` from the rusqlite statement cache (capacity 32).

The indexer is idempotent — `store_batch`/`store_commit` use `INSERT OR IGNORE`, so `slg reindex` is always safe to run repeatedly.

---

## MCP Auto-Init

When an AI agent makes its first tool call and no index exists yet, the MCP server **automatically starts background indexing** without requiring the user to run `slg init` first:

```
Agent tool call (slg_why, slg_blame, etc.)
    │
    ▼
index_exists()? ──No──► spawn_background_index() (INIT_ONCE guard)
    │                         │
    │                         ▼
    │                    3-stage streaming pipeline
    │                    AtomicU64 progress counter
    │
    ▼
Return: {"status":"initializing","message":"slg index is being built..."}
    │
    ▼ (agent retries later)
index_exists()? ──Yes──► normal tool call
```

- `std::sync::Once` guarantees background indexing starts exactly once per process.
- `AtomicBool` + `AtomicU64` counters track progress without locking.
- The initializing response tells the agent to retry — it includes an `eta_seconds` hint.

---

## Git Hook Integration

`slg init` installs four git hooks — `post-commit`, `post-checkout`, `post-merge`, and `post-rewrite`. Each runs:

```sh
# slg semantic index hook — slg.sh — DO NOT EDIT THIS BLOCK
slg reindex --delta-only --background --silent 2>/dev/null &
# end slg hook
```

- The hook block is **appended** to any existing hook rather than overwriting it.
- If a hook file already contains a slg block, it is updated in-place (idempotent).
- The `&` detaches the process so the hook never blocks the git operation.
- `slg reindex --delta-only` uses `git2` merge-base walk to find only commits not yet in the index, making it fast even on large repos.

### Shell Auto-Index

`slg init --global` also installs a shell integration function. When you `cd` into a git repo that has no index file, it automatically starts `slg index --background --silent`:

| Shell      | Mechanism                                      |
| ---------- | ---------------------------------------------- |
| Zsh        | `add-zsh-hook chpwd _slg_chpwd`                |
| Bash       | `PROMPT_COMMAND="_slg_chpwd; $PROMPT_COMMAND"` |
| Fish       | `function _slg_chpwd --on-variable PWD`        |
| PowerShell | Not yet supported — run `slg index` manually   |
