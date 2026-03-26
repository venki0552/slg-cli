# Architecture

lore is a single Rust binary plus a VS Code extension that turns git history into a queryable semantic knowledge base. This document covers the crate structure, data model, storage schema, and the hybrid search pipeline.

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
- [Git Hook Integration](#git-hook-integration)

---

## Design Philosophy

- **Read-only** — lore never modifies the git repository. It only reads git objects and writes to its own `~/.lore/` directory.
- **Local-first** — no data leaves the machine. All models are embedded (no API calls required).
- **Token-efficient** — the search pipeline is designed to return the minimal set of tokens that answer a question, not full file contents.
- **Secure by default** — secrets are redacted before indexing; agent output is CDATA-isolated; all directories are owner-only.

---

## Crate Dependency Graph

```
lore-cli  (binary entry point, CLI parsing)
  ├── lore-core    (shared types, config, errors)
  ├── lore-git     (git history extraction via git2)
  ├── lore-index   (embedding, BM25, SQLite store, search pipeline)
  ├── lore-output  (text / XML / JSON formatters)
  ├── lore-security (secret redaction, path safety)
  └── lore-mcp     (JSON-RPC 2.0 stdio server)

lore-mcp
  └── lore-index   (direct store access for tool calls)

lore-index
  ├── lore-core
  └── lore-security  (redacts before storing)

lore-output
  └── lore-core

lore-git
  └── lore-core

lore-security
  └── lore-core
```

**`lore-core` has no internal dependencies** — it only uses external crates. All other crates depend on `lore-core`.

---

## Crate Summaries

| Crate           | Responsibility                                                                                         |
| --------------- | ------------------------------------------------------------------------------------------------------ |
| `lore-core`     | Shared `CommitDoc`, `CommitIntent`, `SearchResult`, `LoreConfig`, `LoreError`, `OutputFormat`          |
| `lore-security` | `SecretRedactor` (14 patterns), `safe_index_path` (path traversal prevention)                          |
| `lore-git`      | Reads git history via `git2`; extracts diffs, file lists, linked issues/PRs                            |
| `lore-index`    | `IndexStore` (SQLite WAL), `Embedder` (all-MiniLM-L6-v2), `BM25Index`, `search()` pipeline, `Reranker` |
| `lore-output`   | `format_text`, `format_xml`, `format_json` for `SearchResult` slices                                   |
| `lore-mcp`      | JSON-RPC 2.0 `stdio` server, 5 tool handlers, rate limiter, auto-init detection                        |
| `lore-cli`      | `clap` command definitions; thin wrappers calling the other crates; git hooks; shell completions       |

---

## Data Model — CommitDoc

Every indexed commit is stored as a `CommitDoc` struct:

| Field               | Type             | Description                                                                       |
| ------------------- | ---------------- | --------------------------------------------------------------------------------- |
| `hash`              | `String`         | Full 40-char SHA-1                                                                |
| `short_hash`        | `String`         | 7-char display hash                                                               |
| `message`           | `String`         | Sanitized commit subject line                                                     |
| `body`              | `Option<String>` | Sanitized full message body (if present)                                          |
| `diff_summary`      | `String`         | Per-file intent summaries after secret redaction — raw diffs are **never** stored |
| `author`            | `String`         | Display name only — email is **never** stored                                     |
| `timestamp`         | `i64`            | Unix epoch seconds                                                                |
| `files_changed`     | `Vec<String>`    | File paths touched                                                                |
| `insertions`        | `u32`            | Lines added                                                                       |
| `deletions`         | `u32`            | Lines removed                                                                     |
| `linked_issues`     | `Vec<String>`    | Parsed from "fixes #234", "closes #45"                                            |
| `linked_prs`        | `Vec<String>`    | Parsed from "PR #123"                                                             |
| `intent`            | `CommitIntent`   | Detected from conventional commit prefix + diff                                   |
| `risk_score`        | `f32`            | 0.0–1.0; computed from file sensitivity + churn + deletion ratio                  |
| `branch`            | `String`         | Branch the commit was indexed from                                                |
| `injection_flagged` | `bool`           | LLM steering pattern detected in commit text                                      |
| `secrets_redacted`  | `u32`            | Count of redacted secret patterns (not the values)                                |

### CommitIntent variants

`Fix` · `Feature` · `Refactor` · `Perf` · `Security` · `Docs` · `Test` · `Chore` · `Revert` · `Unknown`

Detected from the conventional commit prefix (`fix:`, `feat:`, `refactor:`, etc.) with fallback to diff heuristics.

---

## Storage Schema

Each `(repo, branch)` pair gets its own SQLite database at:

```
~/.lore/indices/<repo_sha256_hash>/<branch_name>.db
```

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
- Stored at: `~/.lore/models/all-MiniLM-L6-v2.onnx`
- Similarity: cosine similarity stored as inner product (vectors are L2-normalised at index time)

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

```
git log (via git2)
    │  for each commit
    ▼
Extract: hash, message, body, author, timestamp, diff, files
    │
    ▼
lore-security: SecretRedactor.redact(diff_summary)
    │
    ▼
Detect: CommitIntent, risk_score, injection_flagged
    │
    ├──► Embedder.embed_text(message + body + diff_summary) → 384-dim vector
    │
    ├──► BM25Index.index_commit(doc) → bm25_terms, bm25_doc_freq rows
    │
    └──► IndexStore.store_commit(doc, embedding) → commits + commit_embeddings rows
```

The indexer is idempotent — `store_commit` is a no-op if the hash already exists. This makes `lore reindex` safe to run repeatedly.

---

## Git Hook Integration

`lore init` installs a `post-commit` git hook:

```sh
#!/bin/sh
HASH=$(git rev-parse HEAD)
lore _index-commit "$HASH" &
```

The hook runs `lore _index-commit` in the background so it never blocks the commit. `lore reindex` (delta-only, fast path) can also be triggered manually or from the VS Code extension on branch switch.
