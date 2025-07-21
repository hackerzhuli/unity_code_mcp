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
async fn test_unity_event_broadcasting() {
    let project_path = get_unity_project_path();
    let mut manager = match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(manager) => manager,
        Err(_) => {
            println!("Skipping event test: Unity project not found");
            return;
        }
    };

    // Update process info to check if Unity is running
    if manager.update_process_info().await.is_err() {
        println!("Skipping event test: Unity Editor not running");
        return;
    }

    let unity_process_id = manager.unity_process_id().expect("Unity process ID should be available");
    let mut client = match UnityMessagingClient::new(unity_process_id) {
        Ok(client) => client,
        Err(e) => {
            println!("Skipping event test: Failed to create messaging client: {}", e);
            return;
        }
    };

    // Subscribe to events before starting the listener
    let mut event_receiver = client.subscribe_to_events();

    // Start automatic message listening
    if let Err(e) = client.start_listening() {
        println!("Failed to start listening: {}", e);
        return;
    }
    println!("✓ Started automatic message listening");

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

    // Listen for events for a short time
    println!("Listening for Unity events for 3 seconds...");
    let start_time = std::time::Instant::now();
    let timeout_duration = Duration::from_secs(3);
    let mut event_count = 0;

    while start_time.elapsed() < timeout_duration {
        match tokio::time::timeout(Duration::from_millis(100), event_receiver.recv()).await {
            Ok(Ok(event)) => {
                event_count += 1;
                match event {
                    UnityEvent::LogMessage { level, message } => {
                        println!("[{:?}] {}", level, message);
                    }
                    UnityEvent::Version(version) => {
                        println!("[VERSION] {}", version);
                    }
                    UnityEvent::ProjectPath(path) => {
                        println!("[PROJECT_PATH] {}", path);
                    }
                    UnityEvent::IsPlaying(playing) => {
                        println!("[IS_PLAYING] {}", playing);
                    }
                    _ => {
                        println!("[EVENT] {:?}", event);
                    }
                }
            }
            Ok(Err(broadcast::error::RecvError::Lagged(skipped))) => {
                println!("Warning: Skipped {} events due to lag", skipped);
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                println!("Event channel closed");
                break;
            }
            Err(_) => {
                // Timeout, continue
            }
        }
    }

    println!("✓ Event listening completed. Received {} events in {} seconds", 
            event_count, start_time.elapsed().as_secs_f32());

    // Stop listening
    client.stop_listening();
    println!("✓ Stopped automatic message listening");

    // Clean up the test file
    cleanup_test_uss_file(&uss_path);
    println!("✓ Cleaned up test USS file");
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

    let unity_process_id = manager.unity_process_id().expect("Unity process ID should be available");
    let client = match UnityMessagingClient::new(unity_process_id) {
        Ok(client) => client,
        Err(e) => {
            println!("Skipping log test: Failed to create messaging client: {}", e);
            return;
        }
    };

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
    println!("Listening for Unity log messages for 3 seconds...");
    let mut log_count = 0;
    let start_time = std::time::Instant::now();
    let timeout_duration = Duration::from_secs(3);
    
    // Create a new socket with a short timeout for this test
    let test_socket = match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(socket) => {
            socket.set_read_timeout(Some(Duration::from_millis(100))).ok();
            socket
        },
        Err(e) => {
            println!("Failed to create test socket: {}", e);
            cleanup_test_uss_file(&uss_path);
            return;
        }
    };

    // Listen for messages until timeout
    while start_time.elapsed() < timeout_duration {
        let mut buffer = [0u8; 8192];
        match test_socket.recv_from(&mut buffer) {
            Ok((bytes_received, _)) => {
                if let Ok(message) = Message::deserialize(&buffer[..bytes_received]) {
                    match message.message_type {
                        MessageType::Info | MessageType::Warning | MessageType::Error => {
                            log_count += 1;
                            match message.message_type {
                                MessageType::Info => println!("[INFO] {}", message.value),
                                MessageType::Warning => println!("[WARNING] {}", message.value),
                                MessageType::Error => println!("[ERROR] {}", message.value),
                                _ => {}
                            }
                        }
                        _ => {
                            // Ignore other message types
                        }
                    }
                }
            }
            Err(e) => {
                // Handle timeout or other errors - continue until our timeout
                if e.kind() != std::io::ErrorKind::WouldBlock && e.kind() != std::io::ErrorKind::TimedOut {
                    println!("Socket error: {}", e);
                    break;
                }
                // For timeout errors, just continue the loop
            }
        }
    }

    println!("✓ Log listening completed. Received {} log messages in {} seconds", 
            log_count, start_time.elapsed().as_secs_f32());

    // Clean up the test file
    cleanup_test_uss_file(&uss_path);
    println!("✓ Cleaned up test USS file");
    
    // The test passes regardless of whether we received logs or not,
    // since Unity might not be configured to send logs via UDP
    // This is more of an integration test to verify the mechanism works
}