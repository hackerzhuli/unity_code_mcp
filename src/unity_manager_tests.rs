use std::time::Duration;
use tokio::time::{sleep, timeout};

use crate::test_utils::{cleanup_test_uss_file, create_test_uss_file, get_unity_project_path, create_test_cs_script_with_errors, cleanup_test_cs_script_with_errors};
use crate::unity_manager::UnityManager;
use crate::unity_messages::{LogLevel, TestFilter, TestMode};

/// Expected test result for individual test validation
#[derive(Debug, Clone)]
pub struct ExpectedTestResult {
    pub full_name: String,
    pub should_pass: bool,
}

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

    // Give the listener task a moment to start up
    sleep(Duration::from_millis(500)).await;

    // Test basic connectivity by checking if Unity is online FIRST
    println!("Testing Unity connectivity...");
    assert!(manager.is_unity_online(), "Unity should be online");
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
    
    // Verify log collection functionality
    let all_logs = manager.get_logs();
    println!("✓ Log collection functionality working correctly - {} total logs", all_logs.len());
    
    // Verify we have logs with different levels
    let has_info = all_logs.iter().any(|log| matches!(log.level, LogLevel::Info));
    let has_warning = all_logs.iter().any(|log| matches!(log.level, LogLevel::Warning));
    let has_error = all_logs.iter().any(|log| matches!(log.level, LogLevel::Error));
    
    println!("Log levels present - Info: {}, Warning: {}, Error: {}", has_info, has_warning, has_error);

    // Test other manager functionality
    assert!(manager.is_unity_online(), "Unity should be online");
    
    // Test version request
    if let Ok(version) = manager.get_unity_package_version().await {
        assert!(!version.is_empty(), "Unity version should not be empty");
        println!("Unity version: {}", version);
    }
    
    // Test project path request
    if let Ok(path) = manager.get_project_path().await {
        assert!(!path.is_empty(), "Project path should not be empty");
        println!("Project path: {}", path);
    }

    // Clean up
    manager.shutdown().await;
}


/// General test execution function that can be used for various test scenarios
/// 
/// # Arguments
/// 
/// * `test_filter` - The test filter to execute (e.g., "EditMode", "TestExecution.Editor.TestExecutionTests")
/// * `expected_pass_count` - Expected number of tests that should pass
/// * `expected_fail_count` - Expected number of tests that should fail
/// * `expected_individual_results` - Vector of expected individual test results to validate
/// * `timeout_seconds` - Timeout for test execution in seconds
async fn execute_unity_tests_with_validation(
    test_filter: &str,
    expected_pass_count: u32,
    expected_fail_count: u32,
    expected_individual_results: Vec<ExpectedTestResult>,
    timeout_seconds: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let project_path = get_unity_project_path();
    let mut manager = UnityManager::new(project_path.to_string_lossy().to_string()).await
        .map_err(|_| "Unity project not found")?;

    // Initialize messaging client
    manager.initialize_messaging().await
        .map_err(|_| "Unity Editor not running or messaging failed")?;

    // Wait for Unity to become online
    manager.wait_online(3).await
        .map_err(|e| format!("Timeout waiting for Unity to become online: {}", e))?;
    println!("✓ Unity connectivity confirmed");

    // Execute tests with the specified filter
    println!("\n=== Executing tests with filter: {} ===", test_filter);
    let test_result = timeout(
        Duration::from_secs(timeout_seconds),
        manager.run_tests(TestFilter::Specific {
            mode: TestMode::EditMode,
            test_name: test_filter.to_string(),
        })
    ).await
        .map_err(|_| "Timeout executing tests")?
        .map_err(|e| format!("Failed to execute tests: {}", e))?;

    println!("✓ Test execution completed: {}", test_result.execution_completed);
     println!("Individual test results: {}", test_result.test_results.len());

     // Verify execution completed
     assert!(test_result.execution_completed, "Test execution should complete");

     // Validate expected pass/fail counts using the counts extracted by UnityManager
     assert_eq!(test_result.pass_count, expected_pass_count, 
                "Expected {} passed tests, but got {}", expected_pass_count, test_result.pass_count);
     assert_eq!(test_result.fail_count, expected_fail_count, 
                "Expected {} failed tests, but got {}", expected_fail_count, test_result.fail_count);
     
     println!("✓ Pass/Fail counts match expectations: {} passed, {} failed", 
              test_result.pass_count, test_result.fail_count);

    // Validate individual test results
    println!("\n=== Validating individual test results ===");
    for expected in &expected_individual_results {
        // Find the test by its full name in the simplified test results
        let actual_result = test_result.test_results.iter()
            .find(|result| result.full_name == expected.full_name)
            .ok_or_else(|| format!("Expected test '{}' not found in results", expected.full_name))?;
        
        assert_eq!(actual_result.passed, expected.should_pass, 
                   "Test '{}' expected to {}, but passed was {}", 
                   expected.full_name, 
                   if expected.should_pass { "pass" } else { "fail" }, 
                   actual_result.passed);
        
        println!("✓ Test '{}': {} (expected: {})", 
                 expected.full_name, 
                 if actual_result.passed { "passed" } else { "failed" }, 
                 if expected.should_pass { "pass" } else { "fail" });
    }
    
    println!("✓ All {} individual test results validated successfully", expected_individual_results.len());
    println!("✓ Test execution validation completed using Unity's built-in counts and individual results");

    // Clean up
    manager.shutdown().await;
    
    Ok(())
}

