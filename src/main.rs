// Allow warnings, so we don't see so many warnings everytime we run tests or build
// We will clean up warnings once in a while
#![allow(warnings)]
mod logging;
mod test_utils;
mod unity_log_utils;
mod unity_manager;
mod unity_manager_tests;
mod unity_messaging_client;
mod unity_project_manager;

use crate::logging::init_logging;
use log::{error, info};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use unity_manager::{TestFilter, TestMode, UnityManager};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RunTestsRequest {
    #[schemars(description = "Test mode to run: EditMode or PlayMode")]
    pub test_mode: String,
    #[schemars(
        description = "Optional filter: namespace (Namespace), class name (Namespace.Class), or specific method name (Namespace.Class.Method), all names must be fully qualified."
    )]
    pub filter: Option<String>,
}

/// Unity Code MCP Server that provides tools for Unity Editor integration
#[derive(Clone)]
pub struct UnityCodeMcpServer {
    unity_manager: Arc<Mutex<Option<UnityManager>>>,
    project_path: String,
    tool_router: ToolRouter<UnityCodeMcpServer>,
}

#[tool_router]
impl UnityCodeMcpServer {
    /// Create a new Unity Code MCP Server
    pub fn new(project_path: String) -> Self {
        Self {
            unity_manager: Arc::new(Mutex::new(None)),
            project_path,
            tool_router: Self::tool_router(),
        }
    }

    /// Initialize the Unity manager and messaging client
    async fn ensure_unity_manager(&self) -> Result<(), McpError> {
        let mut manager_guard = self.unity_manager.lock().await;
        if manager_guard.is_none() {
            match UnityManager::new(self.project_path.clone()).await {
                Ok(mut manager) => {
                    if let Err(e) = manager.initialize_messaging().await {
                        return Err(McpError::internal_error(
                            format!("Failed to initialize messaging: {}", e),
                            None,
                        ));
                    }
                    *manager_guard = Some(manager);
                }
                Err(e) => {
                    return Err(McpError::internal_error(
                        format!("Failed to create Unity manager: {}", e),
                        None,
                    ));
                }
            }
        }
        
        // Update Unity connection to ensure we're connected to the current Unity instance
        if let Some(manager) = manager_guard.as_mut() {
            let _ = manager.update_unity_connection().await;
        }
        
        Ok(())
    }

    /// Refresh Unity asset database and return compilation errors
    #[tool(
        description = "Refresh Unity asset database, which compiles C# scripts if scripts changed. Returns detected problems (e.g. C# compile errors) if any."
    )]
    async fn refresh_asset_database(&self) -> Result<CallToolResult, McpError> {
        self.ensure_unity_manager().await?;

        let mut manager_guard = self.unity_manager.lock().await;
        let manager = manager_guard.as_mut().unwrap();

        match manager.refresh_asset_database().await {
            Ok(result) => {
                let response = json!({
                    "refresh_completed": result.refresh_completed,
                    "refresh_error_message": result.refresh_error_message,
                    "compilation_started": result.compilation_started,
                    "compilation_completed": result.compilation_completed,
                    "problems": result.problems,
                    "duration_seconds": result.duration_seconds
                });
                Ok(CallToolResult::success(vec![Content::text(
                    response.to_string(),
                )]))
            }
            Err(e) => Err(McpError::internal_error(
                format!("Failed to refresh asset database: {}", e),
                None,
            )),
        }
    }

    /// Run Unity tests and return results
    #[tool(
        description = "Run Unity tests. Supports EditMode and PlayMode tests with optional filtering."
    )]
    async fn run_tests(
        &self,
        Parameters(RunTestsRequest { test_mode, filter }): Parameters<RunTestsRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_unity_manager().await?;

        let mut manager_guard = self.unity_manager.lock().await;
        let manager = manager_guard.as_mut().unwrap();

        // Parse test mode
        let mode = match test_mode.as_str() {
            "EditMode" => TestMode::EditMode,
            "PlayMode" => TestMode::PlayMode,
            _ => {
                return Err(McpError::invalid_params(
                    "Invalid test mode. Use 'EditMode' or 'PlayMode'",
                    None,
                ));
            }
        };

        // Parse test filter
        let test_filter = match filter {
            None => TestFilter::All(mode),
            Some(filter_str) => {
                if filter_str.ends_with(".dll") {
                    // Assembly filter
                    TestFilter::Assembly {
                        mode,
                        assembly_name: filter_str,
                    }
                } else if filter_str.contains(".") {
                    // Specific test filter (contains dots, likely a namespace.class.method)
                    TestFilter::Specific {
                        mode,
                        test_name: filter_str,
                    }
                } else {
                    // Custom filter
                    TestFilter::Custom {
                        mode,
                        filter: filter_str,
                    }
                }
            }
        };

        match manager.run_tests(test_filter).await {
            Ok(result) => {
                let response = json!({
                    "test_run_completed": result.execution_completed,
                    "test_run_error_message": result.error_message,
                    "pass_count": result.pass_count,
                    "fail_count": result.fail_count,
                    "skip_count": result.skip_count,
                    "test_count": result.test_count,
                    "duration_seconds": result.duration_seconds,
                    "test_results": result.test_results.iter().map(|test| {
                        json!({
                            "full_name": test.full_name,
                            "passed": test.passed,
                            "duration_seconds": test.duration_seconds,
                            "error_message": test.error_message,
                            "error_stack_trace": test.error_stack_trace,
                            "output_logs": test.output_logs
                        })
                    }).collect::<Vec<_>>()
                });
                Ok(CallToolResult::success(vec![Content::text(
                    response.to_string(),
                )]))
            }
            Err(e) => Err(McpError::internal_error(
                format!("Failed to run tests: {}", e),
                None,
            )),
        }
    }
}

#[tool_handler]
impl ServerHandler for UnityCodeMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Unity Code MCP Server for Unity Editor integration".into()),
        }
    }
}

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

    // Create and start the MCP server
    let server = UnityCodeMcpServer::new(project_path);

    // Start the MCP server
    let service = server.serve(stdio()).await.inspect_err(|e| {
        error!("serving error: {:?}", e);
    })?;

    info!("MCP Server started successfully");
    service.waiting().await?;
    info!("MCP Server stopped");

    Ok(())
}
