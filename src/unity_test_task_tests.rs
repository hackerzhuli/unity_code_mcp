use std::time::{Duration, Instant};
use std::thread;

use crate::unity_messages::{TestAdaptor, TestFilter, TestResultAdaptor};
use crate::unity_test_task::{UnityTestExecutionTask, TestExecutionState};

/// Helper function to create a test filter
fn create_test_filter() -> TestFilter {
    TestFilter::All(crate::unity_messages::TestMode::EditMode)
}

/// Helper function to create a test adaptor
fn create_test_adaptor(id: &str, full_name: &str, has_children: bool, test_count: u32) -> TestAdaptor {
    TestAdaptor {
        id: id.to_string(),
        name: full_name.split('.').last().unwrap_or(full_name).to_string(),
        full_name: full_name.to_string(),
        test_type: 0,
        parent: 0,
        source: "".to_string(),
        test_count,
        has_children,
    }
}

/// Helper function to create a test result adaptor
fn create_test_result_adaptor(
    test_id: &str,
    result_state: &str,
    duration: f64,
    has_children: bool,
    pass_count: u32,
    fail_count: u32,
    skip_count: u32,
) -> TestResultAdaptor {
    TestResultAdaptor {
        test_id: test_id.to_string(),
        result_state: result_state.to_string(),
        test_status: 0,
        duration,
        start_time: 1704067200000, // 2024-01-01T00:00:00.000Z as Unix timestamp
        end_time: 1704067201000,   // 2024-01-01T00:00:01.000Z as Unix timestamp
        stack_trace: "".to_string(),
        message: "".to_string(),
        output: "".to_string(),
        assert_count: 1,
        inconclusive_count: 0,
        pass_count,
        fail_count,
        skip_count,
        has_children,
        parent: 0,
    }
}

#[test]
fn test_new_task_creation() {
    let filter = create_test_filter();
    let task = UnityTestExecutionTask::new(filter.clone(), 30.0, 60.0, 10.0);
    
    assert!(!task.is_completed());
    assert!(!task.is_successful());
    assert_eq!(task.filter(), &filter);
}

#[test]
fn test_mark_test_execution_started() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 30.0, 60.0, 10.0);
    
    // Should succeed from NotStarted state
    assert!(task.mark_test_execution_started().is_ok());
    
    // Should fail if called again
    assert!(task.mark_test_execution_started().is_err());
}

#[test]
fn test_mark_test_run_started() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 30.0, 60.0, 10.0);
    
    // Should fail from NotStarted state
    let adaptors = vec![create_test_adaptor("test1", "TestClass.Test1", false, 1)];
    assert!(task.mark_test_run_started(adaptors.clone()).is_err());
    
    // Should succeed after marking execution started
    task.mark_test_execution_started().unwrap();
    assert!(task.mark_test_run_started(adaptors).is_ok());
}

#[test]
fn test_handle_test_started() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 30.0, 60.0, 10.0);
    
    // Prepare task state
    task.mark_test_execution_started().unwrap();
    let root_adaptors = vec![create_test_adaptor("root", "TestSuite", true, 2)];
    task.mark_test_run_started(root_adaptors).unwrap();
    
    // Handle test started
    let test_adaptors = vec![
        create_test_adaptor("test1", "TestClass.Test1", false, 1),
        create_test_adaptor("test2", "TestClass.Test2", false, 1),
    ];
    
    assert!(task.handle_test_started(test_adaptors).is_ok());
}

#[test]
fn test_handle_test_finished() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 30.0, 60.0, 10.0);
    
    // Prepare task state
    task.mark_test_execution_started().unwrap();
    let root_adaptors = vec![create_test_adaptor("root", "TestSuite", true, 2)];
    task.mark_test_run_started(root_adaptors).unwrap();
    
    let test_adaptors = vec![
        create_test_adaptor("test1", "TestClass.Test1", false, 1),
        create_test_adaptor("test2", "TestClass.Test2", false, 1),
    ];
    task.handle_test_started(test_adaptors).unwrap();
    
    // Handle test finished
    let result_adaptors = vec![
        create_test_result_adaptor("test1", "Passed", 1.5, false, 1, 0, 0),
        create_test_result_adaptor("test2", "Failed", 2.0, false, 0, 1, 0),
    ];
    
    assert!(task.handle_test_finished(result_adaptors).is_ok());
}

#[test]
fn test_handle_test_run_finished() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 30.0, 60.0, 10.0);
    
    // Prepare task state
    task.mark_test_execution_started().unwrap();
    let root_adaptors = vec![create_test_adaptor("root", "TestSuite", true, 2)];
    task.mark_test_run_started(root_adaptors).unwrap();
    
    let test_adaptors = vec![
        create_test_adaptor("test1", "TestClass.Test1", false, 1),
        create_test_adaptor("test2", "TestClass.Test2", false, 1),
    ];
    task.handle_test_started(test_adaptors).unwrap();
    
    let test_result_adaptors = vec![
        create_test_result_adaptor("test1", "Passed", 1.5, false, 1, 0, 0),
        create_test_result_adaptor("test2", "Failed", 2.0, false, 0, 1, 0),
    ];
    task.handle_test_finished(test_result_adaptors.clone()).unwrap();
    
    // Handle test run finished
    let summary_adaptors = vec![
        create_test_result_adaptor("root", "Mixed", 3.5, true, 1, 1, 0),
    ];
    
    let result = task.handle_test_run_finished(summary_adaptors).unwrap();
    
    assert!(task.is_completed());
    assert!(task.is_successful());
    assert_eq!(result.pass_count, 1);
    assert_eq!(result.fail_count, 1);
    assert_eq!(result.skip_count, 0);
    assert_eq!(result.test_count, 2);
    assert_eq!(result.duration_seconds, 3.5);
    assert!(result.execution_completed);
    assert_eq!(result.test_results.len(), 2);
}

