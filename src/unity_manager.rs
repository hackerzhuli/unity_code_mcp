use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::unity_messaging_client::{
    LogLevel, UnityEvent, UnityMessagingClient, UnityMessagingError,
};
use crate::unity_project_manager::{UnityProjectError, UnityProjectManager};
use crate::{debug_log, error_log, info_log, warn_log};

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
    pub error_logs: Vec<String>,
    /// Total duration of the refresh operation in seconds
    pub duration_seconds: f64,
}

// Test-related structures are now imported from unity_messaging_client
use crate::unity_messaging_client::{TestAdaptor, TestResultAdaptor};

/// Test mode for Unity tests
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestMode {
    /// Edit mode tests (run in the editor without entering play mode)
    EditMode,
    /// Play mode tests (run in play mode)
    PlayMode,
}

impl TestMode {
    /// Convert to string representation used by Unity
    pub fn as_str(&self) -> &'static str {
        match self {
            TestMode::EditMode => "EditMode",
            TestMode::PlayMode => "PlayMode",
        }
    }
}

/// Test execution filter options
#[derive(Debug, Clone)]
pub enum TestFilter {
    /// Execute all tests in the specified mode
    All(TestMode),
    /// Execute all tests in a specific assembly
    Assembly {
        mode: TestMode,
        assembly_name: String,
    },
    /// Execute a specific test by its full name
    Specific { mode: TestMode, test_name: String },
    /// Execute tests matching a custom filter string
    Custom { mode: TestMode, filter: String },
}

impl TestFilter {
    /// Convert to the filter string format expected by Unity
    pub fn to_filter_string(&self) -> String {
        match self {
            TestFilter::All(mode) => mode.as_str().to_string(),
            TestFilter::Assembly {
                mode,
                assembly_name,
            } => {
                format!("{}:{}", mode.as_str(), assembly_name)
            }
            TestFilter::Specific { mode, test_name } => {
                format!("{}:{}", mode.as_str(), test_name)
            }
            TestFilter::Custom { mode, filter } => {
                format!("{}:{}", mode.as_str(), filter)
            }
        }
    }
}

/// Simplified test result containing only essential information
#[derive(Debug, Clone)]
pub struct SimpleTestResult {
    /// The full name of the test including namespace and class
    pub full_name: String,
    /// Stack trace information if the test failed, empty if passed
    pub stack_trace: String,
    /// Whether the test passed (true) or failed (false)
    pub passed: bool,
    /// Duration of the test execution in seconds
    pub duration: f64,
    /// Error or failure message, empty if the test passed
    pub message: String,
    /// Test output logs captured during execution
    pub output: String,
}

/// Test execution result
#[derive(Debug, Clone)]
pub struct TestExecutionResult {
    /// Simplified test results containing only essential information
    pub test_results: Vec<SimpleTestResult>,
    /// Whether the test execution completed successfully
    pub execution_completed: bool,
    /// Total number of tests that passed
    pub pass_count: u32,
    /// Total number of tests that failed
    pub fail_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnityLogEntry {
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub message: String,
}

pub struct UnityManager {
    messaging_client: Option<UnityMessagingClient>,
    project_manager: UnityProjectManager,
    logs: Arc<Mutex<VecDeque<UnityLogEntry>>>,
    seen_logs: Arc<Mutex<HashSet<String>>>,
    max_logs: usize,
    event_receiver: Option<broadcast::Receiver<UnityEvent>>,
    is_listening: bool,
    current_unity_pid: Option<u32>,
}

impl UnityManager {
    /// Create a new UnityManager for the given Unity project path
    pub async fn new(project_path: String) -> Result<Self, UnityProjectError> {
        let project_manager = UnityProjectManager::new(project_path).await?;

        Ok(UnityManager {
            messaging_client: None,
            project_manager,
            logs: Arc::new(Mutex::new(VecDeque::new())),
            seen_logs: Arc::new(Mutex::new(HashSet::new())),
            max_logs: 1000, // Keep last 1000 log entries
            event_receiver: None,
            is_listening: false,
            current_unity_pid: None,
        })
    }

