use lore_core::errors::LoreError;
use lore_mcp::server;

/// Start the MCP server on stdio (JSON-RPC 2.0).
pub async fn run() -> Result<(), LoreError> {
    server::run_mcp_server().await
}
