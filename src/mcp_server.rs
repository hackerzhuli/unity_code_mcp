use crate::unity_messages::{TestFilter, TestMode};
use crate::unity_manager::UnityManager;
use crate::unity_project_manager::UnityProjectManager;
use log::{info, warn};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    service::RequestContext,
};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RunTestsRequest {
    #[schemars(description = "Test mode to run: EditMode or PlayMode")]
    pub test_mode: String,
    #[schemars(
        description = "Optional filter: namespace, class, or method name. It is recommended to use the fully qualified name, e.g. `MyNamespace.MyClass` or `MyNamespace.MyClass.MyMethod` for more accurate test selection."
    )]
    pub filter: Option<String>,
}

/// Unity Code MCP Server that provides tools for Unity Editor integration
#[derive(Clone)]
pub struct UnityCodeMcpServer {
    unity_manager: Arc<TokioMutex<Option<UnityManager>>>,
    project_path: Arc<Mutex<Option<String>>>,
    tool_router: ToolRouter<UnityCodeMcpServer>,
}

#[tool_router]
impl UnityCodeMcpServer {
    /// Create a new Unity Code MCP Server
    pub fn new(fallback_project_path: Option<String>) -> Self {
        // Validate the fallback project path before storing it
        let validated_path = fallback_project_path.and_then(|path| {
            if UnityProjectManager::is_unity_project_path(&path) {
                Some(path)
            } else {
                warn!("Provided fallback project path is not a valid Unity project: {}", path);
                None
            }
        });
        
        Self {
            unity_manager: Arc::new(TokioMutex::new(None)),
            project_path: Arc::new(Mutex::new(validated_path)),
            tool_router: Self::tool_router(),
        }
    }

    /// Get a reference to the unity manager for external access
    pub fn get_unity_manager(&self) -> &Arc<TokioMutex<Option<UnityManager>>> {
        &self.unity_manager
    }

    /// Check if a Unity project path is available
    pub fn has_project_path(&self) -> bool {
        self.project_path.lock().unwrap().is_some()
    }

    /// Get the current project path, resolving it if necessary
    async fn get_project_path(&self) -> Result<String, McpError> {
        let project_path_guard = self.project_path.lock().unwrap();
        if let Some(path) = project_path_guard.as_ref() {
            Ok(path.clone())
        } else {
            Err(McpError::internal_error(
                "No Unity project path available. Please ensure roots are set or UNITY_PROJECT_PATH environment variable is configured.".to_string(),
                None,
            ))
        }
    }

    /// Set the project path (used when roots are detected)
    pub fn set_project_path(&self, path: String) {
        if UnityProjectManager::is_unity_project_path(&path) {
            let mut project_path_guard = self.project_path.lock().unwrap();
            *project_path_guard = Some(path);
        } else {
            warn!("Attempted to set invalid Unity project path: {}", path);
        }
    }

    /// Try to detect Unity project from client roots during initialization
    async fn try_detect_from_roots_if_needed(&self, context: &RequestContext<RoleServer>) -> Result<(), McpError> {
        // Only try to detect if we don't have a project path yet
        {
            let project_path_guard = self.project_path.lock().unwrap();
            if project_path_guard.is_some() {
                return Ok(());
            }
        } // Guard is dropped here before await
        
        // Try to request roots from client
        match context.peer.list_roots().await {
            Ok(roots_result) => {
                info!("Received {} roots from client", roots_result.roots.len());
                
                // Find the first root that is a Unity project
                for root in &roots_result.roots {
                    if let Some(path) = root.uri.strip_prefix("file://") {
                        let normalized_path = path.replace("/", "\\");
                        if UnityProjectManager::is_unity_project_path(&normalized_path) {
                            info!("Found Unity project at: {}", normalized_path);
                            self.set_project_path(normalized_path);
                            return Ok(());
                        }
                    }
                }
                
                warn!("No Unity project found in the provided roots");
            }
            Err(e) => {
                info!("Client does not support roots capability or error occurred: {}", e);
            }
        }
        
        Ok(())
    }

    /// Initialize the Unity manager and messaging client
    pub async fn ensure_unity_manager(&self) -> Result<(), McpError> {
        let mut manager_guard = self.unity_manager.lock().await;
        if manager_guard.is_none() {
            let project_path = self.get_project_path().await?;
            match UnityManager::new(project_path).await {
                Ok(mut manager) => {
                    if let Err(e) = manager.update_unity_connection().await {
                        return Err(McpError::internal_error(
                            format!("Failed to update unity connection: {}", e),
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
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Unity Code MCP Server for Unity Editor integration".into()),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        info!("Client connected, server initialized successfully");
        
        // Spawn background task to eagerly detect Unity project from roots
        // This runs in parallel without blocking initialization
        let server_clone = self.clone();
        let context_clone = context.clone();
        tokio::spawn(async move {
            if let Err(e) = server_clone.try_detect_from_roots_if_needed(&context_clone).await {
                warn!("Failed to detect Unity project from roots: {}", e);
            }
            
            // Try to initialize Unity manager after detection
            if let Err(e) = server_clone.ensure_unity_manager().await {
                warn!("Failed to initialize Unity manager after roots detection: {}", e);
            } else {
                info!("Unity manager initialized successfully after roots detection");
            }
        });

        Ok(InitializeResult {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Unity Code MCP Server for Unity Editor integration".into()),
        })
    }
}