    /// Initialize the messaging client if Unity is running
    pub async fn initialize_messaging(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Try to connect to Unity if it's running
        self.try_connect_to_unity().await?;
        
        Ok(())
    }

    /// Try to connect to Unity if it's running
    async fn try_connect_to_unity(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Update process info to check if Unity is running
        self.project_manager.update_process_info().await?;
        
        if let Some(unity_pid) = self.project_manager.unity_process_id() {
            // Check if we're already connected to this PID
            if self.current_unity_pid == Some(unity_pid) && self.messaging_client.is_some() {
                return Ok(()); // Already connected to this Unity instance
            }
            
            // Clean up existing connection if any
            self.cleanup_messaging_client().await;
            
            // Create new connection
            match UnityMessagingClient::new(unity_pid).await {
                Ok(mut client) => {
                    // Subscribe to events before starting listener
                    let event_receiver = client.subscribe_to_events();
                    
                    // Start listening for Unity events
                    client.start_listening().await?;
                    self.is_listening = true;
                    
                    self.messaging_client = Some(client);
                    self.current_unity_pid = Some(unity_pid);
                    
                    // Start the log collection task with the event receiver
                    self.start_log_collection_with_receiver(event_receiver).await;
                    
                    info_log!("Connected to Unity Editor (PID: {})", unity_pid);
                    Ok(())
                }
                Err(e) => {
                    warn_log!("Failed to connect to Unity Editor (PID: {}): {}", unity_pid, e);
                    Err(e.into())
                }
            }
        } else {
            // No Unity process found, clean up if we had a connection
            if self.messaging_client.is_some() {
                info_log!("Unity Editor is no longer running, cleaning up connection");
                self.cleanup_messaging_client().await;
            }
            Err("Unity Editor is not running".into())
        }
    }
    
    /// Clean up the messaging client and related resources
    async fn cleanup_messaging_client(&mut self) {
        if let Some(client) = self.messaging_client.take() {
            // Stop listening
            self.is_listening = false;
            
            // The client will be dropped here, which should clean up resources
            drop(client);
        }
        
        self.current_unity_pid = None;
        self.event_receiver = None;
        
        debug_log!("Messaging client cleaned up");
    }
    
