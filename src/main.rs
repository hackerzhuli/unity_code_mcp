// Allow warnings, so we don't see so many warnings everytime we run tests or build
// We will clean up warnings once in a while
#![allow(warnings)]
mod logging;
mod mcp_server;
mod test_utils;
mod unity_log_manager;
mod unity_log_utils;
mod unity_manager;
mod unity_manager_tests;
mod unity_messaging_client;
mod unity_project_manager;
mod unity_messages;
mod unity_refresh_task;
mod unity_test_task;

use crate::logging::init_logging;
use crate::mcp_server::UnityCodeMcpServer;
use crate::unity_project_manager::UnityProjectManager;
use log::{error, info, warn};
use rmcp::{
    ServiceExt,
    transport::stdio,
};
use std::env;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    init_logging();

    info!("Starting Unity Code MCP Server...");

    // Log all environment variables
    // info!("Environment Variables:");
    // for (key, value) in env::vars() {
    //     info!("  {}: {}", key, value);
    // }

    // Try to get project path from environment variable as fallback
    let fallback_project_path = env::var("UNITY_PROJECT_PATH").ok();
    
    // Validate fallback path if provided
     let validated_fallback = if let Some(ref path) = fallback_project_path {
         if UnityProjectManager::is_unity_project_path(path) {
             info!("Found valid Unity project at UNITY_PROJECT_PATH: {}", path);
             Some(path.clone())
         } else {
             warn!("UNITY_PROJECT_PATH does not point to a valid Unity project: {}", path);
             None
         }
     } else {
         info!("No UNITY_PROJECT_PATH environment variable set");
         None
     };

    // Create the MCP server with optional fallback path
    // The server will prioritize roots capability over this fallback
    let server = UnityCodeMcpServer::new(validated_fallback);

    // Start background task to monitor Unity connection
    // Initial connection is handled eagerly during client initialization
    let server_clone = server.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            // Only try to ensure Unity manager if we have a project path
            // This avoids spamming error logs when no Unity project is configured
            if server_clone.has_project_path() {
                if let Err(e) = server_clone.ensure_unity_manager().await {
                    error!("Failed to ensure Unity manager: {}", e);
                }
            }
        }
    });

    // Start the MCP server
    let service = server.serve(stdio()).await.inspect_err(|e| {
        error!("serving error: {:?}", e);
    })?;

    info!("Unity Code MCP Server ready. Waiting for client connection...");
    info!("The server will use roots capability for dynamic project detection if supported by the client.");
    service.waiting().await?;
    info!("MCP Server stopped");

    Ok(())
}
