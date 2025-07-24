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
    state: RefreshState,
    error_message: Option<String>,
    // Whether compilation occured during refresh
    is_compile: bool,
}

impl UnityRefreshAssetDatabaseTask {
    /// Create a new refresh task
    pub fn new() -> Self {
        Self {
            operation_start: Instant::now(),
            refresh_start_time: SystemTime::UNIX_EPOCH,
            state: RefreshState::NotStarted,
            error_message: None,
            is_compile: false,
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
            refresh_completed: matches!(self.state, RefreshState::RefreshCompleted | RefreshState::CompilationStarted | RefreshState::Finish),
            refresh_error_message: self.error_message.clone(),
            compilation_started: self.is_compile,
            compilation_completed: self.is_compile && self.state == RefreshState::Finish,
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
                if elapsed > Duration::from_secs(REFRESH_TOTAL_TIMEOUT_SECS) {
                    Some(format!("Refresh operation timed out after {} seconds", REFRESH_TOTAL_TIMEOUT_SECS))
                } else {
                    None
                }
            }
            RefreshState::RefreshCompleted => {
                if elapsed > Duration::from_millis(COMPILATION_WAIT_TIMEOUT_MILLIS) {
                    // No compilation events, mark as finished
                    self.state = RefreshState::Finish;
                }
                None
            }
            RefreshState::CompilationStarted => {
                if elapsed > Duration::from_secs(COMPILATION_FINISH_TIMEOUT_SECS) {
                    Some(format!("Compilation timed out after {} seconds", COMPILATION_FINISH_TIMEOUT_SECS))
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