    /// Check and update Unity connection status
    /// This is important because Unity Editor could shutdown or start after this is initialized
    pub async fn update_unity_connection(&mut self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Update process info
        self.project_manager.update_process_info().await?;
        let current_pid = self.project_manager.unity_process_id();
        
        // Check if Unity process changed
        if current_pid != self.current_unity_pid {
            if current_pid.is_some() {
                // Unity started or changed, try to connect
                match self.try_connect_to_unity().await {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            } else {
                // Unity stopped, clean up
                self.cleanup_messaging_client().await;
                Ok(false)
            }
        } else {
            // No process change, return current connection status
            Ok(self.messaging_client.is_some())
        }
    }
    
    /// Start collecting logs from Unity events with a specific receiver
    async fn start_log_collection_with_receiver(
        &mut self,
        mut event_receiver: broadcast::Receiver<UnityEvent>,
    ) {
        let logs = Arc::clone(&self.logs);
        let seen_logs = Arc::clone(&self.seen_logs);
        let max_logs = self.max_logs;

        tokio::spawn(async move {
            //println!("[DEBUG] Log collection task started");
            loop {
                match event_receiver.recv().await {
                    Ok(event) => {
                        //println!("[DEBUG] Log collection received event: {:?}", event);
                        match event {
                            UnityEvent::LogMessage { level, message } => {
                                // Create a unique key for deduplication (message only)
                                let log_key = message.clone();

                                // Check if we've already seen this exact log
                                let is_duplicate = if let Ok(mut seen_guard) = seen_logs.lock() {
                                    !seen_guard.insert(log_key)
                                } else {
                                    false
                                };

                                if !is_duplicate {
                                    let log_entry = UnityLogEntry {
                                        timestamp: SystemTime::now(),
                                        level: level.clone(),
                                        message: message.clone(),
                                    };

                                    debug_log!(
                                        "Adding log entry: [{:?}] {}",
                                        log_entry.level,
                                        log_entry.message
                                    );

                                    if let Ok(mut logs_guard) = logs.lock() {
                                        logs_guard.push_back(log_entry);

                                        // Keep only the last max_logs entries
                                        while logs_guard.len() > max_logs {
                                            logs_guard.pop_front();
                                        }

                                        //println!("[DEBUG] Total logs now: {}", logs_guard.len());
                                    }
                                } else {
                                    //println!("[DEBUG] Skipping duplicate log: [{:?}] {}", level, message);
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn_log!("Log collection lagged, skipped {} messages", skipped);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug_log!("Log collection channel closed, exiting task");
                        break;
                    }
                }
            }
        });
    }

    /// Get all collected logs
    pub fn get_logs(&self) -> Vec<UnityLogEntry> {
        if let Ok(logs_guard) = self.logs.lock() {
            logs_guard.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Clear all collected logs
    pub fn clear_logs(&self) {
        if let Ok(mut logs_guard) = self.logs.lock() {
            logs_guard.clear();
        }
        if let Ok(mut seen_guard) = self.seen_logs.lock() {
            seen_guard.clear();
        }
    }

    /// Get the number of collected logs
    pub fn log_count(&self) -> usize {
        if let Ok(logs_guard) = self.logs.lock() {
            logs_guard.len()
        } else {
            0
        }
    }

    /// Check if Unity is currently online
    pub fn is_unity_online(&self) -> bool {
        self.messaging_client
            .as_ref()
            .map(|client| client.is_online())
            .unwrap_or(false)
    }

    /// Check if Unity is currently connected and responsive
    ///
    /// # Arguments
    ///
    /// * `timeout_seconds` - Maximum age of last response to consider Unity connected (default: 10 seconds)
    ///
    /// # Returns
    ///
    /// Returns `true` if Unity has responded within the timeout period, `false` otherwise
    pub fn is_unity_connected(&self, timeout_seconds: Option<u64>) -> bool {
        self.messaging_client
            .as_ref()
            .map(|client| client.is_connected(timeout_seconds))
            .unwrap_or(false)
    }

    /// Get Unity package version (not Unity Editor version)
    pub async fn get_unity_package_version(
        &mut self,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();

            // Send the version request
            client
                .get_version()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            // Wait for the PackageVersion event response
            let timeout_duration = Duration::from_secs(10);

            match timeout(timeout_duration, async {
                loop {
                    match event_receiver.recv().await {
                        Ok(UnityEvent::PackageVersion(version)) => return Ok(version),
                        Ok(_) => continue,
                        Err(_) => return Err("Event channel closed".into()),
                    }
                }
            })
            .await
            {
                Ok(result) => result,
                Err(_) => Err("Timeout waiting for Unity version response".into()),
            }
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Get Unity project path
    pub async fn get_project_path(
        &mut self,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();

            // Send the project path request
            client
                .get_project_path()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            // Wait for the ProjectPath event response
            let timeout_duration = Duration::from_secs(10);

            match timeout(timeout_duration, async {
                loop {
                    match event_receiver.recv().await {
                        Ok(UnityEvent::ProjectPath(path)) => return Ok(path),
                        Ok(_) => continue,
                        Err(_) => return Err("Event channel closed".into()),
                    }
                }
            })
            .await
            {
                Ok(result) => result,
                Err(_) => Err("Timeout waiting for Unity project path response".into()),
            }
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Wait for Unity to become online
    ///
    /// # Arguments
    ///
    /// * `timeout_seconds` - Maximum time to wait for Unity to become online
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if Unity becomes online within the timeout period, `Err` otherwise
    pub async fn wait_online(
        &mut self,
        timeout_seconds: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            for _ in 0..timeout_seconds * 10 {
                if client.is_online() {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err("Timeout waiting for Unity to become online".into())
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Send refresh message to Unity (this is the simple variant that just sends the message)
    pub async fn refresh_unity(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            client
                .send_refresh_message(Some(60))
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Execute tests based on the specified filter
    ///
    /// # Arguments
    ///
    /// * `filter` - The test filter specifying which tests to execute
    ///
    /// # Returns
    ///
    /// Returns a TestExecutionResult containing information about the test execution
    pub async fn run_tests(
        &mut self,
        filter: TestFilter,
    ) -> Result<TestExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();

            // Send the test execution request directly - TestStarted events will provide the mapping
            debug_log!(
                "Sending test filter to Unity: '{}'",
                filter.to_filter_string()
            );
            client
                .execute_tests(filter)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            // Collect test execution events
            let mut result = TestExecutionResult {
                test_results: Vec::new(),
                execution_completed: false,
                pass_count: 0,
                fail_count: 0,
            };

            // Temporary mapping from TestId to test full name for building SimpleTestResult
            let mut test_id_to_name: HashMap<String, String> = HashMap::new();

            let timeout_duration = Duration::from_secs(300); // 5 minutes for test execution
            let start_time = std::time::Instant::now();

            // Wait for test execution to complete
            while start_time.elapsed() < timeout_duration {
                match timeout(Duration::from_secs(10), event_receiver.recv()).await {
                    Ok(Ok(event)) => {
                        debug_log!("Received event: {:?}", std::mem::discriminant(&event));
                        match event {
                            UnityEvent::TestRunStarted(container) => {
                                debug_log!(
                                    "Test run started with {} test adaptors",
                                    container.test_adaptors.len()
                                );

                                // Build test ID to name mapping from test run started data
                                for adaptor in &container.test_adaptors {
                                    debug_log!(
                                        "TestRunStarted adaptor - Id: '{}', Name: '{}', FullName: '{}', Type: {:?}",
                                        adaptor.id,
                                        adaptor.name,
                                        adaptor.full_name,
                                        adaptor.test_type
                                    );
                                    test_id_to_name
                                        .insert(adaptor.id.clone(), adaptor.full_name.clone());
                                }
                            }
                            UnityEvent::TestStarted(container) => {
                                debug_log!(
                                    "Test started with {} test adaptors",
                                    container.test_adaptors.len()
                                );

                                // Build test ID to name mapping from parsed data
                                for adaptor in &container.test_adaptors {
                                    debug_log!(
                                        "TestStarted adaptor - Id: '{}', Name: '{}', FullName: '{}', Type: {:?}",
                                        adaptor.id,
                                        adaptor.name,
                                        adaptor.full_name,
                                        adaptor.test_type
                                    );
                                    test_id_to_name
                                        .insert(adaptor.id.clone(), adaptor.full_name.clone());
                                }
                                debug_log!(
                                    "Total mappings in test_id_to_name: {}",
                                    test_id_to_name.len()
                                );
                            }
                            UnityEvent::TestFinished(container) => {
                                debug_log!(
                                    "Test finished with {} test result adaptors",
                                    container.test_result_adaptors.len()
                                );

                                // Extract individual test results from parsed data
                                for adaptor in &container.test_result_adaptors {
                                    debug_log!(
                                        "TestFinished adaptor - TestId: '{}', PassCount: {}, FailCount: {}, ResultState: '{}'",
                                        adaptor.test_id,
                                        adaptor.pass_count,
                                        adaptor.fail_count,
                                        adaptor.result_state
                                    );

                                    // Create SimpleTestResult from TestResultAdaptor
                                    let full_name = test_id_to_name
                                        .get(&adaptor.test_id)
                                        .cloned()
                                        .unwrap_or_else(|| {
                                            format!("Unknown test (ID: {})", adaptor.test_id)
                                        });

                                    let simple_result = SimpleTestResult {
                                        full_name,
                                        stack_trace: adaptor.stack_trace.clone(),
                                        passed: adaptor.result_state == "Passed",
                                        duration: adaptor.duration,
                                        message: adaptor.message.clone(),
                                        output: adaptor.output.clone(),
                                    };

                                    result.test_results.push(simple_result);
                                }
                            }
                            UnityEvent::TestRunFinished(container) => {
                                debug_log!("Test run finished");

                                // Only process the first TestRunFinished event to avoid accumulation
                                if !result.execution_completed {
                                    result.execution_completed = true;

                                    // Extract pass/fail counts from parsed data
                                    if let Some(adaptor) = container.test_result_adaptors.first() {
                                        result.pass_count = adaptor.pass_count;
                                        result.fail_count = adaptor.fail_count;
                                        debug_log!(
                                            "Extracted counts from TestRunFinished: {} passed, {} failed",
                                            adaptor.pass_count,
                                            adaptor.fail_count
                                        );
                                    } else {
                                        debug_log!("No test result adaptors in TestRunFinished");
                                    }
                                }
                                break;
                            }
                            UnityEvent::TestListRetrieved(_test_list) => {
                                // Test list is no longer stored in TestExecutionResult
                            }
                            _ => {
                                // Ignore other events during test execution
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        return Err("Event channel closed during test execution".into());
                    }
                    Err(_) => {
                        // Timeout on individual event - check if we should continue waiting
                        if result.execution_completed {
                            break;
                        }
                        // Continue waiting if tests are still running
                    }
                }
            }

            if !result.execution_completed {
                return Err("Timeout waiting for test execution to complete".into());
            }

            Ok(result)
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Send refresh message and collect error logs during compilation
    ///
    /// This method sends a refresh message to Unity, waits for a refresh response,
    /// then waits for compilation events while collecting all error logs received
    /// after the initial refresh message was sent.
    ///
    /// # Returns
    ///
    /// Returns a RefreshResult containing comprehensive information about the refresh operation
    pub async fn refresh(
        &mut self,
    ) -> Result<RefreshResult, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();

            let operation_start = std::time::Instant::now();

            // Send the refresh message, allow 60 seconds to send
            client
                .send_refresh_message(Some(60))
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            //println!("[DEBUG] Refresh message sent");

            // refresh start should be when we actually send the message
            let refresh_start_time = SystemTime::now();

            // Track refresh process state
            let mut refresh_response_received = false;
            let mut refresh_error_message: Option<String> = None;
            let mut compilation_started = false;
            let mut compilation_finished = false;

            let timeout_duration = Duration::from_secs(60); // 1 minute total timeout
            let start_time = std::time::Instant::now();

            // Wait for refresh response first
            while start_time.elapsed() < timeout_duration && !refresh_response_received {
                match timeout(Duration::from_secs(10), event_receiver.recv()).await {
                    Ok(Ok(event)) => {
                        match event {
                            UnityEvent::RefreshCompleted(message) => {
                                debug_log!("Refresh completed with message: '{}'", message);
                                refresh_response_received = true;

                                // Check if refresh failed (non-empty message indicates error)
                                if !message.is_empty() {
                                    refresh_error_message = Some(message.clone());
                                    error_log!("Refresh failed: {}", message);

                                    // Return early with error result
                                    let duration = operation_start.elapsed().as_secs_f64();
                                    return Ok(RefreshResult {
                                        refresh_completed: false,
                                        refresh_error_message: Some(message),
                                        compilation_started: false,
                                        compilation_completed: false,
                                        error_logs: Vec::new(),
                                        duration_seconds: duration,
                                    });
                                }
                            }
                            _ => {
                                // Ignore other events while waiting for refresh response
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        return Err("Event channel closed during refresh".into());
                    }
                    Err(_) => {
                        // Timeout on individual event - continue waiting
                    }
                }
            }

            if !refresh_response_received {
                let duration = operation_start.elapsed().as_secs_f64();
                return Ok(RefreshResult {
                    refresh_completed: false,
                    refresh_error_message: Some("Timeout waiting for refresh response".to_string()),
                    compilation_started: false,
                    compilation_completed: false,
                    error_logs: Vec::new(),
                    duration_seconds: duration,
                });
            }

            // Wait for compilation started event for 3 seconds
            let compilation_wait_start = std::time::Instant::now();
            while compilation_wait_start.elapsed() < Duration::from_secs(3) && !compilation_started
            {
                match timeout(Duration::from_millis(100), event_receiver.recv()).await {
                    Ok(Ok(event)) => {
                        match event {
                            UnityEvent::CompilationStarted => {
                                debug_log!("Compilation started");
                                compilation_started = true;
                            }
                            _ => {
                                // Ignore other events
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        return Err("Event channel closed during compilation wait".into());
                    }
                    Err(_) => {
                        // Timeout on individual event - continue waiting
                    }
                }
            }

            // If compilation started, wait for compilation finished for 60 seconds
            if compilation_started {
                while start_time.elapsed() < timeout_duration && !compilation_finished {
                    match timeout(Duration::from_secs(60), event_receiver.recv()).await {
                        Ok(Ok(event)) => {
                            match event {
                                UnityEvent::CompilationFinished => {
                                    debug_log!("Compilation finished");
                                    compilation_finished = true;
                                }
                                _ => {
                                    // Ignore other events
                                }
                            }
                        }
                        Ok(Err(_)) => {
                            return Err("Event channel closed during compilation".into());
                        }
                        Err(_) => {
                            // Timeout on individual event - continue waiting
                        }
                    }
                }

                if !compilation_finished {
                    let duration = operation_start.elapsed().as_secs_f64();
                    return Ok(RefreshResult {
                        refresh_completed: false,
                        refresh_error_message: Some(
                            "Timeout waiting for compilation to finish after 60 seconds"
                                .to_string(),
                        ),
                        compilation_started: true,
                        compilation_completed: false,
                        error_logs: Vec::new(),
                        duration_seconds: duration,
                    });
                }

                // Wait additional time for error logs to arrive after compilation finishes
                debug_log!("Waiting 2 seconds for error logs after compilation finished");
                tokio::time::sleep(Duration::from_secs(2)).await;
            } else {
                debug_log!("No compilation started within 5 seconds");
            }

            // Filter error logs from the existing log collection based on timestamp
            let error_logs: Vec<String> = self
                .get_logs()
                .into_iter()
                .filter(|log| log.level == LogLevel::Error && log.timestamp >= refresh_start_time)
                .map(|log| log.message)
                .collect();

            let duration = operation_start.elapsed().as_secs_f64();
            debug_log!(
                "Refresh completed, collected {} error logs in {:.2} seconds",
                error_logs.len(),
                duration
            );

            Ok(RefreshResult {
                refresh_completed: compilation_finished,
                refresh_error_message: None,
                compilation_started: compilation_started,
                compilation_completed: compilation_finished,
                error_logs,
                duration_seconds: duration,
            })
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Stop the messaging client and cleanup
    pub async fn shutdown(&mut self) {
        // Clean up messaging client
        self.cleanup_messaging_client().await;

        // Clear all logs and seen logs to prevent state leakage between test runs
        self.clear_logs();

        // Add a small delay to ensure Unity has time to fully shut down
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

impl Drop for UnityManager {
    fn drop(&mut self) {
        // Clean up messaging client synchronously
        if let Some(client) = self.messaging_client.take() {
            drop(client);
        }
        
        self.current_unity_pid = None;
        self.event_receiver = None;
        self.is_listening = false;
    }
}
