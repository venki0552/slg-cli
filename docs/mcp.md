# MCP Integration Guide

`slg serve` (alias: `slg mcp`) exposes the slg index as a **Model Context Protocol (MCP)** server over `stdio`, letting AI coding agents — Claude Code, Cursor, Windsurf, and GitHub Copilot — query your git history in real time.

---

## Contents

- [Protocol Overview](#protocol-overview)
- [Starting the Server](#starting-the-server)
- [Server Limits](#server-limits)
- [Tools Reference](#tools-reference)
  - [slg_why](#slg_why)
  - [slg_blame](#slg_blame)
  - [slg_log](#slg_log)
  - [slg_bisect](#slg_bisect)
  - [slg_status](#slg_status)
- [JSON-RPC Methods](#json-rpc-methods)
- [Output Format](#output-format)
- [Error Handling](#error-handling)
- [Auto-Registration via VS Code Extension](#auto-registration-via-vs-code-extension)
- [Manual Agent Configuration](#manual-agent-configuration)

---

## Protocol Overview

| Property       | Value                                     |
| -------------- | ----------------------------------------- |
| Transport      | `stdio` (stdin → stdout, stderr for logs) |
| Protocol       | JSON-RPC 2.0                              |
| MCP Version    | `2024-11-05`                              |
| Server Name    | `slg`                                     |
| Server Version | `0.1.0`                                   |
| Capability     | `tools` (read-only, no writes)            |

---

## Starting the Server

```bash
# From inside a git repository
slg serve

# Alias
slg mcp
```

The server reads from `stdin` and writes to `stdout`, one JSON-RPC message per line. Log output goes to `stderr`.

---

## Server Limits

| Limit                | Value                                                    |
| -------------------- | -------------------------------------------------------- |
| Rate limit           | 60 requests / minute (token bucket, refills 1 token/sec) |
| Response size        | 50 KB max (`mcp_output_max_bytes`, configurable)         |
| Tool call timeout    | 15 seconds (`mcp_timeout_secs`, configurable)            |
| Max results per tool | 10                                                       |
| Max query length     | 500 characters                                           |

When the rate limit is exceeded the server returns HTTP-style error `429`:

```json
{
	"jsonrpc": "2.0",
	"id": 1,
	"error": {
		"code": -32000,
		"message": "Rate limit exceeded. Try again in 1s."
	}
}
```

When the response is too large, only the first 50 KB are returned with a truncation note appended.

---

## Tools Reference

### slg_why

Search git history semantically. Returns commits that explain **why** a decision was made.

**Input schema:**

| Parameter    | Type                | Required | Default | Description                                                 |
| ------------ | ------------------- | -------- | ------- | ----------------------------------------------------------- |
| `query`      | string              | ✓        | —       | Semantic search query (max 500 chars)                       |
| `limit`      | number              |          | 3       | Number of results (max 10)                                  |
| `since`      | string              |          | —       | Filter commits after this ISO 8601 date (e.g. `2024-01-01`) |
| `author`     | string              |          | —       | Filter by author name or email substring                    |
| `format`     | `"xml"` \| `"json"` |          | `"xml"` | Output format                                               |
| `max_tokens` | number              |          | 4096    | Maximum response tokens                                     |

**Example call:**

```json
{
	"jsonrpc": "2.0",
	"id": 1,
	"method": "tools/call",
	"params": {
		"name": "slg_why",
		"arguments": {
			"query": "why did we switch to async runtime",
			"limit": 5,
			"since": "2024-01-01"
		}
	}
}
```

**Example response (XML format):**

```xml
<slg_results query="why did we switch to async runtime" count="1" latency_ms="42">
  <security_notice>Output may contain sanitized content.</security_notice>
  <commit rank="1" relevance="0.94">
    <hash>a1b2c3d4</hash>
    <author>Alice</author>
    <date>2024-03-15</date>
    <intent>refactor</intent>
    <risk>low</risk>
    <message><![CDATA[refactor: migrate to tokio async runtime]]></message>
    <diff_summary><![CDATA[Replaced blocking IO in 8 files with async equivalents]]></diff_summary>
    <files>src/server.rs, src/handler.rs</files>
  </commit>
</slg_results>
```

---

### slg_blame

Find semantic ownership of a file or function — shows which authors made the most meaningful commits touching that code.

**Input schema:**

| Parameter | Type    | Required | Default | Description                                  |
| --------- | ------- | -------- | ------- | -------------------------------------------- |
| `file`    | string  | ✓        | —       | File path to analyze (relative to repo root) |
| `fn`      | string  |          | —       | Function name to focus on                    |
| `risk`    | boolean |          | false   | Include risk score per author                |

**Example call:**

```json
{
	"jsonrpc": "2.0",
	"id": 2,
	"method": "tools/call",
	"params": {
		"name": "slg_blame",
		"arguments": {
			"file": "src/auth.rs",
			"fn": "verify_token",
			"risk": true
		}
	}
}
```

---

### slg_log

Search git history and (optionally) group the results by **commit intent** (feat, fix, refactor, docs, etc.).

**Input schema:**

| Parameter   | Type    | Required | Default | Description                             |
| ----------- | ------- | -------- | ------- | --------------------------------------- |
| `query`     | string  | ✓        | —       | Search query                            |
| `since`     | string  |          | —       | Filter commits after this ISO 8601 date |
| `by_intent` | boolean |          | false   | Group results by intent classification  |

**Example call:**

```json
{
	"jsonrpc": "2.0",
	"id": 3,
	"method": "tools/call",
	"params": {
		"name": "slg_log",
		"arguments": {
			"query": "authentication changes",
			"by_intent": true
		}
	}
}
```

---

### slg_bisect

Find which commit likely introduced a bug. Performs semantic search against the bug description to rank candidate commits.

**Input schema:**

| Parameter         | Type   | Required | Default | Description                             |
| ----------------- | ------ | -------- | ------- | --------------------------------------- |
| `bug_description` | string | ✓        | —       | Natural-language description of the bug |
| `limit`           | number |          | 5       | Max candidates to return                |

**Example call:**

```json
{
	"jsonrpc": "2.0",
	"id": 4,
	"method": "tools/call",
	"params": {
		"name": "slg_bisect",
		"arguments": {
			"bug_description": "login fails with OAuth providers after the JWT refactor",
			"limit": 3
		}
	}
}
```

---

### slg_status

Get current slg index status — whether it is indexed, up to date, and how many commits it covers.

**Input schema:** No parameters.

**Example call:**

```json
{
	"jsonrpc": "2.0",
	"id": 5,
	"method": "tools/call",
	"params": {
		"name": "slg_status",
		"arguments": {}
	}
}
```

**Example response:**

```json
{
	"indexed": true,
	"commit_count": 1247,
	"last_indexed_at": "2024-06-10T14:32:00Z",
	"index_age_hours": 0.4
}
```

---

## JSON-RPC Methods

The server implements these standard MCP methods:

| Method                      | Description                                            |
| --------------------------- | ------------------------------------------------------ |
| `initialize`                | Handshake — returns `serverInfo` and `capabilities`    |
| `tools/list`                | Returns the 5 tool definitions with full input schemas |
| `tools/call`                | Invoke a tool by name                                  |
| `notifications/initialized` | Acknowledged but produces no response (no-op)          |

### `initialize` example

```json
{
	"jsonrpc": "2.0",
	"id": 0,
	"method": "initialize",
	"params": {
		"protocolVersion": "2024-11-05",
		"clientInfo": { "name": "claude-code", "version": "1.0" }
	}
}
```

Response:

```json
{
	"jsonrpc": "2.0",
	"id": 0,
	"result": {
		"protocolVersion": "2024-11-05",
		"serverInfo": { "name": "slg", "version": "0.1.0" },
		"capabilities": { "tools": {} }
	}
}
```

---

## Output Format

All tool results default to **XML** output with commit data wrapped in `<![CDATA[...]]>` sections to prevent injection and preserve formatting. XML output always includes a `<security_notice>` element.

Pass `"format": "json"` in `slg_why` to receive structured JSON instead.

---

## Error Handling

When the index is not yet built (e.g., `slg index` is still running), every tool call returns an initializing response instead of an error:

```json
{
	"jsonrpc": "2.0",
	"id": 1,
	"result": {
		"content": [
			{
				"type": "text",
				"text": "{\"status\":\"initializing\",\"message\":\"slg index is being built. Please wait.\",\"eta_seconds\":15}"
			}
		]
	}
}
```

Standard JSON-RPC error codes:

| Code     | Meaning                                           |
| -------- | ------------------------------------------------- |
| `-32700` | Parse error (malformed JSON)                      |
| `-32600` | Invalid request                                   |
| `-32601` | Method not found                                  |
| `-32602` | Invalid params                                    |
| `-32000` | Server error (rate limit, timeout, index missing) |

---

## Auto-Registration via VS Code Extension

When the VS Code extension is installed and a workspace is open, it automatically registers the slg MCP server with every supported AI agent config file it finds on disk:

| Agent       | Config file                            |
| ----------- | -------------------------------------- |
| Claude Code | `~/.claude/claude_desktop_config.json` |
| Cursor      | `~/.cursor/mcp.json`                   |
| Windsurf    | `~/.windsurf/mcp.json`                 |

The extension writes the entry only if the config directory already exists (i.e., the agent is installed). The slg binary path is resolved per platform.

Disable this behaviour in VS Code settings:

```json
{ "slg.autoRegisterMCP": false }
```

---

## Manual Agent Configuration

### Claude Code (`~/.claude/claude_desktop_config.json`)

```json
{
	"mcpServers": {
		"slg": {
			"command": "slg",
			"args": ["serve"],
			"env": {}
		}
	}
}
```

### Cursor (`~/.cursor/mcp.json`)

```json
{
	"mcpServers": {
		"slg": {
			"command": "slg",
			"args": ["serve"]
		}
	}
}
```

### Windsurf (`~/.windsurf/mcp.json`)

```json
{
	"mcpServers": {
		"slg": {
			"command": "slg",
			"args": ["serve"]
		}
	}
}
```

### GitHub Copilot (`.vscode/mcp.json`)

```json
{
	"servers": {
		"slg": {
			"type": "stdio",
			"command": "slg",
			"args": ["serve"]
		}
	}
}
```

> **Note:** Ensure `slg` is on your `PATH`, or replace `"slg"` with the absolute path to the binary (e.g., `~/.slg/bin/slg`).
