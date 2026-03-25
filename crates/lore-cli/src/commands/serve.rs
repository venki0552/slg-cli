use lore_core::errors::LoreError;
use lore_mcp::server;

pub async fn run() -> Result<(), LoreError> {
    server::run_mcp_server().await
}
