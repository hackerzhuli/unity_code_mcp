use std::time::Duration;
use std::path::Path;
use tokio::time::timeout;

use crate::test_utils::{get_unity_project_path, create_test_uss_file, cleanup_test_uss_file};
use crate::unity_manager::UnityManager;
use crate::unity_messaging_client::Message;

#[tokio::test]
async fn test_unity_manager_log_collection() {
    let project_path = get_unity_project_path();
    let mut manager = match UnityManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(manager) => manager,
        Err(_) => {
            println!("Skipping Unity manager test: Unity project not found");
            return;
        }
    };

    // Initialize messaging client
    if let Err(_) = manager.initialize_messaging().await {
        println!("Skipping Unity manager test: Unity Editor not running or messaging failed");
        return;
    }

    // Clear any existing logs
    manager.clear_logs();
    assert_eq!(manager.log_count(), 0, "Logs should be cleared initially");

    // Give the listener task a moment to start up
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Test basic connectivity by checking if Unity is online FIRST
    println!("Testing Unity connectivity...");
    if !manager.is_unity_online() {
        println!("⚠ Unity is not responding to ping");
        println!("This suggests Unity is running but the messaging package may not be installed or active.");
        return; // Don't fail the test, just skip it
    }
    println!("✓ Unity connectivity confirmed");
    
    // Create a USS file with invalid syntax to trigger warning logs
    let uss_path = create_test_uss_file(&project_path);
    
    // Send refresh message to Unity to trigger asset processing
    manager.refresh_unity().await
        .expect("Failed to send refresh message");
    println!("✓ Sent refresh message to Unity");

    // Poll for logs with faster response time
    let logs_received = timeout(Duration::from_secs(10), async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let current_count = manager.log_count();
            if current_count > 0 {
                // Found logs! Wait just a bit more for any additional logs
                tokio::time::sleep(Duration::from_millis(500)).await;
                return true;
            }
        }
    }).await;
    
    // Check if we received any logs
    if logs_received.is_err() {
        println!("⚠ Timeout waiting for logs from Unity");
    }

    // Clean up the test file
    cleanup_test_uss_file(&uss_path);

    // Analyze the logs we received
    let final_logs = manager.get_logs();
    
    // Check if we got any logs at all
    assert!(!final_logs.is_empty(), "Should have received some logs from Unity");
    
    // Look for any USS-related logs (could be info, warning, or error)
    let uss_related_logs: Vec<_> = final_logs.iter().filter(|log| {
        let msg_lower = log.message.to_lowercase();
        msg_lower.contains("uss") || 
        msg_lower.contains("test_errors") ||
        msg_lower.contains("parse") ||
        msg_lower.contains("syntax") ||
        msg_lower.contains("asset")
    }).collect();
    
    // Verify we got USS-related logs as expected
    assert!(!uss_related_logs.is_empty(), "Should have received USS-related logs from Unity");
    println!("✓ Received {} USS-related log(s) from Unity", uss_related_logs.len());
    
    // Test log filtering functionality
    let info_logs = manager.get_logs_by_level("Info");
    let warning_logs = manager.get_logs_by_level("Warning");
    let error_logs = manager.get_logs_by_level("Error");
    let all_logs = manager.get_logs();
    
    assert!(all_logs.len() >= info_logs.len(), "All logs should include info logs");
    assert!(all_logs.len() >= warning_logs.len(), "All logs should include warning logs");
    assert!(all_logs.len() >= error_logs.len(), "All logs should include error logs");
    
    println!("✓ Log collection and filtering functionality working correctly");

    // Test other manager functionality
    assert!(manager.is_unity_online(), "Unity should be online");
    
    // Test version request
    if let Ok(version) = manager.get_unity_version().await {
        assert!(!version.is_empty(), "Unity version should not be empty");
        println!("Unity version: {}", version);
    }
    
    // Test project path request
    if let Ok(path) = manager.get_project_path().await {
        assert!(!path.is_empty(), "Project path should not be empty");
        println!("Project path: {}", path);
    }

    // Clean up
    manager.shutdown();
}

#[tokio::test]
async fn test_unity_manager_without_unity() {
    let project_path = get_unity_project_path();
    let mut manager = UnityManager::new(project_path.to_string_lossy().to_string()).await
        .expect("Should be able to create manager even without Unity running");

    // Should fail to initialize messaging without Unity
    assert!(manager.initialize_messaging().await.is_err(), "Should fail to initialize messaging without Unity");
    
    // Should report Unity as offline
    assert!(!manager.is_unity_online(), "Unity should be offline");
    
    // Should have no logs
    assert_eq!(manager.log_count(), 0, "Should have no logs without Unity");
    
    // Should fail to get version
    assert!(manager.get_unity_version().await.is_err(), "Should fail to get version without Unity");
    
    // Should fail to get project path
    assert!(manager.get_project_path().await.is_err(), "Should fail to get project path without Unity");
}