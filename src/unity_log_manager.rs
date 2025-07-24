use std::collections::VecDeque;
use std::time::SystemTime;

use crate::unity_messages::LogLevel;

/// Unity log entry structure
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnityLogEntry {
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub message: String,
}

/// Maximum number of logs to keep in memory
const MAX_LOGS: usize = 10000;

/// Manages Unity logs independently from messaging client
#[derive(Clone)]
pub struct UnityLogManager {
    /// Accumulated logs - automatically managed with size limit
    /// Using VecDeque for O(1) front removal when enforcing log limits
    logs: VecDeque<UnityLogEntry>,
}

impl UnityLogManager {
    /// Create a new UnityLogManager
    pub fn new() -> Self {
        UnityLogManager {
            logs: VecDeque::with_capacity(MAX_LOGS),
        }
    }

    /// Add a log entry to the manager
    pub fn add_log(&mut self, level: LogLevel, message: String) {
        let log_entry = UnityLogEntry {
            timestamp: SystemTime::now(),
            level,
            message,
        };

        self.logs.push_back(log_entry);
        self.enforce_log_limit();
    }

    /// Add a log entry with a custom timestamp
    pub fn add_log_with_timestamp(&mut self, level: LogLevel, message: String, timestamp: SystemTime) {
        let log_entry = UnityLogEntry {
            timestamp,
            level,
            message,
        };

        self.logs.push_back(log_entry);
        self.enforce_log_limit();
    }

    /// Enforce the maximum log limit by removing old logs if necessary
    /// Using VecDeque::pop_front for O(1) removal of old logs
    fn enforce_log_limit(&mut self) {
        while self.logs.len() > MAX_LOGS {
            self.logs.pop_front();
        }
    }

    /// Get all collected logs
    /// Returns a reference to avoid cloning for read-only access
    pub fn get_logs(&self) -> &VecDeque<UnityLogEntry> {
        &self.logs
    }

    /// Get all collected logs as Vec (for compatibility)
    pub fn get_logs_vec(&self) -> Vec<UnityLogEntry> {
        self.logs.iter().cloned().collect()
    }

    /// Clear all collected logs
    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    /// Get the number of collected logs
    pub fn log_count(&self) -> usize {
        self.logs.len()
    }

    /// Get recent logs with optional filtering
    ///
    /// Searches logs in reverse order (newest first) and stops when timestamp is before `after_time`.
    /// This is optimized for cases where we typically only need recent logs.
    ///
    /// # Arguments
    /// * `after_time` - Only return logs with timestamp >= this time
    /// * `level_filter` - Optional log level filter (if None, all levels are included)
    /// * `substring_pattern` - Optional substring that must be present in the message
    ///
    /// # Returns
    /// Vector of log messages that match the criteria
    pub fn get_recent_logs(
        &self,
        after_time: SystemTime,
        level_filter: Option<LogLevel>,
        substring_pattern: Option<&str>,
    ) -> Vec<UnityLogEntry> {
        let mut matching_logs = Vec::new();

        // Search in reverse order (newest first) and stop when we hit older timestamps
        for log in self.logs.iter().rev() {
            // Stop searching if we've gone too far back in time
            if log.timestamp < after_time {
                break;
            }

            // Apply level filter if specified
            if let Some(required_level) = &level_filter {
                if log.level != *required_level {
                    continue;
                }
            }

            // Apply substring pattern filter if specified
            if let Some(pattern) = substring_pattern {
                if !log.message.contains(pattern) {
                    continue;
                }
            }

            // Log matches all criteria, add to results
            matching_logs.push(log.clone());
        }

        // Reverse to get chronological order (oldest first)
        matching_logs.reverse();
        matching_logs
    }

    /// Get logs filtered by level
    /// Optimized to avoid unnecessary cloning when possible
    pub fn get_logs_by_level(&self, level: LogLevel) -> Vec<UnityLogEntry> {
        self.logs
            .iter()
            .filter(|log| log.level == level)
            .cloned()
            .collect()
    }

    /// Get logs containing a specific substring
    /// Optimized to avoid unnecessary cloning when possible
    pub fn get_logs_containing(&self, pattern: &str) -> Vec<UnityLogEntry> {
        self.logs
            .iter()
            .filter(|log| log.message.contains(pattern))
            .cloned()
            .collect()
    }
}

impl Default for UnityLogManager {
    fn default() -> Self {
        Self::new()
    }
}