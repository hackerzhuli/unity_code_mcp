// Allow warnings, so we don't see so many warnings everytime we run tests or build
// We will clean up warnings once in a while
#![allow(warnings)] 
mod unity_project_manager;
mod unity_messaging_client;
mod unity_manager;
mod unity_manager_tests;
mod test_utils;
mod logging;

use std::path::PathBuf;
use unity_project_manager::UnityProjectManager;
use unity_messaging_client::UnityMessagingClient;
use crate::logging::init_logging;

#[tokio::main]
async fn main() {
    // Initialize logging
    init_logging();
    
    info_log!("Unity Code MCP Server starting");

    // Example usage with the embedded Unity project
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
    
    info_log!("Project path: {}", project_path.display());
    
    match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(mut manager) => {
            info_log!("Successfully initialized Unity project manager");
            
            if let Some(version) = manager.unity_version() {
                info_log!("Unity version: {}", version);
            }
            
            // Try to update process info
            match manager.update_process_info().await {
                Ok(()) => {
                    if let Some(pid) = manager.unity_process_id() {
                        info_log!("Unity process ID: {}", pid);
                        
                        // Test messaging client if Unity is running
                        info_log!("Testing Unity messaging client...");
                        match UnityMessagingClient::new(pid).await {
                            Ok(mut client) => {
                                info_log!("Created messaging client, Unity address: {}", client.unity_address());
                                
                                // Start listening for Unity messages to track connection status
                                if let Err(e) = client.start_listening().await {
                                    error_log!("Failed to start listening: {}", e);
                                    return;
                                }
                                
                                // Wait a moment for Unity to send initial messages
                                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                
                                // Test connection status
                                if client.is_connected(Some(5)) {
                                    info_log!("✓ Unity connection test successful!");
                                    
                                    // Check Unity online status
                                    info_log!("Unity online status: {}", client.is_online());
                                    
                                    // Request Unity version (response will come via events)
                                    if let Ok(_) = client.get_version().await {
                                        info_log!("Version request sent successfully");
                                    }
                                    
                                    // Request Unity project path (response will come via events)
                                    if let Ok(_) = client.get_project_path().await {
                                        info_log!("Project path request sent successfully");
                                    }
                                } else {
                                    error_log!("✗ Unity connection test failed: No recent responses from Unity");
                                    info_log!("Unity online status: {}", client.is_online());
                                }
                                
                                client.stop_listening();
                            },
                            Err(e) => error_log!("Failed to create messaging client: {}", e),
                        }
                    }
                },
                Err(e) => error_log!("Unity process not found: {}", e),
            }
        },
        Err(e) => error_log!("Failed to initialize Unity project manager: {}", e),
    }
}
