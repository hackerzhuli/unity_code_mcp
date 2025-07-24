use std::time::{Duration, Instant, SystemTime};

use crate::unity_log_manager::UnityLogManager;
use crate::unity_log_utils::extract_main_message;
use crate::unity_messages::LogLevel;

/// States of the refresh task
#[derive(Debug, Clone, PartialEq)]
enum RefreshState {
    /// Task has been created but not started
    NotStarted,
    /// Refresh message has been sent, waiting for response
    RefreshSent,
    /// Refresh completed, waiting for compilation to start
    RefreshCompleted,
    /// Compilation has started
    CompilationStarted,
    /// Task is complete
    Finish,
    /// Task failed or timed out
    Failed,
}

/// Result of a refresh operation
#[derive(Debug, Clone)]
pub struct RefreshResult {
    /// Whether the refresh was completed successfully
    pub success: bool,
    /// Error message from refresh response (if any)
    pub refresh_error_message: Option<String>,
    /// Whether compilation completed during the refresh
    pub compiled: bool,
    /// Error logs collected during the refresh process
    pub problems: Vec<String>,
    /// Total duration of the refresh operation in seconds
    pub duration_seconds: f64,
}

/// Task for managing Unity asset database refresh operations
pub struct UnityRefreshAssetDatabaseTask {
    operation_start: Instant,
    refresh_start_time: SystemTime,
    state: RefreshState,
    error_message: Option<String>,
    // Whether compilation start occured during refresh
    is_compile: bool,
    // Configurable timeout values (all in seconds)
    refresh_timeout_secs: f64,
    compilation_wait_timeout_secs: f64,
    compilation_finish_timeout_secs: f64,
}

impl UnityRefreshAssetDatabaseTask {
    /// Create a new refresh task with configurable timeouts
    ///
    /// # Arguments
    ///
    /// * `refresh_total_timeout_secs` - Total timeout in seconds for the refresh operation (after sent, not including compilation)
    /// * `compilation_wait_timeout_secs` - Timeout in seconds to wait for compilation to start after refresh finished
    /// * `compilation_finish_timeout_secs` - Timeout in seconds for compilation to finish
    pub fn new(
        refresh_total_timeout_secs: f64,
        compilation_wait_timeout_secs: f64,
        compilation_finish_timeout_secs: f64,
    ) -> Self {
        Self {
            operation_start: Instant::now(),
            refresh_start_time: SystemTime::UNIX_EPOCH,
            state: RefreshState::NotStarted,
            error_message: None,
            is_compile: false,
            refresh_timeout_secs: refresh_total_timeout_secs,
            compilation_wait_timeout_secs,
            compilation_finish_timeout_secs,
        }
    }

    /// Mark the refresh as started
    pub fn mark_refresh_started(&mut self) -> Result<(), String> {
        if self.state != RefreshState::NotStarted {
            return Err(format!("Cannot start refresh from state {:?}", self.state));
        }
        self.state = RefreshState::RefreshSent;
        self.refresh_start_time = SystemTime::now();
        Ok(())
    }

    /// Mark the refresh as completed
    pub fn mark_refresh_completed(&mut self, success: bool, error_message: Option<String>) -> Result<(), String> {
        if self.state != RefreshState::RefreshSent {
            return Err(format!("Cannot complete refresh from state {:?}", self.state));
        }
        
        if success {
            self.state = RefreshState::RefreshCompleted;
        } else {
            self.state = RefreshState::Failed;
            self.error_message = error_message;
        }
        Ok(())
    }

    /// Mark compilation as started
    pub fn mark_compilation_started(&mut self) -> Result<(), String> {
        if self.state != RefreshState::RefreshCompleted {
            return Err(format!("Cannot start compilation from state {:?}", self.state));
        }
        self.state = RefreshState::CompilationStarted;
        self.is_compile = true;
        Ok(())
    }

    /// Mark compilation as finished
    pub fn mark_compilation_finished(&mut self) -> Result<(), String> {
        if self.state != RefreshState::CompilationStarted {
            return Err(format!("Cannot finish compilation from state {:?}", self.state));
        }
        self.state = RefreshState::Finish;
        Ok(())
    }

    /// Build the refresh result
    pub fn build_result(
        &self,
        log_manager: &std::sync::Arc<std::sync::Mutex<UnityLogManager>>,
    ) -> RefreshResult {
        let logs = self.collect_refresh_logs(log_manager);
        let duration = self.operation_start.elapsed().as_secs_f64();

        RefreshResult {
            success: self.state == RefreshState::Finish,
            refresh_error_message: self.error_message.clone(),
            compiled: self.is_compile && self.state == RefreshState::Finish,
            problems: logs,
            duration_seconds: duration,
        }
    }

