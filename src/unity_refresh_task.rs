use std::time::{Duration, Instant, SystemTime};

use crate::unity_log_manager::UnityLogManager;
use crate::unity_log_utils::extract_main_message;
use crate::unity_messages::LogLevel;

// Timing constants for refresh operations

/// Total timeout in seconds for the refresh operation(after sent, not including compilation)
const REFRESH_TOTAL_TIMEOUT_SECS: u64 = 60;

/// Timeout in milliseconds to wait for compilation to start after refresh finished
const COMPILATION_WAIT_TIMEOUT_MILLIS: u64 = 1000;

/// Timeout in seconds for compilation to finish
const COMPILATION_FINISH_TIMEOUT_SECS: u64 = 60;

/// Wait time in seconds after compilation finishes for logs to arrive
const POST_COMPILATION_WAIT_SECS: u64 = 1;

/// Result of a refresh operation
#[derive(Debug, Clone)]
pub struct RefreshResult {
    /// Whether the refresh was completed successfully
    pub refresh_completed: bool,
    /// Error message from refresh response (if any)
    pub refresh_error_message: Option<String>,
    /// Whether compilation occurred during the refresh
    pub compilation_started: bool,
    /// Whether compilation completed during the refresh
    pub compilation_completed: bool,
    /// Error logs collected during the refresh process
    pub problems: Vec<String>,
    /// Total duration of the refresh operation in seconds
    pub duration_seconds: f64,
}

/// Task for managing Unity asset database refresh operations
pub struct UnityRefreshAssetDatabaseTask {
    operation_start: Instant,
    refresh_start_time: SystemTime,
    refresh_completed: bool,
    compilation_started: bool,
    compilation_finished: bool,
    error_message: Option<String>,
}

impl UnityRefreshAssetDatabaseTask {
    /// Create a new refresh task
    pub fn new() -> Self {
        Self {
            operation_start: Instant::now(),
            refresh_start_time: SystemTime::now(),
            refresh_completed: false,
            compilation_started: false,
            compilation_finished: false,
            error_message: None,
        }
    }

    /// Mark the refresh as started
    pub fn mark_refresh_started(&mut self) {
        self.refresh_start_time = SystemTime::now();
    }

    /// Mark the refresh as completed
    pub fn mark_refresh_completed(&mut self, success: bool, error_message: Option<String>) {
        self.refresh_completed = success;
        self.error_message = error_message;
    }

    /// Mark compilation as started
    pub fn mark_compilation_started(&mut self) {
        self.compilation_started = true;
    }

    /// Mark compilation as finished
    pub fn mark_compilation_finished(&mut self) {
        self.compilation_finished = true;
    }

    /// Build the refresh result
    pub fn build_result(
        &self,
        log_manager: &std::sync::Arc<std::sync::Mutex<UnityLogManager>>,
    ) -> RefreshResult {
        let logs = self.collect_refresh_logs(log_manager);
        let duration = self.operation_start.elapsed().as_secs_f64();

        RefreshResult {
            refresh_completed: self.refresh_completed && self.error_message.is_none(),
            refresh_error_message: self.error_message.clone(),
            compilation_started: self.compilation_started,
            compilation_completed: self.compilation_finished,
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
        self.refresh_completed && (!self.compilation_started || self.compilation_finished)
    }

    /// Get the current duration of the operation
    pub fn duration(&self) -> Duration {
        self.operation_start.elapsed()
    }

    /// Check if refresh was successful
    pub fn is_successful(&self) -> bool {
        self.refresh_completed && self.error_message.is_none()
    }

    /// Get the refresh start time
    pub fn refresh_start_time(&self) -> SystemTime {
        self.refresh_start_time
    }
}