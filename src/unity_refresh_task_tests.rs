use crate::unity_refresh_task::*;
use crate::unity_log_manager::{UnityLogEntry, UnityLogManager};
use crate::unity_messages::LogLevel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

fn create_test_log_manager() -> Arc<Mutex<UnityLogManager>> {
    Arc::new(Mutex::new(UnityLogManager::new()))
}

#[test]
fn test_new_task_creation() {
    let task = UnityRefreshAssetDatabaseTask::new(30.0, 1.0, 60.0);
    assert!(!task.is_completed());
    assert!(!task.is_successful());
    assert!(!task.has_compilation());
}

#[test]
fn test_successful_refresh_without_compilation() {
    let mut task = UnityRefreshAssetDatabaseTask::new(30.0, 1.0, 60.0);
    let log_manager = create_test_log_manager();

    // Start refresh
    assert!(task.mark_refresh_started().is_ok());
    assert!(!task.is_completed());

    // Complete refresh successfully
    assert!(task.mark_refresh_completed(true, None).is_ok());
    assert!(!task.is_completed()); // Not completed until timeout or compilation

    // Wait for compilation timeout to finish the task
    thread::sleep(Duration::from_millis(1100)); // Wait longer than compilation_wait_timeout_secs
    assert!(task.update().is_ok());
    assert!(task.is_completed());
    assert!(task.is_successful());
    assert!(!task.has_compilation());

    let result = task.build_result(&log_manager, &Vec::new());
    assert!(result.success);
    assert!(result.refresh_error_message.is_none());
    assert!(!result.compiled);
}

#[test]
fn test_successful_refresh_with_compilation() {
    let mut task = UnityRefreshAssetDatabaseTask::new(30.0, 1.0, 60.0);
    let log_manager = create_test_log_manager();

    // Start refresh
    assert!(task.mark_refresh_started().is_ok());

    // Complete refresh successfully
    assert!(task.mark_refresh_completed(true, None).is_ok());

    // Start compilation
    assert!(task.mark_compilation_started().is_ok());
    assert!(task.has_compilation());
    assert!(!task.is_completed());

    // Finish compilation
    assert!(task.mark_compilation_finished().is_ok());
    assert!(task.is_completed());
    assert!(task.is_successful());

    let result = task.build_result(&log_manager, &Vec::new());
    assert!(result.success);
    assert!(result.refresh_error_message.is_none());
    assert!(result.compiled);
}

#[test]
fn test_refresh_timeout() {
    let mut task = UnityRefreshAssetDatabaseTask::new(0.1, 1.0, 60.0); // Very short timeout
    let log_manager = create_test_log_manager();

    // Start refresh
    assert!(task.mark_refresh_started().is_ok());

    // Wait for timeout
    thread::sleep(Duration::from_millis(150));
    let update_result = task.update();
    assert!(update_result.is_err());
    assert!(
        update_result
            .unwrap_err()
            .contains("Refresh operation timed out")
    );
    assert!(task.is_completed());
    assert!(!task.is_successful());

    let result = task.build_result(&log_manager, &Vec::new());
    assert!(!result.success);
    assert!(result.refresh_error_message.is_some());
    assert!(!result.compiled);
}

#[test]
fn test_compilation_timeout() {
    let mut task = UnityRefreshAssetDatabaseTask::new(30.0, 1.0, 0.1); // Very short compilation timeout
    let log_manager = create_test_log_manager();

    // Start and complete refresh
    assert!(task.mark_refresh_started().is_ok());
    assert!(task.mark_refresh_completed(true, None).is_ok());
    assert!(task.mark_compilation_started().is_ok());

    // Wait for compilation timeout
    thread::sleep(Duration::from_millis(150));
    let update_result = task.update();
    assert!(update_result.is_err());
    assert!(update_result.unwrap_err().contains("Compilation timed out"));
    assert!(task.is_completed());
    assert!(!task.is_successful());

    let result = task.build_result(&log_manager, &Vec::new());
    assert!(!result.success);
    assert!(!result.compiled);
}

#[test]
fn test_refresh_failure() {
    let mut task = UnityRefreshAssetDatabaseTask::new(30.0, 1.0, 60.0);
    let log_manager = create_test_log_manager();

    // Start refresh
    assert!(task.mark_refresh_started().is_ok());

    // Fail refresh
    let error_msg = "Refresh failed due to Unity error";
    assert!(
        task.mark_refresh_completed(false, Some(error_msg.to_string()))
            .is_ok()
    );
    assert!(task.is_completed());
    assert!(!task.is_successful());

    let result = task.build_result(&log_manager, &Vec::new());
    assert!(!result.success);
    assert_eq!(result.refresh_error_message, Some(error_msg.to_string()));
    assert!(!result.compiled);
}

