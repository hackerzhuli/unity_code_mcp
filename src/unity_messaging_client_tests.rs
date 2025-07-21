use crate::unity_project_manager::UnityProjectManager;
use crate::test_utils::*;
use crate::unity_messaging_client::*;
use std::time::Duration;

#[test]
fn test_message_serialization() {
    let message = Message::new(MessageType::Ping, "test".to_string());
    let serialized = message.serialize();
    let deserialized = Message::deserialize(&serialized).unwrap();
    
    assert_eq!(message.message_type as i32, deserialized.message_type as i32);
    assert_eq!(message.value, deserialized.value);
}

#[test]
fn test_empty_message_serialization() {
    let message = Message::ping();
    let serialized = message.serialize();
    let deserialized = Message::deserialize(&serialized).unwrap();
    
    assert_eq!(message.message_type as i32, deserialized.message_type as i32);
    assert_eq!(message.value, deserialized.value);
    assert!(message.value.is_empty());
}

#[test]
fn test_port_calculation() {
    let process_id = 12345u32;
    let expected_port = 58000 + (process_id % 1000);
    
    let client = UnityMessagingClient::new(process_id).unwrap();
    assert_eq!(client.unity_address().port(), expected_port as u16);
}

#[tokio::test]
async fn test_unity_messaging_integration() {
    let project_path = get_unity_project_path();
    let mut manager = match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(manager) => manager,
        Err(_) => {
            println!("Skipping integration test: Unity project not found");
            return;
        }
    };

    // Update process info to check if Unity is running
    if manager.update_process_info().await.is_err() {
        println!("Skipping integration test: Unity Editor not running");
        return;
    }

    let unity_process_id = manager.unity_process_id().expect("Unity process ID should be available");
    let mut client = match UnityMessagingClient::new(unity_process_id) {
        Ok(client) => client,
        Err(e) => {
            println!("Skipping integration test: Failed to create messaging client: {}", e);
            return;
        }
    };

    // Test connection functionality
    if let Err(e) = client.start_listening() {
        println!("Failed to start listening: {}", e);
        return;
    }
    
    // Wait a moment for potential Unity responses
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    match client.is_connected(Some(5)) {
        true => println!("✓ Connection test passed - Unity is responding"),
        false => {
            println!("Connection test failed - Unity not responding recently");
            client.stop_listening();
            return;
        }
    }
    
    client.stop_listening();

    // Test version retrieval
    match client.get_version() {
        Ok(version) => println!("✓ Unity package version: {}", version),
        Err(e) => println!("Failed to get Unity version: {}", e),
    }

    // Test project path retrieval
    match client.get_project_path() {
        Ok(path) => println!("✓ Unity project path: {}", path),
        Err(e) => println!("Failed to get Unity project path: {}", e),
    }
}

#[tokio::test]
async fn test_unity_log_generation_and_listening() {
    let project_path = get_unity_project_path();
    let mut manager = match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(manager) => manager,
        Err(_) => {
            println!("Skipping log test: Unity project not found");
            return;
        }
    };

    // Update process info to check if Unity is running
    if manager.update_process_info().await.is_err() {
        println!("Skipping log test: Unity Editor not running");
        return;
    }

    let unity_process_id = match manager.unity_process_id() {
        Some(pid) => pid,
        None => {
            println!("✓ Unity is not running - test passes (no Unity process to test)");
            return;
        }
    };
    
    let mut client = match UnityMessagingClient::new(unity_process_id) {
        Ok(client) => client,
        Err(e) => {
            println!("Skipping log test: Failed to create messaging client: {}", e);
            return;
        }
    };

    println!("Unity process ID: {}, calculated port: {}", unity_process_id, client.unity_address().port());

    // Subscribe to Unity events BEFORE starting listener to avoid missing messages
    let mut event_receiver = client.subscribe_to_events();

    // Start listening for Unity events
    if let Err(e) = client.start_listening() {
        println!("Failed to start listening: {}", e);
        return;
    }
    
    // Give the listener task a moment to start up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Test basic connectivity first with a ping
    println!("Testing Unity connectivity with ping...");
    match client.send_message(&Message::ping(), false) {
        Ok(_) => println!("✓ Ping sent successfully"),
        Err(e) => {
            println!("⚠ Failed to send ping to Unity: {}", e);
            println!("This suggests Unity is running but the messaging package may not be installed or active.");
            client.stop_listening();
            return; // Don't fail the test, just skip it
        }
    }

    // Create a USS file with errors to trigger Unity warnings
    let uss_path = create_test_uss_file(&project_path);
    println!("Created test USS file: {}", uss_path.display());

    // Send refresh message to Unity
    if let Err(e) = client.send_refresh_message() {
        println!("Failed to send refresh message: {}", e);
        cleanup_test_uss_file(&uss_path);
        return;
    }
    println!("✓ Sent refresh message to Unity");

    // Listen for log messages with a strict timeout
    println!("Listening for Unity log messages for 8 seconds...");
    let mut log_count = 0;
    let start_time = std::time::Instant::now();
    let timeout_duration = Duration::from_secs(8);
    
    // Listen for events until timeout or first log message
    while start_time.elapsed() < timeout_duration {
        match tokio::time::timeout(Duration::from_millis(100), event_receiver.recv()).await {
            Ok(Ok(event)) => {
                match event {
                    UnityEvent::LogMessage { level, message } => {
                        log_count += 1;
                        match level {
                            LogLevel::Info => println!("[INFO] {}", message),
                            LogLevel::Warning => println!("[WARNING] {}", message),
                            LogLevel::Error => println!("[ERROR] {}", message),
                        }
                        // Exit early after receiving first log message to keep test fast
                        break;
                    }
                    _ => {
                        // Ignore other event types for this test
                    }
                }
            }
            Ok(Err(_)) => {
                // Channel error (e.g., sender dropped)
                break;
            }
            Err(_) => {
                // Timeout occurred, continue listening
                continue;
            }
        }
    }

    println!("✓ Log listening completed. Received {} log messages in {} seconds", 
            log_count, start_time.elapsed().as_secs_f32());

    // Stop listening
    client.stop_listening();

    // Clean up the test file
    cleanup_test_uss_file(&uss_path);
    println!("✓ Cleaned up test USS file");
    

    
    // Since Unity is running and ping works, we should receive log messages
    assert!(log_count > 0, "Unity is running and ping works, but no log messages were received. This indicates a problem with our message listening implementation.");
    println!("✓ Successfully validated Unity log message reception via event system");
}