    /// Collect relevant logs during refresh
    fn collect_refresh_logs(
        &self,
        log_manager: &std::sync::Arc<std::sync::Mutex<UnityLogManager>>,
    ) -> Vec<String> {
        let mut logs: Vec<String> = Vec::new();

        // Get errors and warnings during this refresh (excluding CS warnings because there can be too many)
        if let Ok(log_manager_guard) = log_manager.lock() {
            let recent_logs = log_manager_guard.get_recent_logs(self.refresh_start_time, None, None);

            for log in recent_logs {
                if !log.message.contains("warning CS")
                    && (log.level == LogLevel::Error || log.level == LogLevel::Warning)
                {
                    logs.push(extract_main_message(log.message.as_str()));
                }
            }
        }

        logs
    }

    /// Check if the task has completed
    pub fn is_completed(&self) -> bool {
        matches!(self.state, RefreshState::Finish | RefreshState::Failed)
    }

    /// Get the current duration of the operation
    pub fn duration(&self) -> Duration {
        self.operation_start.elapsed()
    }

    /// Check if refresh was successful
    pub fn is_successful(&self) -> bool {
        (self.state == RefreshState::Finish || self.state == RefreshState::RefreshCompleted) && self.error_message.is_none()
    }

    /// Get the refresh start time
    pub fn refresh_start_time(&self) -> SystemTime {
        self.refresh_start_time
    }

    /// Check if compilation has started
    pub fn has_compilation(&self) -> bool {
        self.is_compile
    }

    /// Update the task state and check for timeout
    /// 
    /// Returns `Ok(())` if no timeout occured, or an `Err` with a timeout message if the task timed out.
    pub fn update(&mut self) -> Result<(), String> {
        if self.is_completed() {
            return Ok(());
        }
        
        let elapsed = self.duration();
        let timeout_message = match self.state {
            RefreshState::RefreshSent => {
                if elapsed > Duration::from_secs_f64(self.refresh_timeout_secs) {
                    Some(format!("Refresh operation timed out after {} seconds", self.refresh_timeout_secs))
                } else {
                    None
                }
            }
            RefreshState::RefreshCompleted => {
                if elapsed > Duration::from_secs_f64(self.compilation_wait_timeout_secs) {
                    // No compilation events, mark as finished
                    self.state = RefreshState::Finish;
                }
                None
            }
            RefreshState::CompilationStarted => {
                if elapsed > Duration::from_secs_f64(self.compilation_finish_timeout_secs) {
                    Some(format!("Compilation timed out after {} seconds", self.compilation_finish_timeout_secs))
                } else {
                    None
                }
            }
            _ => None,
        };
        
        if let Some(message) = timeout_message {
            self.state = RefreshState::Failed;
            self.error_message = Some(message.clone());
            Err(message)
        } else {
            Ok(())
   }    
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unity_log_manager::{UnityLogManager, UnityLogEntry};
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

        let result = task.build_result(&log_manager);
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

        let result = task.build_result(&log_manager);
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
        assert!(update_result.unwrap_err().contains("Refresh operation timed out"));
        assert!(task.is_completed());
        assert!(!task.is_successful());

        let result = task.build_result(&log_manager);
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

        let result = task.build_result(&log_manager);
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
        assert!(task.mark_refresh_completed(false, Some(error_msg.to_string())).is_ok());
        assert!(task.is_completed());
        assert!(!task.is_successful());

        let result = task.build_result(&log_manager);
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
            log_manager_guard.add_log(LogLevel::Warning, "warning CS1234: Some compiler warning".to_string());
            log_manager_guard.add_log(LogLevel::Info, "Info message should be ignored".to_string());
        }

        // Complete refresh
        assert!(task.mark_refresh_completed(true, None).is_ok());

        let result = task.build_result(&log_manager);
        assert_eq!(result.problems.len(), 2); // Only error and warning, not CS warning or info
        assert!(result.problems.iter().any(|p| p.contains("Test error message")));
        assert!(result.problems.iter().any(|p| p.contains("Test warning message")));
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

        let result = task.build_result(&log_manager);
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

        let result = task.build_result(&log_manager);
        assert!(result.success);
        assert!(!result.compiled);
    }
}