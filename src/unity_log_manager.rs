//! Unity Log Manager Module
//!
//! This module provides a robust log management system for Unity Editor integration.
//! It handles log collection, storage, and retrieval with built-in memory management
//! and intelligent deduplication to prevent log spam.
//!
//! ## Key Features
//!
//! ### Memory Management
//! - **Automatic Capacity Control**: Maintains a maximum of 10,000 log entries in memory
//!
//! ### High-Frequency Log Deduplication
//! - **Smart Filtering**: Prevents duplicate log messages within a 100ms window
//! - **Performance Optimization**: Reduces memory usage and improves log readability
//!
//! ## Why Deduplication?
//!
//! Unity applications, especially during development and debugging, often generate
//! high-frequency duplicate log messages (e.g., logs inside Update loops, physics warnings,
//! or repeated error conditions). 
//! This prevents frequent repeating log entries to occupy too much of our log capacity, allowing other logs to be stored for longer time.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime};

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

/// Deduplication window in milliseconds
const DEDUP_WINDOW_MS: u64 = 100;

/// How often to clear the deduplication cache (in seconds)
const DEDUP_CACHE_CLEAR_INTERVAL_SECS: u64 = 60;

/// Manages Unity logs independently from messaging client
///
/// This manager provides comprehensive log handling with automatic memory management
/// and intelligent deduplication. It maintains up to 10,000 log entries and prevents
/// log spam by filtering duplicate messages within a 100ms window.
/// This ensures that we will only use limited amount of memory, typicall not more than a few mega bytes.
/// Even if Unity Editor is sending us millions of logs over a few hours, we will be able to handle that effectively.
#[derive(Clone)]
pub struct UnityLogManager {
    /// Accumulated logs - automatically managed with size limit
    /// Using VecDeque for O(1) front removal when enforcing log limits
    logs: VecDeque<UnityLogEntry>,
    /// Deduplication cache: maps log content to last seen timestamp
    /// Used to prevent duplicate messages within DEDUP_WINDOW_MS timeframe
    dedup_cache: HashMap<String, SystemTime>,
    /// Last time we cleared the deduplication cache
    /// Used to periodically clean cache and prevent memory bloat
    last_cache_clear: SystemTime,
}

impl UnityLogManager {
    /// Create a new UnityLogManager
    pub fn new() -> Self {
        let now = SystemTime::now();
        UnityLogManager {
            logs: VecDeque::with_capacity(MAX_LOGS),
            dedup_cache: HashMap::new(),
            last_cache_clear: now,
        }
    }

    /// Add a log entry to the manager
    pub fn add_log(&mut self, level: LogLevel, message: String) {
        let now = SystemTime::now();
        
        // Check if we should clear the deduplication cache
        self.maybe_clear_dedup_cache(now);
        
        // Check for duplicate within the deduplication window
        if self.is_duplicate_log(&message, now) {
            // Update the timestamp for this message but don't add to logs
            self.dedup_cache.insert(message, now);
            return;
        }
        
        let log_entry = UnityLogEntry {
            timestamp: now,
            level,
            message: message.clone(),
        };

        // Add to deduplication cache
        self.dedup_cache.insert(message, now);
        
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
        // Also clear deduplication cache when clearing logs
        self.dedup_cache.clear();
        self.last_cache_clear = SystemTime::now();
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
}

impl UnityLogManager {
    /// Check if a log message is a duplicate within the deduplication window
    fn is_duplicate_log(&self, message: &str, current_time: SystemTime) -> bool {
        if let Some(&last_seen) = self.dedup_cache.get(message) {
            if let Ok(duration) = current_time.duration_since(last_seen) {
                return duration.as_millis() <= DEDUP_WINDOW_MS as u128;
            }
        }
        false
    }
    
    /// Clear the deduplication cache if enough time has passed
    fn maybe_clear_dedup_cache(&mut self, current_time: SystemTime) {
        if let Ok(duration) = current_time.duration_since(self.last_cache_clear) {
            if duration.as_secs() >= DEDUP_CACHE_CLEAR_INTERVAL_SECS {
                self.dedup_cache.clear();
                self.last_cache_clear = current_time;
            }
        }
    }
}

impl Default for UnityLogManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_high_frequency_log_deduplication() {
        let mut log_manager = UnityLogManager::new();
        let test_message = "Repeated error message".to_string();
        
        // Add the same log message multiple times in quick succession (within 100ms window)
        for _ in 0..50 {
            log_manager.add_log(LogLevel::Error, test_message.clone());
            // Sleep for a very short time (much less than 100ms)
            thread::sleep(Duration::from_millis(10));
        }
        
        // Should only have 1 log entry due to deduplication
        assert_eq!(log_manager.log_count(), 1);
        
        let logs = log_manager.get_logs_vec();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, test_message);
        assert_eq!(logs[0].level, LogLevel::Error);
    }

    #[test]
    fn test_low_frequency_log_no_deduplication() {
        let mut log_manager = UnityLogManager::new();
        let test_message = "Repeated error message".to_string();
        
        // Add the same log message with sufficient delay between them (> 100ms)
        for i in 0..3 {
            log_manager.add_log(LogLevel::Error, format!("{} {}", test_message, i));
            // Sleep for longer than the deduplication window
            thread::sleep(Duration::from_millis(200));
        }
        
        // Should have 3 log entries since they're spaced out
        assert_eq!(log_manager.log_count(), 3);
        
        let logs = log_manager.get_logs_vec();
        assert_eq!(logs.len(), 3);
        
        // Now test with the exact same message but with proper spacing
        let mut log_manager2 = UnityLogManager::new();
        
        for _ in 0..3 {
            log_manager2.add_log(LogLevel::Error, test_message.clone());
            // Sleep for longer than the deduplication window
            thread::sleep(Duration::from_millis(150));
        }
        
        // Should have 3 log entries since they're spaced out beyond the dedup window
        assert_eq!(log_manager2.log_count(), 3);
        
        let logs2 = log_manager2.get_logs_vec();
        assert_eq!(logs2.len(), 3);
        for log in &logs2 {
            assert_eq!(log.message, test_message);
            assert_eq!(log.level, LogLevel::Error);
        }
    }
}