// Allow warnings, so we don't see so many warnings everytime we run tests or build
// We will clean up warnings once in a while
#![allow(warnings)] 
mod unity_project_manager;
mod unity_messaging_client;

use std::path::PathBuf;
use unity_project_manager::UnityProjectManager;
use unity_messaging_client::UnityMessagingClient;

#[tokio::main]
async fn main() {
    println!("Unity Code MCP Server");

    // Example usage with the embedded Unity project
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
    
    println!("Project path: {}", project_path.display());
    
    match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(mut manager) => {
            println!("Successfully initialized Unity project manager");
            
            if let Some(version) = manager.unity_version() {
                println!("Unity version: {}", version);
            }
            
            // Try to update process info
            match manager.update_process_info().await {
                Ok(()) => {
                    if let Some(pid) = manager.unity_process_id() {
                        println!("Unity process ID: {}", pid);
                        
                        // Test messaging client if Unity is running
                        println!("\nTesting Unity messaging client...");
                        match UnityMessagingClient::new(pid) {
                            Ok(client) => {
                                println!("Created messaging client, Unity address: {}", client.unity_address());
                                
                                // Test ping-pong
                                match client.ping() {
                                    Ok(()) => {
                                        println!("✓ Ping-pong test successful!");
                                        
                                        // Try to get version and project path
                                        if let Ok(version) = client.get_version() {
                                            println!("Unity package version: {}", version);
                                        }
                                        
                                        if let Ok(project_path) = client.get_project_path() {
                                            println!("Unity project path: {}", project_path);
                                        }
                                    },
                                    Err(e) => println!("✗ Ping-pong test failed: {}", e),
                                }
                            },
                            Err(e) => println!("Failed to create messaging client: {}", e),
                        }
                    }
                },
                Err(e) => println!("Unity process not found: {}", e),
            }
        },
        Err(e) => println!("Failed to initialize Unity project manager: {}", e),
    }
}
