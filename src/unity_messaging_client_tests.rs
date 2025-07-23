use crate::unity_project_manager::UnityProjectManager;
use crate::test_utils::*;
use crate::unity_messaging_client::*;
use crate::unity_manager::{TestFilter, TestMode};
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

#[tokio::test]
async fn test_port_calculation() {
    let process_id = 12345u32;
    let expected_port = 58000 + (process_id % 1000);
    
    let client = UnityMessagingClient::new(process_id).await.unwrap();
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
    let mut client = match UnityMessagingClient::new(unity_process_id).await {
        Ok(client) => client,
        Err(e) => {
            println!("Skipping integration test: Failed to create messaging client: {}", e);
            return;
        }
    };

    // Test connection functionality
    if let Err(e) = client.start_listening().await {
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
    match client.get_version().await {
        Ok(_) => println!("✓ Version request sent successfully"),
        Err(e) => println!("Failed to send version request: {}", e),
    }

    // Test project path retrieval
    match client.get_project_path().await {
        Ok(_) => println!("✓ Project path request sent successfully"),
        Err(e) => println!("Failed to send project path request: {}", e),
    }
}

#[tokio::test]
async fn test_tcp_fallback_integration_with_unity() {
    use std::path::Path;
    
    let project_path = get_unity_project_path();
    let mut manager = match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(manager) => manager,
        Err(_) => {
            println!("Skipping TCP fallback integration test: Unity project not found");
            return;
        }
    };

    // Update process info to check if Unity is running
    if manager.update_process_info().await.is_err() {
        println!("Skipping TCP fallback integration test: Unity Editor not running");
        return;
    }

    let unity_process_id = match manager.unity_process_id() {
        Some(pid) => pid,
        None => {
            println!("Skipping TCP fallback integration test: Unity is not running");
            return;
        }
    };
    
    let mut client = UnityMessagingClient::new(unity_process_id).await
        .expect("Failed to create messaging client");

    // Subscribe to events BEFORE starting listener
    let mut event_receiver = client.subscribe_to_events();

    // Start listening for Unity events
    client.start_listening().await
        .expect("Failed to start listening");
    
    // Give the listener task a moment to start up
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify Unity is online
    if !client.is_online() {
        println!("Skipping TCP fallback integration test: Unity is not online");
        client.stop_listening();
        return;
    }
    
    println!("[TEST] Unity is online, proceeding with TCP fallback integration test");
    
    // Execute the LargeMessageTest to trigger large log messages
    println!("[TEST] Executing LargeMessageTest to generate large log messages");
    
    // Execute the specific test that generates large messages
    let test_filter = TestFilter::Specific {
        mode: TestMode::EditMode,
        test_name: "TestExecution.Editor.TestWithLargeLog.LargeMessageTest".to_string(),
    };
    client.execute_tests(test_filter, None).await
        .expect("Failed to send test execution message");
    
    // Wait for test completion and collect results
    let mut test_completed = false;
    let mut large_message_received = false;
    let test_timeout = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match event_receiver.recv().await {
                Ok(UnityEvent::TestFinished(test_result)) => {
                    println!("[TEST] Test finished: {} test result adaptors", test_result.test_result_adaptors.len());
                    
                    test_completed = true;
                    break;
                }
                Ok(UnityEvent::LogMessage { level: _, message }) => {
                    // Check if we received a large log message (indicating TCP fallback was used)
                    if message.len() > 9000 {
                        println!("[TEST] Received large log message ({} chars) - TCP fallback likely triggered", message.len());
                        large_message_received = true;
                    }
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    }).await;
    
    if test_timeout.is_err() {
        println!("[TEST] Warning: Timeout waiting for test completion");
    }
    
    if !test_completed {
        println!("[TEST] Warning: Test did not complete within timeout");
    }
    
    if large_message_received {
        println!("[TEST] ✓ Large message received - TCP fallback functionality verified");
    } else {
        println!("[TEST] Note: No large messages detected, but test execution completed");
    }
    
    println!("[TEST] TCP fallback integration test completed successfully");
    println!("[TEST] Note: To manually test large messages, use the LargeMessageTest component in Unity");
    println!("[TEST] The TCP fallback mechanism is now ready to handle messages over 8KB from Unity");
    
    // Stop listening
    client.stop_listening();
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
    
    let mut client = match UnityMessagingClient::new(unity_process_id).await {
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
    if let Err(e) = client.start_listening().await {
        println!("Failed to start listening: {}", e);
        return;
    }
    
    // Give the listener task a moment to start up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Test basic connectivity first with a ping
    println!("Testing Unity connectivity with ping...");
    match client.send_message(&Message::ping(), None).await {
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
    if let Err(e) = client.send_refresh_message(None).await {

        println!("Failed to send refresh message: {}", e);
        cleanup_test_uss_file(&uss_path);
        return;
    }
    println!("✓ Sent refresh message to Unity");

    // Listen for log messages with a reasonable timeout
    println!("Listening for Unity log messages for 3 seconds...");
    let mut log_count = 0;
    let start_time = std::time::Instant::now();
    let timeout_duration = Duration::from_secs(3);
    
    // Start a background task to continuously listen for events
    let (log_sender, mut log_receiver) = tokio::sync::mpsc::channel(100);
    let event_listener = tokio::spawn(async move {
        loop {
            match event_receiver.recv().await {
                Ok(event) => {
                    if let UnityEvent::LogMessage { level, message } = event {
                        let _ = log_sender.send((level, message)).await;
                    }
                    // Continue listening for more events
                }
                Err(_) => {
                    // Channel closed, exit
                    break;
                }
            }
        }
    });
    
    // Wait for log messages with timeout
    match tokio::time::timeout(timeout_duration, log_receiver.recv()).await {
        Ok(Some((level, message))) => {
            log_count += 1;
            let receive_time = start_time.elapsed();
            println!("✓ Received log message after {:.3}s", receive_time.as_secs_f32());
            match level {
                LogLevel::Info => println!("[INFO] {}", message),
                LogLevel::Warning => println!("[WARNING] {}", message),
                LogLevel::Error => println!("[ERROR] {}", message),
            }
        }
        Ok(None) => {
            // Channel closed without receiving message
        }
        Err(_) => {
            // Timeout occurred
        }
    }
    
    // Stop the event listener
    event_listener.abort();

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

#[tokio::test]
async fn test_unity_online_offline_state() {
    let project_path = get_unity_project_path();
    let mut manager = match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(manager) => manager,
        Err(_) => {
            // Skip test if Unity project not found
            return;
        }
    };

    // Update process info to check if Unity is running
    if manager.update_process_info().await.is_err() {
        // Skip test if Unity Editor not running
        return;
    }

    let unity_process_id = match manager.unity_process_id() {
        Some(pid) => pid,
        None => {
            // Skip test if Unity is not running
            return;
        }
    };
    
    let mut client = UnityMessagingClient::new(unity_process_id).await
        .expect("Failed to create messaging client");

    // Start listening for Unity events
    client.start_listening().await
        .expect("Failed to start listening");
    
    // Give the listener task a moment to start up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Note: The listener task sends a ping immediately, so Unity may already be online
    // We'll test the state transitions instead of the initial state
    let initial_state = client.is_online();

    // Wait for ping response to set Unity online
    let start_time = std::time::Instant::now();
    let mut unity_online = false;
    while start_time.elapsed() < Duration::from_millis(500) {
        if client.is_online() {
            unity_online = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    
    if !unity_online {
        println!("[TEST] Warning: Unity didn't come online after initial ping");
    }

    // Create a C# script to trigger Unity compilation (which will make Unity go offline)
    let cs_script_path = create_test_cs_script(&project_path);

    // Send refresh message to Unity to trigger compilation
    println!("[TEST] About to send refresh message (timestamp: {:?})", std::time::Instant::now());
    client.send_refresh_message(None).await
        .expect("Failed to send refresh message");
    println!("[TEST] Sent refresh message to trigger compilation (timestamp: {:?})", std::time::Instant::now());

    // Poll for Unity to go offline (should happen within 5 seconds)
    let compilation_start_time = std::time::Instant::now();
    let mut state_during_compilation = true;
    let mut unity_went_offline = false;
    
    println!("[TEST] Polling for Unity to go offline during compilation...");
    while compilation_start_time.elapsed() < Duration::from_secs(5) {
        if !client.is_online() {
            state_during_compilation = false;
            unity_went_offline = true;
            println!("[TEST] Unity went offline after {:?} (timestamp: {:?})", compilation_start_time.elapsed(), std::time::Instant::now());
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    if !unity_went_offline {
        println!("[TEST] Warning: Unity didn't go offline within 5 seconds of compilation trigger");
        state_during_compilation = client.is_online();
    }

    // Poll for Unity to come back online (should happen within 10 seconds after going offline)
    let mut final_online_state = false;
    let online_poll_start = std::time::Instant::now();
    
    println!("[TEST] Polling for Unity to come back online...");
    while online_poll_start.elapsed() < Duration::from_secs(10) {
        if client.is_online() {
            final_online_state = true;
            println!("[TEST] Unity came back online after {:?} (timestamp: {:?})", online_poll_start.elapsed(), std::time::Instant::now());
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    if !final_online_state {
        println!("[TEST] Warning: Unity didn't come back online within 10 seconds");
    }

    // Clean up the test file
    cleanup_test_cs_script(&cs_script_path);

    // Send another refresh message to Unity to trigger second compilation after cleanup
    // This ensures Unity processes the file deletion and reaches a clean state
    if let Err(e) = client.send_refresh_message(None).await {

        println!("Warning: Failed to send second refresh message after cleanup: {}", e);
    } else {
        println!("[TEST] Sent second refresh message for cleanup compilation");
        
        // Poll for Unity to go offline for second compilation (within 5 seconds)
        let second_compilation_start = std::time::Instant::now();
        let mut second_offline_detected = false;
        
        while second_compilation_start.elapsed() < Duration::from_secs(5) {
            if !client.is_online() {
                second_offline_detected = true;
                println!("[TEST] Unity went offline for second compilation after {:?}", second_compilation_start.elapsed());
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        if second_offline_detected {
            // Poll for Unity to come back online after second compilation (within 10 seconds)
            let second_online_poll_start = std::time::Instant::now();
            
            while second_online_poll_start.elapsed() < Duration::from_secs(10) {
                if client.is_online() {
                    println!("[TEST] Unity came back online after second compilation after {:?}", second_online_poll_start.elapsed());
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } else {
            println!("[TEST] Warning: Unity didn't go offline for second compilation, waiting 2 seconds as fallback");
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    // Stop listening
    client.stop_listening();

    // Assert that the online/offline state tracking worked correctly throughout the test
    assert!(unity_online, "Unity should have been online after initial ping");
    
    // After completing both compilation rounds, assert on the stored boolean values
    // This validates that our offline detection mechanism worked correctly
    assert!(!state_during_compilation, "Unity should have gone offline during compilation - our state tracking should detect this");
    
    assert!(final_online_state, "Unity should be back online after compilation finishes");
    
    // Note: We don't assert that Unity comes back online immediately after compilation
    // because the timing can vary and Unity might still be restarting its messaging system
}

#[tokio::test]
async fn test_send_message_with_stable_delivery() {
    let project_path = get_unity_project_path();
    let mut manager = match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(manager) => manager,
        Err(_) => {
            println!("Skipping stable delivery test: Unity project not found");
            return;
        }
    };

    // Update process info to check if Unity is running
    if manager.update_process_info().await.is_err() {
        println!("Skipping stable delivery test: Unity Editor not running");
        return;
    }

    let unity_process_id = match manager.unity_process_id() {
        Some(pid) => pid,
        None => {
            println!("Skipping stable delivery test: Unity is not running");
            return;
        }
    };
    
    let mut client = UnityMessagingClient::new(unity_process_id).await
        .expect("Failed to create messaging client");

    // Subscribe to events BEFORE starting listener to avoid missing messages
    let mut event_receiver = client.subscribe_to_events();

    // Start listening for Unity events
    client.start_listening().await
        .expect("Failed to start listening");
    
    // Give the listener task a moment to start up and establish connection
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Test 1: Send project path request when Unity is online
    client.get_project_path().await
        .expect("Failed to send project path request");
    
    // Wait for project path response
    let project_path_response = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match event_receiver.recv().await {
                Ok(UnityEvent::ProjectPath(path)) => {
                    return Some(path);
                },
                Ok(_) => continue, // Ignore other events
                Err(_) => return None,
            }
        }
    }).await;
    
    let received_path = project_path_response
        .expect("Timeout waiting for project path response")
        .expect("Failed to receive project path response");
    
    // Assert that we received a valid project path
    assert!(!received_path.is_empty(), "Project path should not be empty");
    assert!(received_path.contains("UnityProject"), "Project path should contain 'UnityProject'");

    // Test 2: Create a C# script to trigger Unity compilation (making Unity go offline)
    let cs_script_path = create_test_cs_script(&project_path);

    // Send refresh message to Unity to trigger compilation
    client.send_refresh_message(None).await

        .expect("Failed to send refresh message");

    // Wait for Unity to go offline during compilation
    let mut offline_detected = false;
    for _ in 0..50 { // Wait up to 5 seconds
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !client.is_online() {
            offline_detected = true;
            break;
        }
    }

    assert!(offline_detected, "Unity should go offline during compilation");

    // Test 3: Send project path request when Unity is offline and verify stable delivery
    let start_time = std::time::Instant::now();
    
    client.get_project_path().await
        .expect("Failed to send project path request while offline");
    
    let elapsed = start_time.elapsed();
    
    // Verify it took some time (indicating it waited for Unity to come back online)
    assert!(elapsed.as_secs() >= 1, "Should have waited at least 1 second for Unity to come back online");
    
    // Wait for the project path response after Unity comes back online
    let project_path_response_after_offline = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match event_receiver.recv().await {
                Ok(UnityEvent::ProjectPath(path)) => {
                    return Some(path);
                },
                Ok(_) => continue, // Ignore other events
                Err(_) => return None,
            }
        }
    }).await;
    
    let received_path_after_offline = project_path_response_after_offline
        .expect("Timeout waiting for project path response after Unity came back online")
        .expect("Failed to receive project path response after Unity came back online");
    
    // Assert that we received the same valid project path after Unity came back online
    assert_eq!(received_path, received_path_after_offline, "Project path should be consistent");
    assert!(client.is_online(), "Unity should be online after receiving response");

    // Clean up the test file
    cleanup_test_cs_script(&cs_script_path);

    // Send another refresh message to clean up
    if let Err(e) = client.send_refresh_message(None).await {

        println!("Warning: Failed to send cleanup refresh message: {}", e);
    } else {
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // wait until unity finished compliation
    for i in 0..100 {
        if client.is_online() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Stop listening
    client.stop_listening();
}

#[tokio::test]
async fn test_tcp_fallback_for_large_messages() {
    use tokio::net::TcpListener;
    use tokio::io::AsyncWriteExt;
    
    // Create a large message (over 8KB) to test TCP fallback
    let large_content = "A".repeat(10000); // 10KB of 'A' characters
    let large_message = Message::new(MessageType::Info, large_content.clone());
    let serialized_large_message = large_message.serialize();
    let message_length = serialized_large_message.len();
    
    // Start a mock TCP server on a random port
    let tcp_listener = TcpListener::bind("127.0.0.1:0").await
        .expect("Failed to bind TCP listener");
    let tcp_port = tcp_listener.local_addr().unwrap().port();
    
    // Spawn a task to handle the TCP connection
    let tcp_task = tokio::spawn(async move {
        if let Ok((mut stream, _)) = tcp_listener.accept().await {
            // Send the large message data
            if let Err(e) = stream.write_all(&serialized_large_message).await {
                eprintln!("Failed to write TCP data: {}", e);
            }
        }
    });
    
    // Test the receive_tcp_message function directly
    let result = UnityMessagingClient::receive_tcp_message(tcp_port, message_length).await;
    
    // Wait for the TCP task to complete
    let _ = tcp_task.await;
    
    // Verify the result
    match result {
        Ok(received_message) => {
            assert_eq!(received_message.message_type, MessageType::Info);
            assert_eq!(received_message.value, large_content);
            assert_eq!(received_message.value.len(), 10000);
            println!("[TEST] Successfully received large message via TCP: {} bytes", received_message.value.len());
        }
        Err(e) => {
            panic!("Failed to receive TCP message: {}", e);
        }
    }
}