// Allow warnings, so we don't see so many warnings everytime we run tests or build
// We will clean up warnings once in a while
#![allow(warnings)] 
mod unity_project_manager;
mod unity_messaging_client;
mod unity_manager;
mod unity_manager_tests;
mod test_utils;
mod logging;

use serde_json::json;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    schemars,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use unity_manager::{UnityManager, TestFilter, TestMode};
use crate::logging::init_logging;
use std::sync::Arc;
use tokio::sync::Mutex;
use log::{info, error};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RunTestsRequest {
    #[schemars(description = "Test mode to run: EditMode or PlayMode")]
    pub test_mode: String,
    #[schemars(description = "Optional filter: namespace (Namespace), class name (Namespace.Class), or specific method name (Namespace.Class.Method)")]
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
                        return Err(McpError::internal_error(format!("Failed to initialize messaging: {}", e), None));
                    }
                    *manager_guard = Some(manager);
                }
                Err(e) => {
                    return Err(McpError::internal_error(format!("Failed to create Unity manager: {}", e), None));
                }
            }
        }
        Ok(())
    }

    /// Refresh Unity asset database and return compilation errors
    #[tool(description = "Refresh Unity asset database, which also compiles scripts if necessary. Returns error logs including compilation errors if any.")]
    async fn refresh_asset_database(&self) -> Result<CallToolResult, McpError> {
        self.ensure_unity_manager().await?;
        
        let mut manager_guard = self.unity_manager.lock().await;
        let manager = manager_guard.as_mut().unwrap();
        
        match manager.refresh().await {
            Ok(result) => {
                let response = json!({
                    "refresh_completed": result.refresh_completed,
                    "refresh_error_message": result.refresh_error_message,
                    "compilation_started": result.compilation_started,
                    "compilation_completed": result.compilation_completed,
                    "error_logs": result.error_logs,
                    "duration_seconds": result.duration_seconds
                });
                Ok(CallToolResult::success(vec![Content::text(
                    response.to_string(),
                )]))
            }
            Err(e) => Err(McpError::internal_error(format!("Failed to refresh asset database: {}", e), None))
        }
    }

    /// Run Unity tests and return results
    #[tool(description = "Run Unity tests. Supports EditMode and PlayMode tests with optional filtering.")]
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
            _ => return Err(McpError::invalid_params("Invalid test mode. Use 'EditMode' or 'PlayMode'", None)),
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
                    "execution_completed": result.execution_completed,
                    "pass_count": result.pass_count,
                    "fail_count": result.fail_count,
                    "test_results": result.test_results.iter().map(|test| {
                        json!({
                            "full_name": test.full_name,
                            "passed": test.passed,
                            "duration": test.duration,
                            "message": test.message,
                            "stack_trace": test.stack_trace,
                            "output": test.output
                        })
                    }).collect::<Vec<_>>()
                });
                Ok(CallToolResult::success(vec![Content::text(
                    response.to_string(),
                )]))
            }
            Err(e) => Err(McpError::internal_error(format!("Failed to run tests: {}", e), None))
        }
    }
}

#[tool_handler]
impl ServerHandler for UnityCodeMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Unity Code MCP Server for Unity Editor integration".into()),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    init_logging();
    
    // Get Unity project path from environment variable or use default
    let project_path = std::env::var("UNITY_PROJECT_PATH")
        .unwrap_or_else(|_| ".".to_string());
    
    info!("Starting Unity Code MCP Server for project: {}", project_path);
    
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
