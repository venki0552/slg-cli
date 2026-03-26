# Security Design

slg is designed to index git history without leaking secrets, credentials, or sensitive data to the search index or to AI agents. This document describes the threat model and every mitigation layer.

---

## Contents

- [Threat Model](#threat-model)
- [Secret Redaction](#secret-redaction)
- [Path Security](#path-security)
- [Output Isolation (CDATA)](#output-isolation-cdata)
- [File System Permissions](#file-system-permissions)
- [Security Logging](#security-logging)
- [MCP Read-Only Guarantee](#mcp-read-only-guarantee)
- [Injection Flagging](#injection-flagging)

---

## Threat Model

| Threat                                            | Mitigation                               |
| ------------------------------------------------- | ---------------------------------------- |
| Secrets committed to git end up in the AI context | Secret redaction before indexing         |
| Malicious branch names causing path traversal     | Branch name sanitization + prefix check  |
| AI agent output manipulated via commit messages   | CDATA wrapping, injection flagging       |
| Index files readable by other system users        | `0o700` directory permissions (Unix)     |
| Secrets leaked in debug logs                      | Redactor only logs counts, never content |
| Excessive data sent to AI agent                   | 50 KB response cap, token budget         |
| Prompt injection in diff/commit text              | `injection_flagged` column in SQLite     |

---

## Secret Redaction

The `SecretRedactor` in `slg-security` scans every `diff_summary` field **before it is written to SQLite**. Secrets never reach the index. Patterns are matched in order (most-specific first to prevent partial overlap):

| Pattern                         | Replacement token        | Example match                 |
| ------------------------------- | ------------------------ | ----------------------------- |
| AWS access key                  | `[REDACTED-AWS-ACCESS]`  | `AKIAIOSFODNN7EXAMPLE`        |
| GitHub classic PAT              | `[REDACTED-GH-TOKEN]`    | `ghp_Abc123...` (36 chars)    |
| GitHub fine-grained PAT         | `[REDACTED-GH-PAT]`      | `github_pat_...` (82 chars)   |
| Anthropic API key               | `[REDACTED-ANTHROPIC]`   | `sk-ant-api03-...`            |
| OpenAI API key                  | `[REDACTED-OPENAI]`      | `sk-...` (32+ chars)          |
| Google API key                  | `[REDACTED-GOOGLE]`      | `AIza...` (35 chars)          |
| Stripe live key                 | `[REDACTED-STRIPE-LIVE]` | `sk_live_...`                 |
| Stripe test key                 | `[REDACTED-STRIPE-TEST]` | `sk_test_...`                 |
| Twilio account SID              | `[REDACTED-TWILIO]`      | `AC` + 32 hex chars           |
| PEM private key                 | `[REDACTED-PRIVATE-KEY]` | `-----BEGIN PRIVATE KEY-----` |
| JWT token                       | `[REDACTED-JWT]`         | `eyJ...` three-part           |
| Database URL with credentials   | `[REDACTED-DB-URL]://`   | `postgres://user:pass@host`   |
| Generic `key=value` credentials | `[REDACTED-GENERIC]`     | `api_key: "abc123"`           |

**What the redactor does NOT log:** The redactor records only the number of redacted matches per pattern, never the matched content. This prevents sensitive values from appearing in application logs.

### Design notes

- The generic pattern uses a case-insensitive match against variable names that contain `api_key`, `secret`, `password`, `passwd`, `token`, `auth`, or `credential`, followed by `=` or `:` and a value of 8+ characters.
- After redaction the original `diff_summary` string is discarded and only the cleaned version is stored.

---

## Path Security

Index databases are stored under `~/.slg/indices/<repo_hash>/<branch>.db`. Branch names are user-controlled and could contain path traversal sequences. The `safe_index_path()` function in `slg-security` enforces:

1. **Character allowlist** — only ASCII alphanumerics, `-`, and `_` survive. `.` is replaced with `_` (prevents `.ssh`, `..`).
2. **Length cap** — branch names are truncated to 64 characters.
3. **Dangerous name rejection** — empty names and single-underscore names fall back to `unknown-branch`.
4. **Leading-dash rejection** — branch names starting with `-` are prefixed with `b_` to prevent them being interpreted as flags by shell tools.
5. **Prefix check** — after constructing the candidate path, slg verifies it is lexically under the `~/.slg/indices/<repo_hash>/` base. Any path that would escape returns a `PathTraversal` error.
6. **`..` component check** — every component of the path is inspected for `ParentDir`; any match immediately returns a `PathTraversal` error.

`safe_index_path()` is the **only** function in the codebase that is allowed to construct index paths. All other code calls it.

---

## Output Isolation (CDATA)

When MCP tool results are returned in XML format (the default), all commit content — `message`, `body`, `diff_summary` — is wrapped in `<![CDATA[...]]>` sections:

```xml
<message><![CDATA[fix: prevent SQL injection in user query handler]]></message>
```

This means a commit message that contains XML tags or what looks like an XML instruction cannot be interpreted as markup by the consuming agent. The output also always starts with a `<security_notice>` element:

```xml
<security_notice>Output may contain sanitized content.</security_notice>
```

---

## File System Permissions

All directories created by slg under `~/.slg/` are created with **`0o700` permissions** on Unix systems (owner read/write/execute only). This includes:

| Directory                      | Purpose                               |
| ------------------------------ | ------------------------------------- |
| `~/.slg/`                     | Root of all slg data                 |
| `~/.slg/indices/<repo_hash>/` | Per-repo, per-branch SQLite databases |

On Windows, explicit permission setting is a no-op (the OS ACLs on the user's home directory provide equivalent isolation).

---

## Security Logging

Any redaction event or path traversal attempt is written to `~/.slg/security.log` in addition to the application warning log. This file can be inspected to audit how many secrets were detected during indexing.

```
~/.slg/security.log
```

---

## MCP Read-Only Guarantee

All MCP tool definitions are explicitly annotated read-only. The MCP server:

- Does **not** implement any `resources/write`, `sampling`, or other write-capable MCP capabilities.
- Only invokes slg CLI sub-commands: `why`, `blame`, `log`, `bisect`, and `status`.
- None of those sub-commands modify the git repository or the index.

The `capabilities` object returned by `initialize` only lists `"tools": {}`, with no write methods.

---

## Injection Flagging

During indexing, slg checks every commit body and diff summary for patterns that resemble prompt-injection attempts (e.g., `Ignore previous instructions`, `[[SYSTEM]]`, or other LLM steering phrases). When a match is found:

- The `injection_flagged` boolean column in SQLite is set to `true`.
- The commit is still indexed (for auditability) but all MCP output includes the `injection_flagged` field so AI agents can treat it with appropriate suspicion.
- The flag is surfaced in `slg doctor` as a warning count.

Flagged commits do **not** have their content removed — they are visible in search results — but consumers are expected to treat them as untrusted.