#[test]
fn test_timeout_test_run_start() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 0.1, 60.0, 10.0); // Very short timeout
    
    task.mark_test_execution_started().unwrap();
    
    // Wait for timeout
    thread::sleep(Duration::from_millis(150));
    
    let result = task.update();
    assert!(result.is_err());
    assert!(task.is_completed());
    assert!(!task.is_successful());
}

#[test]
fn test_timeout_individual_test() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 30.0, 0.1, 10.0); // Very short test timeout
    
    // Prepare task state
    task.mark_test_execution_started().unwrap();
    let root_adaptors = vec![create_test_adaptor("root", "TestSuite", true, 1)];
    task.mark_test_run_started(root_adaptors).unwrap();
    
    let test_adaptors = vec![
        create_test_adaptor("test1", "TestClass.Test1", false, 1),
    ];
    task.handle_test_started(test_adaptors).unwrap();
    
    // Wait for timeout
    thread::sleep(Duration::from_millis(150));
    
    let result = task.update();
    assert!(result.is_err());
    assert!(task.is_completed());
    assert!(!task.is_successful());
}

#[test]
fn test_timeout_test_start_delay() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 30.0, 60.0, 0.1); // Very short start timeout
    
    // Prepare task state
    task.mark_test_execution_started().unwrap();
    let root_adaptors = vec![create_test_adaptor("root", "TestSuite", true, 2)];
    task.mark_test_run_started(root_adaptors).unwrap();
    
    let test_adaptors = vec![
        create_test_adaptor("test1", "TestClass.Test1", false, 1),
    ];
    task.handle_test_started(test_adaptors).unwrap();
    
    // Finish first test
    let result_adaptors = vec![
        create_test_result_adaptor("test1", "Passed", 1.0, false, 1, 0, 0),
    ];
    task.handle_test_finished(result_adaptors).unwrap();
    
    // Wait for timeout (no second test starts)
    thread::sleep(Duration::from_millis(150));
    
    let result = task.update();
    assert!(result.is_err());
    assert!(task.is_completed());
    assert!(!task.is_successful());
}

#[test]
fn test_create_test_result_with_error() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 30.0, 60.0, 10.0);
    
    // Prepare task state with some test results
    task.mark_test_execution_started().unwrap();
    let root_adaptors = vec![create_test_adaptor("root", "TestSuite", true, 3)];
    task.mark_test_run_started(root_adaptors).unwrap();
    
    let test_adaptors = vec![
        create_test_adaptor("test1", "TestClass.Test1", false, 1),
        create_test_adaptor("test2", "TestClass.Test2", false, 1),
        create_test_adaptor("test3", "TestClass.Test3", false, 1),
    ];
    task.handle_test_started(test_adaptors).unwrap();
    
    // Only finish some tests
    let result_adaptors = vec![
        create_test_result_adaptor("test1", "Passed", 1.0, false, 1, 0, 0),
        create_test_result_adaptor("test2", "Failed", 2.0, false, 0, 1, 0),
    ];
    task.handle_test_finished(result_adaptors).unwrap();
    
    let error_result = task.create_test_result_with_error("Test execution failed");
    
    assert!(!error_result.execution_completed);
    assert_eq!(error_result.error_message, "Test execution failed");
    assert_eq!(error_result.pass_count, 1);
    assert_eq!(error_result.fail_count, 2); // 1 failed + 1 unfinished
    assert_eq!(error_result.test_count, 3);
    assert_eq!(error_result.skip_count, 0);
    assert_eq!(error_result.test_results.len(), 2);
}

#[test]
fn test_successful_complete_flow() {
    let filter = create_test_filter();
    let mut task = UnityTestExecutionTask::new(filter, 30.0, 60.0, 10.0);
    
    // Complete flow
    assert!(task.mark_test_execution_started().is_ok());
    
    let root_adaptors = vec![create_test_adaptor("root", "TestSuite", true, 2)];
    assert!(task.mark_test_run_started(root_adaptors).is_ok());
    
    let test_adaptors = vec![
        create_test_adaptor("test1", "TestClass.Test1", false, 1),
        create_test_adaptor("test2", "TestClass.Test2", false, 1),
    ];
    assert!(task.handle_test_started(test_adaptors).is_ok());
    
    let test_result_adaptors = vec![
        create_test_result_adaptor("test1", "Passed", 1.5, false, 1, 0, 0),
        create_test_result_adaptor("test2", "Passed", 2.0, false, 1, 0, 0),
    ];
    assert!(task.handle_test_finished(test_result_adaptors).is_ok());
    
    let summary_adaptors = vec![
        create_test_result_adaptor("root", "Passed", 3.5, true, 2, 0, 0),
    ];
    let result = task.handle_test_run_finished(summary_adaptors).unwrap();
    
    assert!(task.is_completed());
    assert!(task.is_successful());
    assert_eq!(result.pass_count, 2);
    assert_eq!(result.fail_count, 0);
    assert!(result.execution_completed);
    
    // Update should not fail after completion
    assert!(task.update().is_ok());
}

#[test]
fn test_duration_tracking() {
    let filter = create_test_filter();
    let task = UnityTestExecutionTask::new(filter, 30.0, 60.0, 10.0);
    
    let start_duration = task.duration();
    thread::sleep(Duration::from_millis(10));
    let end_duration = task.duration();
    
    assert!(end_duration > start_duration);
    assert!(end_duration.as_millis() >= 10);
}