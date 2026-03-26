use slg_core::errors::SlgError;
use slg_mcp::server;

/// Start the MCP server on stdio (JSON-RPC 2.0).
pub async fn run() -> Result<(), SlgError> {
    server::run_mcp_server().await
}
