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

/// Result of a Unity asset database refresh operation
/// 
/// Contains comprehensive information about the refresh process including success status,
/// compilation results, error messages, and performance metrics.
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
/// 
/// This struct encapsulates the entire lifecycle of a Unity asset database refresh,
/// including the refresh itself and any subsequent compilation that may occur.
/// It provides timeout management, state tracking, and result collection capabilities.
/// 
/// The task follows this state flow:
/// 1. NotStarted -> RefreshSent (when refresh message is sent)
/// 2. RefreshSent -> RefreshCompleted/Failed (when refresh response is received)
/// 3. RefreshCompleted -> CompilationStarted (if compilation begins)
/// 4. CompilationStarted -> Finish (when compilation completes)
/// 
/// Timeouts are enforced at each stage to prevent indefinite waiting.
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
#[path = "unity_refresh_task_tests.rs"]
mod tests;