use lore_core::errors::LoreError;
use lore_core::types::OutputFormat;
use lore_git::detector;
use lore_index::embedder::Embedder;
use lore_index::search::{self, SearchOptions};
use lore_index::store::IndexStore;
use lore_output::{json as json_fmt, xml};
use lore_security::output_guard::OutputGuard;
use lore_security::paths;
use serde_json::{json, Value};
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info};

use crate::rate_limiter::RateLimiter;
use crate::tools;

const MAX_OUTPUT_BYTES: usize = 50_000;
const TIMEOUT_SECS: u64 = 5;

/// Run the MCP server over stdio (JSON-RPC 2.0, line-delimited).
pub async fn run_mcp_server() -> Result<(), LoreError> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut limiter = RateLimiter::new(60);

    info!("lore MCP server started");

    let mut line = String::new();
    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .map_err(|e| LoreError::Io(e))?;

        if bytes_read == 0 {
            debug!("stdin closed, shutting down");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = json_rpc_error(None, -32700, &format!("Parse error: {}", e));
                write_response(&mut stdout, &err_resp).await?;
                continue;
            }
        };

        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("");

        debug!("Received method: {}", method);

        // Rate limit check
        if let Err(_) = limiter.check() {
            let resp = json_rpc_error(id, -32000, "Rate limit exceeded (60 req/min)");
            write_response(&mut stdout, &resp).await?;
            continue;
        }

        // Handle with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(TIMEOUT_SECS),
            handle_method(method, &request),
        )
        .await;

        let response = match result {
            Ok(Ok(result_value)) => json_rpc_result(id.clone(), result_value),
            Ok(Err(e)) => json_rpc_error(id.clone(), -32603, &e.to_string()),
            Err(_) => json_rpc_error(id.clone(), -32000, "Request timed out"),
        };

        // Cap output size
        let response_str = serde_json::to_string(&response).unwrap_or_default();
        let capped = if response_str.len() > MAX_OUTPUT_BYTES {
            let err = json_rpc_error(id.clone(), -32000, "Response too large");
            serde_json::to_string(&err).unwrap_or_default()
        } else {
            response_str
        };

        stdout
            .write_all(capped.as_bytes())
            .await
            .map_err(|e| LoreError::Io(e))?;
        stdout
            .write_all(b"\n")
            .await
            .map_err(|e| LoreError::Io(e))?;
        stdout.flush().await.map_err(|e| LoreError::Io(e))?;
    }

    Ok(())
}

async fn handle_method(method: &str, request: &Value) -> Result<Value, LoreError> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "lore",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(tools::tools_list_response()),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(json!({}));
            let tool_name = params
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("");

            handle_tool_call(tool_name, &params).await
        }
        "notifications/initialized" => Ok(json!({})),
        _ => Err(LoreError::Config(format!(
            "Unknown method: {}",
            method
        ))),
    }
}

