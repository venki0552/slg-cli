use lore_core::errors::LoreError;
use serde_json::{json, Value};
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

    match tool_name {
        "lore_why" => {
            let query = arguments
                .get("query")
                .and_then(|q| q.as_str())
                .unwrap_or("");
            if query.is_empty() {
                return Ok(json!({
                    "content": [{"type": "text", "text": "Error: query is required"}],
                    "isError": true
                }));
            }

            // Placeholder: actual search requires initialized index + embedder
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("lore_why: query received: '{}'. Index not yet loaded — run `lore init` first.", query)
                }]
            }))
        }
        "lore_blame" => {
            let file = arguments
                .get("file")
                .and_then(|f| f.as_str())
                .unwrap_or("");
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("lore_blame: file '{}' — index not yet loaded.", file)
                }]
            }))
        }
        "lore_log" => {
            let query = arguments
                .get("query")
                .and_then(|q| q.as_str())
                .unwrap_or("");
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("lore_log: query '{}' — index not yet loaded.", query)
                }]
            }))
        }
        "lore_bisect" => {
            let desc = arguments
                .get("bug_description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("lore_bisect: '{}' — index not yet loaded.", desc)
                }]
            }))
        }
        "lore_status" => Ok(json!({
            "content": [{
                "type": "text",
                "text": "lore status: not initialized. Run `lore init` to start."
            }]
        })),
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