#[test]
fn test_invalid_state_transitions() {
    let mut task = UnityRefreshAssetDatabaseTask::new(30.0, 1.0, 60.0);

    // Cannot complete refresh before starting
    assert!(task.mark_refresh_completed(true, None).is_err());

    // Cannot start compilation before refresh completion
    assert!(task.mark_compilation_started().is_err());

    // Cannot finish compilation before starting
    assert!(task.mark_compilation_finished().is_err());

    // Start refresh
    assert!(task.mark_refresh_started().is_ok());

    // Cannot start refresh again
    assert!(task.mark_refresh_started().is_err());

    // Cannot start compilation before refresh completion
    assert!(task.mark_compilation_started().is_err());
}

#[test]
fn test_log_collection() {
    let mut task = UnityRefreshAssetDatabaseTask::new(30.0, 1.0, 60.0);
    let log_manager = create_test_log_manager();

    // Start refresh to set the start time
    assert!(task.mark_refresh_started().is_ok());
    let start_time = task.refresh_start_time();

    // Add some logs after refresh started
    {
        let mut log_manager_guard = log_manager.lock().unwrap();
        log_manager_guard.add_log(LogLevel::Error, "Test error message".to_string());
        log_manager_guard.add_log(LogLevel::Warning, "Test warning message".to_string());
        // CS warning should be filtered out
        log_manager_guard.add_log(
            LogLevel::Warning,
            "warning CS1234: Some compiler warning".to_string(),
        );
        log_manager_guard.add_log(LogLevel::Info, "Info message should be ignored".to_string());
    }

    // Complete refresh
    assert!(task.mark_refresh_completed(true, None).is_ok());

    let result = task.build_result(&log_manager, &Vec::new());
    assert_eq!(result.problems.len(), 2); // Only error and warning, not CS warning or info
    assert!(
        result
            .problems
            .iter()
            .any(|p| p.contains("Test error message"))
    );
    assert!(
        result
            .problems
            .iter()
            .any(|p| p.contains("Test warning message"))
    );
    assert!(!result.problems.iter().any(|p| p.contains("warning CS1234")));
    assert!(!result.problems.iter().any(|p| p.contains("Info message")));
}

#[test]
fn test_duration_tracking() {
    let mut task = UnityRefreshAssetDatabaseTask::new(30.0, 1.0, 60.0);
    let log_manager = create_test_log_manager();

    let start_duration = task.duration();
    thread::sleep(Duration::from_millis(50));
    let later_duration = task.duration();

    assert!(later_duration > start_duration);
    assert!(later_duration.as_millis() >= 50);

    // Start and complete refresh
    assert!(task.mark_refresh_started().is_ok());
    assert!(task.mark_refresh_completed(true, None).is_ok());

    let result = task.build_result(&log_manager, &Vec::new());
    assert!(result.duration_seconds > 0.0);
}

#[test]
fn test_update_when_completed() {
    let mut task = UnityRefreshAssetDatabaseTask::new(30.0, 1.0, 60.0);

    // Start and complete refresh with compilation
    assert!(task.mark_refresh_started().is_ok());
    assert!(task.mark_refresh_completed(true, None).is_ok());
    assert!(task.mark_compilation_started().is_ok());
    assert!(task.mark_compilation_finished().is_ok());

    // Update should not fail when task is completed
    assert!(task.update().is_ok());
    assert!(task.is_completed());
}

#[test]
fn test_compilation_wait_timeout_auto_finish() {
    let mut task = UnityRefreshAssetDatabaseTask::new(30.0, 0.1, 60.0); // Very short compilation wait timeout
    let log_manager = create_test_log_manager();

    // Start and complete refresh
    assert!(task.mark_refresh_started().is_ok());
    assert!(task.mark_refresh_completed(true, None).is_ok());
    assert!(!task.is_completed());

    // Wait for compilation wait timeout
    thread::sleep(Duration::from_millis(150));
    assert!(task.update().is_ok()); // Should not error, just auto-finish
    assert!(task.is_completed());
    assert!(task.is_successful());
    assert!(!task.has_compilation());

    let result = task.build_result(&log_manager, &Vec::new());
    assert!(result.success);
    assert!(!result.compiled);
}
