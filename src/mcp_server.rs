use crate::unity_messages::{TestFilter, TestMode};
use crate::unity_manager::UnityManager;
use log::info;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

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

    /// Get a reference to the unity manager for external access
    pub fn get_unity_manager(&self) -> &Arc<Mutex<Option<UnityManager>>> {
        &self.unity_manager
    }

    /// Initialize the Unity manager and messaging client
    pub async fn ensure_unity_manager(&self) -> Result<(), McpError> {
        let mut manager_guard = self.unity_manager.lock().await;
        if manager_guard.is_none() {
            match UnityManager::new(self.project_path.clone()).await {
                Ok(mut manager) => {
                    if let Err(e) = manager.update_unity_connection().await {
                        return Err(McpError::internal_error(
                            format!("Failed to upadte unity connection: {}", e),
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
                    "refresh_success": result.success,
                    "refresh_error_message": result.refresh_error_message,
                    "compilation_occurred": result.compiled,
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
        description = "Run Unity tests. Supports EditMode and PlayMode tests with optional filtering. Please use refresh_asset_database tool to compile scripts before running tests(if any script changed)."
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
                TestFilter::Custom {
                    mode,
                    filter: format!("{}?", filter_str), // add a question mark for fuzzy matching
                }
            }
        };

        match manager.run_tests(test_filter).await {
            Ok(result) => {
                let response = json!({
                    "test_run_completed": result.execution_completed,
                    "test_run_error_message": result.error_message,
                    "passed_test_count": result.pass_count,
                    "failed_test_count": result.fail_count,
                    "skipped_test_count": result.skip_count,
                    "total_test_count": result.test_count,
                    "duration_seconds": result.duration_seconds,
                    "first_50_passed_tests": result.test_results.iter().filter(|test| test.passed).take(50).map(|test| {
                        test.full_name.clone()
                    }).collect::<Vec<_>>(),
                    // AI agent only need to care about failed tests, so we filter out passed tests(there can be hundreds of tests, so filter make sense)
                    "failed_tests": result.test_results.iter().filter(|test| !test.passed).map(|test| {
                        let combined_error = if test.error_message.is_empty() && test.error_stack_trace.is_empty() {
                            String::new()
                        } else if test.error_message.is_empty() {
                            test.error_stack_trace.clone()
                        } else if test.error_stack_trace.is_empty() {
                            test.error_message.clone()
                        } else {
                            format!("{} {}", test.error_message, test.error_stack_trace)
                        };
                        
                        json!({
                            "full_name": test.full_name,
                            "duration_seconds": test.duration_seconds,
                            "error_message": combined_error,
                            "logs": test.output_logs.trim_end()
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