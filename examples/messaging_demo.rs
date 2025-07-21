//! Unity Messaging Client Demo
//! 
//! This example demonstrates how to use the Unity messaging client to:
//! - Test connectivity with ping-pong
//! - Retrieve Unity information (version, project path)
//! - Listen for Unity log messages

use std::path::PathBuf;
use std::time::Duration;
use tokio::time::timeout;

use unity_code_mcp::unity_project_manager::UnityProjectManager;
use unity_code_mcp::unity_messaging_client::{UnityMessagingClient, MessageType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Unity Messaging Client Demo");
    println!("==============================\n");

    // Initialize Unity project manager
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
    println!("Project path: {}", project_path.display());
    
    let mut manager = UnityProjectManager::new(project_path.to_string_lossy().to_string()).await?;
    println!("✓ Unity project manager initialized");
    
    if let Some(version) = manager.unity_version() {
        println!("Unity version: {}", version);
    }
    
    // Try to get Unity process
    match manager.update_process_info().await {
        Ok(()) => {
            if let Some(pid) = manager.unity_process_id() {
                println!("Unity process ID: {}\n", pid);
                
                // Create messaging client
                match UnityMessagingClient::new(pid) {
                    Ok(client) => {
                        println!("✓ Created messaging client for Unity at {}", client.unity_address());
                        
                        // Test 1: Ping-Pong
                        println!("\n1. Testing Ping-Pong...");
                        match client.ping() {
                            Ok(()) => println!("   ✓ Ping-pong successful!"),
                            Err(e) => println!("   ✗ Ping-pong failed: {}", e),
                        }
                        
                        // Test 2: Get Unity Information
                        println!("\n2. Retrieving Unity Information...");
                        
                        if let Ok(package_version) = client.get_version() {
                            println!("   Package version: {}", package_version);
                        }
                        
                        if let Ok(project_path) = client.get_project_path() {
                            println!("   Project path: {}", project_path);
                        }
                        
                        // Test 3: Listen for logs (with timeout)
                        println!("\n3. Listening for Unity logs (10 seconds)...");
                        println!("   (Try generating some logs in Unity Editor)");
                        
                        let log_future = async {
                            let mut log_count = 0;
                            client.listen_for_logs(|message| {
                                log_count += 1;
                                match message.message_type {
                                    MessageType::Info => println!("   [INFO] {}", message.value),
                                    MessageType::Warning => println!("   [WARNING] {}", message.value),
                                    MessageType::Error => println!("   [ERROR] {}", message.value),
                                    _ => println!("   [OTHER] {:?}: {}", message.message_type, message.value),
                                }
                                
                                // Stop after 5 logs or continue listening
                                log_count < 5
                            })
                        };
                        
                        match timeout(Duration::from_secs(10), log_future).await {
                            Ok(Ok(())) => println!("   ✓ Log listening completed"),
                            Ok(Err(e)) => println!("   ✗ Log listening error: {}", e),
                            Err(_) => println!("   ⏱ Log listening timed out (no logs received)"),
                        }
                        
                        println!("\n✓ Demo completed successfully!");
                    },
                    Err(e) => {
                        println!("✗ Failed to create messaging client: {}", e);
                        return Err(e.into());
                    }
                }
            } else {
                println!("✗ Unity process ID not available");
            }
        },
        Err(e) => {
            println!("✗ Unity process not found: {}", e);
            println!("\nMake sure Unity Editor is running with the project open.");
        }
    }
    
    Ok(())
}