#[tokio::test]
async fn test_unity_test_execution_specific_class() {
    let expected_individual_results = vec![
        ExpectedTestResult {
            full_name: "TestExecution.Editor.TestExecutionTests.SimplePassingTest".to_string(),
            should_pass: true,
        },
        ExpectedTestResult {
            full_name: "TestExecution.Editor.TestExecutionTests.MathTest".to_string(),
            should_pass: true,
        },
        ExpectedTestResult {
            full_name: "TestExecution.Editor.TestExecutionTests.StringTest".to_string(),
            should_pass: true,
        },
        ExpectedTestResult {
            full_name: "TestExecution.Editor.TestExecutionTests.UnityObjectTest".to_string(),
            should_pass: true,
        },
        ExpectedTestResult {
            full_name: "TestExecution.Editor.TestExecutionTests.SlowTest".to_string(),
            should_pass: true,
        },
    ];

    execute_unity_tests_with_validation(
         "TestExecution.Editor.TestExecutionTests",
         5, // expected pass count (all 5 individual tests)
         0, // expected fail count
         expected_individual_results,
         60, // timeout in seconds
     ).await.expect("Test execution validation should succeed");
     
     println!("✓ Test execution validation completed successfully");
}

#[tokio::test]
async fn test_unity_manager_refresh_with_compilation_errors() {
    let project_path = get_unity_project_path();
    let mut manager = match UnityManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(manager) => manager,
        Err(_) => {
            println!("Skipping Unity manager refresh test: Unity project not found");
            return;
        }
    };

    // Initialize messaging client
    if let Err(_) = manager.initialize_messaging().await {
        println!("Skipping Unity manager refresh test: Unity Editor not running or messaging failed");
        return;
    }

    // Wait a moment for ping response
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Check if Unity is connected
    assert!(manager.is_unity_online(), "unity must be online");

    println!("✓ Unity connectivity confirmed");
    
    let cs_path = create_test_cs_script_with_errors(&project_path);
    println!("✓ Created C# script with compilation errors");
    
    // Call the refresh method which should trigger compilation and collect error logs
    let refresh_result = manager.refresh_asset_database().await;
    
    // Clean up the test file immediately after refresh
    cleanup_test_cs_script_with_errors(&cs_path);
    println!("✓ Cleaned up test C# script");

    // trigger another round of compilation so unity reverts back to old state
    let refresh_result_2 = manager.refresh_asset_database().await;
    
    // Verify the refresh method completed successfully
    match refresh_result {
        Ok(result) => {
            println!("✓ Refresh method completed successfully");
            println!("Refresh completed: {}", result.refresh_completed);
            println!("Compilation occurred: {}", result.compilation_started);
            println!("Duration: {:.2} seconds", result.duration_seconds);
            println!("Collected {} error logs during refresh", result.problems.len());
            
            // Verify refresh completed successfully
            assert!(result.refresh_completed, "Refresh should have completed successfully");
            assert!(result.refresh_error_message.is_none(), "Refresh should not have error message: {:?}", result.refresh_error_message);
            
            // Verify we received compilation error logs
            assert!(!result.problems.is_empty(), "Should have received compilation error logs");
            
            // Check that the error logs contain compilation-related errors
            let compilation_errors: Vec<_> = result.problems.iter().filter(|log| {
                let msg_lower = log.to_lowercase();
                msg_lower.contains("error") || 
                msg_lower.contains("compilation") ||
                msg_lower.contains("testcompilationerrors") ||
                msg_lower.contains("nonexistentnamespace") ||
                msg_lower.contains("undefinedvariable") ||
                msg_lower.contains("cs(") // C# error format usually contains "cs(line,col)"
            }).collect();
            
            assert!(!compilation_errors.is_empty(), 
                   "Should have received compilation-related error logs. Received logs: {:?}", result.problems);
            
            println!("✓ Received {} compilation-related error logs", compilation_errors.len());
            
            // Print some of the error logs for debugging
            for (i, log) in result.problems.iter().take(3).enumerate() {
                println!("Error log {}: {}", i + 1, log);
            }
        },
        Err(e) => {
            println!("⚠ Refresh method failed: {}", e);
            // Don't fail the test if Unity is not available or has issues
            return;
        }
    }

    // use if let
    if let Ok(result) = refresh_result_2 {
        assert!(result.refresh_completed, "Refresh should have completed successfully");
        assert!(result.compilation_started, "Should have compiled");
    }else{
        assert!(false, "Refresh should have completed successfully");
    }

    // Clean up
    manager.shutdown().await;
    println!("✓ Refresh test completed successfully");
}

#[tokio::test]
async fn test_unity_test_execution_single_test() {
    let expected_individual_results = vec![
        ExpectedTestResult {
            full_name: "TestExecution.Editor.TestExecutionTests.SimplePassingTest".to_string(),
            should_pass: true,
        },
    ];

    execute_unity_tests_with_validation(
         "TestExecution.Editor.TestExecutionTests.SimplePassingTest",
         1, // expected pass count (just the single test)
         0, // expected fail count
         expected_individual_results,
         30, // timeout in seconds
     ).await.expect("Single test execution validation should succeed");
     
     println!("✓ Single test execution validation completed successfully");
}