async fn handle_tool_call(tool_name: &str, params: &Value) -> Result<Value, LoreError> {
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    // Try to open index for current repo
    let cwd = std::env::current_dir().map_err(LoreError::Io)?;
    let git_root = match detector::find_git_root(&cwd) {
        Ok(root) => root,
        Err(_) => {
            return Ok(json!({
                "content": [{"type": "text", "text": "Error: not in a git repository. Run `lore init` first."}],
                "isError": true
            }));
        }
    };

    let repo_hash = detector::compute_repo_hash(&git_root);
    let branch = detector::get_current_branch(&git_root).unwrap_or_else(|_| "main".to_string());
    let index_path = paths::safe_index_path(&repo_hash, &branch)?;

    if !index_path.exists() {
        // Auto-init hint
        if crate::auto_init::is_indexing() {
            return Ok(crate::auto_init::initializing_response(tool_name));
        }
        return Ok(json!({
            "content": [{"type": "text", "text": "Index not found. Run `lore init` first."}],
            "isError": true
        }));
    }

    let store = IndexStore::open(&index_path)?;

    match tool_name {
        "lore_why" => {
            let query = arguments.get("query").and_then(|q| q.as_str()).unwrap_or("");
            if query.is_empty() {
                return Ok(json!({"content": [{"type": "text", "text": "Error: query is required"}], "isError": true}));
            }
            if query.len() > 500 {
                return Ok(json!({"content": [{"type": "text", "text": "Error: query too long (max 500 chars)"}], "isError": true}));
            }

            let limit = arguments.get("limit").and_then(|l| l.as_u64()).unwrap_or(3) as u32;
            let max_tokens = arguments.get("max_tokens").and_then(|t| t.as_u64()).unwrap_or(4096) as usize;
            let fmt = match arguments.get("format").and_then(|f| f.as_str()) {
                Some("json") => OutputFormat::Json,
                _ => OutputFormat::Xml,
            };

            let embedder = Embedder::new()?;
            let options = SearchOptions {
                limit: limit.min(10),
                since: None,
                until: None,
                author: arguments.get("author").and_then(|a| a.as_str()).map(|s| s.to_string()),
                module: None,
                max_tokens,
                enable_reranker: false,
                format: fmt,
            };

            let start = Instant::now();
            let results = search::search(query, &store, &embedder, &options).await?;
            let latency_ms = start.elapsed().as_millis() as u64;

            let output = match fmt {
                OutputFormat::Json => json_fmt::format_json(&results, query, latency_ms),
                _ => xml::format_xml(&results, query, latency_ms),
            };

            let guard = OutputGuard::new();
            let safe = guard.check_and_sanitize(&output, MAX_OUTPUT_BYTES);

            Ok(json!({"content": [{"type": "text", "text": safe}]}))
        }
        "lore_blame" => {
            let file = arguments.get("file").and_then(|f| f.as_str()).unwrap_or("");
            if file.is_empty() {
                return Ok(json!({"content": [{"type": "text", "text": "Error: file is required"}], "isError": true}));
            }

            let query = format!("changes to file {}", file);
            let embedder = Embedder::new()?;
            let options = SearchOptions {
                limit: 10,
                since: None,
                until: None,
                author: None,
                module: Some(file.to_string()),
                max_tokens: 4096,
                enable_reranker: false,
                format: OutputFormat::Xml,
            };

            let start = Instant::now();
            let results = search::search(&query, &store, &embedder, &options).await?;
            let latency_ms = start.elapsed().as_millis() as u64;
            let output = xml::format_xml(&results, &query, latency_ms);

            let guard = OutputGuard::new();
            let safe = guard.check_and_sanitize(&output, MAX_OUTPUT_BYTES);

            Ok(json!({"content": [{"type": "text", "text": safe}]}))
        }
        "lore_log" => {
            let query = arguments.get("query").and_then(|q| q.as_str()).unwrap_or("");
            if query.is_empty() {
                return Ok(json!({"content": [{"type": "text", "text": "Error: query is required"}], "isError": true}));
            }

            let embedder = Embedder::new()?;
            let options = SearchOptions {
                limit: 10,
                since: None,
                until: None,
                author: None,
                module: None,
                max_tokens: 8192,
                enable_reranker: false,
                format: OutputFormat::Xml,
            };

            let start = Instant::now();
            let results = search::search(query, &store, &embedder, &options).await?;
            let latency_ms = start.elapsed().as_millis() as u64;
            let output = xml::format_xml(&results, query, latency_ms);

            let guard = OutputGuard::new();
            let safe = guard.check_and_sanitize(&output, MAX_OUTPUT_BYTES);

            Ok(json!({"content": [{"type": "text", "text": safe}]}))
        }
        "lore_bisect" => {
            let desc = arguments.get("bug_description").and_then(|d| d.as_str()).unwrap_or("");
            if desc.is_empty() {
                return Ok(json!({"content": [{"type": "text", "text": "Error: bug_description is required"}], "isError": true}));
            }

            let query = format!("bug: {}", desc);
            let limit = arguments.get("limit").and_then(|l| l.as_u64()).unwrap_or(5) as u32;

            let embedder = Embedder::new()?;
            let options = SearchOptions {
                limit: limit.min(10),
                since: None,
                until: None,
                author: None,
                module: None,
                max_tokens: 4096,
                enable_reranker: false,
                format: OutputFormat::Xml,
            };

            let start = Instant::now();
            let results = search::search(&query, &store, &embedder, &options).await?;
            let latency_ms = start.elapsed().as_millis() as u64;
            let output = xml::format_xml(&results, &query, latency_ms);

            let guard = OutputGuard::new();
            let safe = guard.check_and_sanitize(&output, MAX_OUTPUT_BYTES);

            Ok(json!({"content": [{"type": "text", "text": safe}]}))
        }
        "lore_status" => {
            let all_hashes = store.list_all_hashes()?;
            let base_branch = detector::detect_base_branch(&git_root);
            let meta = store.get_metadata(&repo_hash, &branch, &base_branch)?;

            let status = json!({
                "indexed": true,
                "branch": branch,
                "repo_hash": &repo_hash[..8],
                "commit_count": all_hashes.len(),
                "size_bytes": meta.size_bytes,
                "model_version": meta.model_version,
                "index_version": meta.index_version,
            });

            Ok(json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&status).unwrap_or_default()}]}))
        }
        _ => Ok(json!({
            "content": [{"type": "text", "text": format!("Unknown tool: {}", tool_name)}],
            "isError": true
        })),
    }
}

fn json_rpc_result(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn json_rpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

async fn write_response(
    stdout: &mut tokio::io::Stdout,
    response: &Value,
) -> Result<(), LoreError> {
    let s = serde_json::to_string(response).unwrap_or_default();
    stdout
        .write_all(s.as_bytes())
        .await
        .map_err(|e| LoreError::Io(e))?;
    stdout
        .write_all(b"\n")
        .await
        .map_err(|e| LoreError::Io(e))?;
    stdout.flush().await.map_err(|e| LoreError::Io(e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_result() {
        let resp = json_rpc_result(Some(json!(1)), json!({"ok": true}));
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert!(resp["result"]["ok"].as_bool().unwrap());
    }

    #[test]
    fn test_json_rpc_error() {
        let resp = json_rpc_error(Some(json!(2)), -32600, "Invalid request");
        assert_eq!(resp["error"]["code"], -32600);
        assert_eq!(resp["error"]["message"], "Invalid request");
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let req = json!({});
        let result = handle_method("initialize", &req).await.unwrap();
        assert_eq!(result["serverInfo"]["name"], "lore");
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let req = json!({});
        let result = handle_method("tools/list", &req).await.unwrap();
        assert!(result["tools"].is_array());
    }

    #[tokio::test]
    async fn test_handle_unknown_tool() {
        let req = json!({
            "params": {
                "name": "nonexistent",
                "arguments": {}
            }
        });
        let result = handle_method("tools/call", &req).await.unwrap();
        assert!(result["isError"].as_bool().unwrap_or(false));
    }
}
