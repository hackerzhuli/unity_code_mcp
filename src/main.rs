// Allow warnings, so we don't see so many warnings everytime we run tests or build
// We will clean up warnings once in a while
#![allow(warnings)]
mod logging;
mod mcp_server;
mod test_utils;
mod unity_log_utils;
mod unity_manager;
mod unity_manager_tests;
mod unity_messaging_client;
mod unity_project_manager;
mod unity_messages;

use crate::logging::init_logging;
use crate::mcp_server::UnityCodeMcpServer;
use log::{error, info};
use rmcp::{
    ServiceExt,
    transport::stdio,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    init_logging();

    // Get Unity project path from environment variable or use default
    let project_path = std::env::var("UNITY_PROJECT_PATH").unwrap_or_else(|_| ".".to_string());

    info!(
        "Starting Unity Code MCP Server for project: {}",
        project_path
    );

    // Create the MCP server
    let server = UnityCodeMcpServer::new(project_path);

    // Initialize Unity manager once at startup
    if let Err(e) = server.ensure_unity_manager().await {
        log::warn!("Initial Unity manager initialization failed: {:?}", e);
    }

    // Start background Unity connection monitoring
    let server_clone = server.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let mut manager_guard = server_clone.get_unity_manager().lock().await;
            if let Some(manager) = manager_guard.as_mut() {
                if let Err(e) = manager.update_unity_connection().await {
                    log::debug!("Unity connection update failed: {:?}", e);
                }
            }
        }
    });

    // Start the MCP server
    let service = server.serve(stdio()).await.inspect_err(|e| {
        error!("serving error: {:?}", e);
    })?;

    info!("MCP Server started successfully");
    service.waiting().await?;
    info!("MCP Server stopped");

    Ok(())
}
