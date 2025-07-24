use chrono;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::unity_log_manager::{UnityLogEntry, UnityLogManager};
use crate::unity_log_utils::{self, extract_main_message};
use crate::unity_messaging_client::UnityMessagingClient;
use crate::unity_project_manager::{UnityProjectError, UnityProjectManager};
use crate::{debug_log, error_log, info_log, warn_log};

// Timing constants for refresh operations

/// Total timeout in seconds for the refresh operation(after sent, not including compilation)
const REFRESH_TOTAL_TIMEOUT_SECS: u64 = 60;

/// Timeout in milliseconds to wait for compilation to start after refresh finised
const COMPILATION_WAIT_TIMEOUT_MILLIS: u64 = 1000;

/// Timeout in seconds for compilation to finish
const COMPILATION_FINISH_TIMEOUT_SECS: u64 = 60;

/// Wait time in seconds after compilation finishes for logs to arrive
const POST_COMPILATION_WAIT_SECS: u64 = 1;

// Timing constants for test execution

/// Timeout in seconds for an individual test to complete after it starts
///
/// Note: The timeout is different in debug and release build for easy testing with a debug build
const TEST_TIMEOUT_SECS: u64 = if cfg!(debug_assertions) { 30 } else { 180 };

/// Timeout in seconds to wait for the next test to start
const TEST_START_TIMEOUT_SECS: u64 = 3;

/// Timeout in seconds to wait for TestRunStarted event, this need to be a little bit longer, because Unity Editor can be busy(e.g. importing assets, something that we don't track, we track compilation, running tests, Play Mode, but there can be operation that we don't track happening)
const TEST_RUN_START_TIMEOUT_SECS: u64 = 30;

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

// Test-related structures are now imported from unity_messaging_client
use crate::unity_messages::{LogLevel, TestAdaptor, TestFilter, UnityEvent};

/// tracking running state of individual tests
#[derive(Debug, Clone)]
struct TestState {
    start_time: std::time::Instant,
    finish_time: Option<std::time::Instant>,
    adapater: TestAdaptor,
}

/// Simplified test result containing only essential information
#[derive(Debug, Clone)]
pub struct SimpleTestResult {
    /// The full name of the test including namespace and class
    pub full_name: String,
    /// Stack trace information if the test failed, empty if passed
    pub error_stack_trace: String,
    /// Whether the test passed (true) or failed (false)
    pub passed: bool,
    /// Duration of the test execution in seconds
    pub duration_seconds: f64,
    /// Error or failure message, empty if the test passed
    pub error_message: String,
    /// Test output logs captured during execution
    pub output_logs: String,
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
    /// Total duration of the test execution in seconds
    pub duration_seconds: f64,
    /// Total number of tests
    pub test_count: u32,
    /// Total number of tests that skipped
    pub skip_count: u32,
    /// Additional error message, empty if no additional error occurred
    pub error_message: String,
}

pub struct UnityManager {
    messaging_client: Option<UnityMessagingClient>,
    project_manager: UnityProjectManager,
    /// Log manager for handling Unity logs
    log_manager: Arc<Mutex<UnityLogManager>>,
    current_unity_pid: Option<u32>,
    /// ID of the current test run in Unity Editor
    current_test_run_id: Arc<Mutex<Option<String>>>,
    /// Whether Unity Editor is currently in play mode
    is_in_play_mode: Arc<Mutex<bool>>,
    /// Timestamp of the last compilation finished event
    last_compilation_finished: Arc<Mutex<Option<SystemTime>>>,
    /// Compile errors collected after compilation finishes
    last_compile_errors: Arc<Mutex<Vec<String>>>,
}

const MESSAGING_CLIENT_NOT_INIT_ERROR: &'static str = "Messaging client not initialized, this is likely because Unity Editor is not running for this project.";

impl UnityManager {
    /// Create a new UnityManager for the given Unity project path
    pub async fn new(project_path: String) -> Result<Self, UnityProjectError> {
        let project_manager = UnityProjectManager::new(project_path).await?;

        Ok(UnityManager {
            messaging_client: None,
            project_manager,
            log_manager: Arc::new(Mutex::new(UnityLogManager::new())),
            current_unity_pid: None,
            current_test_run_id: Arc::new(Mutex::new(None)),
            is_in_play_mode: Arc::new(Mutex::new(false)),
            last_compilation_finished: Arc::new(Mutex::new(None)),
            last_compile_errors: Arc::new(Mutex::new(Vec::new())),
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
    async fn try_connect_to_unity(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

                    self.messaging_client = Some(client);
                    self.current_unity_pid = Some(unity_pid);

                    // Start the log collection task with the event receiver
                    self.background_event_handling(event_receiver).await;

                    info_log!("Connected to Unity Editor (PID: {})", unity_pid);
                    Ok(())
                }
                Err(e) => {
                    warn_log!(
                        "Failed to connect to Unity Editor (PID: {}): {}",
                        unity_pid,
                        e
                    );
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

    /// Reset the editor state to clean state
    fn reset_editor_state(&mut self) {
        if let Ok(mut log_manager) = self.log_manager.lock() {
            log_manager.clear_logs();
        }

        if let Ok(mut test_run_guard) = self.current_test_run_id.lock() {
            *test_run_guard = None;
        }
        if let Ok(mut play_mode_guard) = self.is_in_play_mode.lock() {
            *play_mode_guard = false;
        }
        if let Ok(mut compilation_guard) = self.last_compilation_finished.lock() {
            *compilation_guard = None;
        }
        if let Ok(mut compile_errors_guard) = self.last_compile_errors.lock() {
            compile_errors_guard.clear();
        }
    }

    /// Clean up the messaging client and related resources
    async fn cleanup_messaging_client(&mut self) {
        if let Some(client) = self.messaging_client.take() {
            // The client will be dropped here, which should clean up resources
            drop(client);
        }

        self.current_unity_pid = None;

        debug_log!("Messaging client cleaned up");
    }

    /// Check and update Unity connection status
    /// This is important because Unity Editor could shutdown or start after this is initialized
    pub async fn update_unity_connection(
        &mut self,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        debug_log!("Updating Unity connection status");

        // Update process info
        // ignore error here, we just want to extract process id later
        // it will error if unity editor is not running
        let _ = self.project_manager.update_process_info().await;
        let current_pid = self.project_manager.unity_process_id();

        // Check if Unity process changed
        if current_pid != self.current_unity_pid {
            self.reset_editor_state();
            if current_pid.is_some() {
                // Unity started or changed, try to connect
                match self.try_connect_to_unity().await {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            } else {
                // Unity stopped, clean up
                debug_log!("Unity Editor is no longer running, cleaning up connection");
                self.cleanup_messaging_client().await;
                Ok(false)
            }
        } else {
            // No process change, return current connection status
            Ok(self.messaging_client.is_some())
        }
    }

    /// Start background event handling logic
    async fn background_event_handling(
        &mut self,
        mut event_receiver: broadcast::Receiver<UnityEvent>,
    ) {
        let log_manager = self.log_manager.clone();
        let current_test_run_id = Arc::clone(&self.current_test_run_id);
        let is_in_play_mode = Arc::clone(&self.is_in_play_mode);
        let last_compilation_finished = Arc::clone(&self.last_compilation_finished);
        let last_compile_errors = Arc::clone(&self.last_compile_errors);

        tokio::spawn(async move {
            //println!("[DEBUG] Log collection task started");
            loop {
                match event_receiver.recv().await {
                    Ok(event) => {
                        //println!("[DEBUG] Log collection received event: {:?}", event);
                        match event {
                            UnityEvent::LogMessage { level, message } => {
                                // only errors and warnings because we never use info logs anyway
                                if level == LogLevel::Error || level == LogLevel::Warning {
                                    if let Ok(mut log_manager_guard) = log_manager.lock() {
                                        log_manager_guard.add_log_with_timestamp(
                                            level.clone(),
                                            message.clone(),
                                            SystemTime::now(),
                                        );
                                    }
                                }
                            }
                            UnityEvent::CompilationStarted => {
                                // Clear logs when compilation starts to prevent memory growth
                                if let Ok(mut log_manager_guard) = log_manager.lock() {
                                    log_manager_guard.clear_logs();
                                }
                                // Clear previous compile errors
                                if let Ok(mut compile_errors_guard) = last_compile_errors.lock() {
                                    compile_errors_guard.clear();
                                }
                            }
                            UnityEvent::CompilationFinished => {
                                // Record compilation finished timestamp
                                if let Ok(mut compilation_guard) = last_compilation_finished.lock()
                                {
                                    *compilation_guard = Some(SystemTime::now());
                                }

                                // Collect compile errors from logs
                                if let Ok(log_manager_guard) = log_manager.lock() {
                                    if let Ok(mut compile_errors_guard) = last_compile_errors.lock()
                                    {
                                        let logs = log_manager_guard.get_logs();
                                        for log_entry in logs {
                                            if log_entry.level == LogLevel::Error {
                                                let main_message =
                                                    extract_main_message(&log_entry.message);
                                                compile_errors_guard.push(main_message);
                                            }
                                        }
                                    }
                                }

                                debug_log!("Compilation finished");
                            }
                            UnityEvent::TestRunStarted(container) => {
                                // Track the root test run ID
                                if let Some(first_adaptor) = container.test_adaptors.first() {
                                    if let Ok(mut test_run_guard) = current_test_run_id.lock() {
                                        *test_run_guard = Some(first_adaptor.id.clone());
                                        debug_log!(
                                            "Test run started with root ID: {}",
                                            first_adaptor.id
                                        );
                                    }
                                }
                            }
                            UnityEvent::TestRunFinished(_) => {
                                // Clear the test run ID when test run finishes
                                if let Ok(mut test_run_guard) = current_test_run_id.lock() {
                                    if let Some(test_id) = test_run_guard.take() {
                                        debug_log!("Test run finished for ID: {}", test_id);
                                    }
                                }
                            }
                            UnityEvent::IsPlaying(playing) => {
                                // Track Unity's play mode state
                                if let Ok(mut play_mode_guard) = is_in_play_mode.lock() {
                                    *play_mode_guard = playing;
                                    debug_log!(
                                        "Unity play mode changed: {}",
                                        if playing { "Playing" } else { "Stopped" }
                                    );
                                }

                                if playing {
                                    // Clear logs when play mode starts to prevent memory growth (similar to Unity Editor's behaviour)
                                    if let Ok(mut log_manager_guard) = log_manager.lock() {
                                        log_manager_guard.clear_logs();
                                    }
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
        if let Ok(log_manager_guard) = self.log_manager.lock() {
            log_manager_guard.get_logs()
        } else {
            Vec::new()
        }
    }

    /// Clear all collected logs
    pub fn clear_logs(&self) {
        if let Ok(mut log_manager_guard) = self.log_manager.lock() {
            log_manager_guard.clear_logs();
        }
    }

    /// Get the number of collected logs
    pub fn log_count(&self) -> usize {
        if let Ok(log_manager_guard) = self.log_manager.lock() {
            log_manager_guard.log_count()
        } else {
            0
        }
    }

    /// Get the timestamp of the last compilation finished event
    pub fn get_last_compilation_finished(&self) -> Option<SystemTime> {
        if let Ok(compilation_guard) = self.last_compilation_finished.lock() {
            *compilation_guard
        } else {
            None
        }
    }

    /// Get the compile errors from the last compilation
    pub fn get_last_compile_errors(&self) -> Vec<String> {
        if let Ok(compile_errors_guard) = self.last_compile_errors.lock() {
            compile_errors_guard.clone()
        } else {
            Vec::new()
        }
    }

    /// Check if Unity is currently online
    pub fn is_unity_online(&self) -> bool {
        self.messaging_client
            .as_ref()
            .map(|client| client.is_online())
            .unwrap_or(false)
    }

    /// Check if Unity is currently running tests
    ///
    /// # Returns
    ///
    /// Returns `true` if Unity is currently executing a test run, `false` otherwise
    pub fn is_unity_running_tests(&self) -> bool {
        if let Ok(test_run_guard) = self.current_test_run_id.lock() {
            test_run_guard.is_some()
        } else {
            false
        }
    }

    /// Check if Unity is currently in Play mode
    ///
    /// # Returns
    ///
    /// Returns `true` if Unity is currently in Play mode, `false` otherwise
    pub fn is_unity_in_play_mode(&self) -> bool {
        if let Ok(play_mode_guard) = self.is_in_play_mode.lock() {
            *play_mode_guard
        } else {
            false
        }
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
            Err(MESSAGING_CLIENT_NOT_INIT_ERROR.into())
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
            Err(MESSAGING_CLIENT_NOT_INIT_ERROR.into())
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
            Err(MESSAGING_CLIENT_NOT_INIT_ERROR.into())
        }
    }

    /// Send refresh message to Unity (this is the simple variant that just sends the message)
    pub async fn refresh_unity(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            client
                .send_refresh_message(None)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        } else {
            Err(MESSAGING_CLIENT_NOT_INIT_ERROR.into())
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
        // Check if Unity is currently running tests to avoid conflicts
        if self.is_unity_running_tests() {
            return Err("Cannot start a new test run because Unity Editor is still running tests, please try again later.".into());
        }

        // Check if Unity is currently in Play mode to avoid conflicts
        if self.is_unity_in_play_mode() {
            return Err("Cannot start a new test run because Unity Editor is in Play mode, please stop Play mode and try again.".into());
        }

        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();

            // Send the test execution request directly - TestStarted events will provide the mapping
            debug_log!(
                "Sending test filter to Unity: '{}'",
                filter.to_filter_string()
            );

            client
                .execute_tests(filter, None)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            let execute_test_message_sent_time = Instant::now();

            // Temporary mapping from TestId to test full name for building SimpleTestResult
            let mut test_states: HashMap<String, TestState> = HashMap::new();

            let mut test_results: Vec<SimpleTestResult> = Vec::new();

            let mut run_start_time: Option<Instant> = None;

            let mut root_test_adaptor: Option<TestAdaptor> = None;

            // Wait for test execution to complete
            loop {
                match timeout(Duration::from_secs(3), event_receiver.recv()).await {
                    Ok(Ok(event)) => {
                        debug_log!("Received event: {:?}", std::mem::discriminant(&event));
                        match event {
                            UnityEvent::TestRunStarted(container) => {
                                debug_log!(
                                    "Test run started with {} test adaptors",
                                    container.test_adaptors.len()
                                );
                                run_start_time = Some(Instant::now());
                                root_test_adaptor = Some(container.test_adaptors[0].clone());
                            }
                            UnityEvent::TestStarted(container) => {
                                debug_log!(
                                    "Test started with {} test adaptors",
                                    container.test_adaptors.len()
                                );

                                // Build test ID to adaptor mapping from parsed data
                                for adaptor in &container.test_adaptors {
                                    debug_log!(
                                        "TestStarted adaptor - Id: '{}', Name: '{}', FullName: '{}', Type: {:?}",
                                        adaptor.id,
                                        adaptor.name,
                                        adaptor.full_name,
                                        adaptor.test_type
                                    );
                                    test_states.insert(
                                        adaptor.id.clone(),
                                        TestState {
                                            start_time: std::time::Instant::now(),
                                            finish_time: None,
                                            adapater: adaptor.clone(),
                                        },
                                    );
                                }
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

                                    // record finish time
                                    if let Some(test_state) = test_states.get_mut(&adaptor.test_id)
                                    {
                                        test_state.finish_time = Some(std::time::Instant::now());
                                    }

                                    // only add tests that don't have children
                                    if adaptor.has_children {
                                        continue;
                                    }

                                    // Create SimpleTestResult from TestResultAdaptor
                                    if let Some(test_state) = test_states.get(&adaptor.test_id) {
                                        let simple_result = SimpleTestResult {
                                            full_name: test_state.adapater.full_name.clone(),
                                            error_stack_trace: adaptor.stack_trace.clone(),
                                            passed: adaptor.result_state == "Passed",
                                            duration_seconds: adaptor.duration,
                                            error_message: adaptor.message.clone(),
                                            output_logs: adaptor.output.clone(),
                                        };

                                        test_results.push(simple_result);
                                    }
                                }
                            }
                            UnityEvent::TestRunFinished(container) => {
                                debug_log!("Test run finished");

                                // Extract pass/fail counts from parsed data
                                if let Some(adaptor) = container.test_result_adaptors.first() {
                                    let result = TestExecutionResult {
                                        pass_count: adaptor.pass_count,
                                        fail_count: adaptor.fail_count,
                                        skip_count: adaptor.skip_count,
                                        duration_seconds: adaptor.duration,
                                        test_count: adaptor.pass_count
                                            + adaptor.fail_count
                                            + adaptor.skip_count,
                                        test_results,
                                        execution_completed: true,
                                        error_message: "".into(),
                                    };

                                    debug_log!(
                                        "Extracted counts from TestRunFinished: {} passed, {} failed",
                                        adaptor.pass_count,
                                        adaptor.fail_count
                                    );

                                    return Ok(result);
                                } else {
                                    debug_log!("No test result adaptors in TestRunFinished");
                                }
                                break;
                            }
                            _ => {
                                // Ignore other events during test execution
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        return Ok(self.create_test_result_with_error(
                            root_test_adaptor,
                            test_results,
                            test_states,
                            "Event channel closed during test execution. Hint: Unity Editor process shuts down unexpectedly, it could have crashed or been killed by user.",
                        ));
                    }
                    Err(_) => {
                        if run_start_time.is_none()
                            && execute_test_message_sent_time.elapsed()
                                >= Duration::from_secs(TEST_RUN_START_TIMEOUT_SECS)
                        {
                            return Err(format!("Test run didn't start within {} seconds. Hint: Unity Editor is busy and can't respond now, please try again later.", TEST_RUN_START_TIMEOUT_SECS).into());
                        }

                        // check for time out in tests
                        if let Err(e) = self.check_test_timeout(&mut test_states, run_start_time) {
                            // Return results with error message instead of just error
                            return Ok(self.create_test_result_with_error(
                                root_test_adaptor,
                                test_results,
                                test_states,
                                e.as_str(),
                            ));
                        }

                        // since this is when a mini time out happened
                        // this is the best time to check whether Unity Editor is still running
                        // to prevent keep waiting for Unity to respond for too long if Unity shuts down unexpectedly
                        // since we have a very generous timeout for tests
                        // A long timeout is important because we don't want to limit how long one test can take, one test can take minutes, it depends on the project
                        self.update_unity_connection().await;
                    }
                }
            }

            // this should not occur
            Ok(self.create_test_result_with_error(
                root_test_adaptor,
                test_results,
                test_states,
                "Some internal error occured during test execution",
            ))
        } else {
            Err(MESSAGING_CLIENT_NOT_INIT_ERROR.into())
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
    pub async fn refresh_asset_database(
        &mut self,
    ) -> Result<RefreshResult, Box<dyn std::error::Error + Send + Sync>> {
        // Check if Unity is currently running tests to avoid conflicts
        if self.is_unity_running_tests() {
            return Err("Cannot refresh asset database because Unity Editor is still running tests, please try again later.".into());
        }

        // Check if Unity is currently in Play mode to avoid conflicts
        if self.is_unity_in_play_mode() {
            return Err("Cannot refresh asset database because Unity Editor is in Play mode, please stop Play mode and try again.".into());
        }

        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();

            let operation_start = std::time::Instant::now();

            // Send the refresh message, allow configured timeout to send
            client
                .send_refresh_message(None)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            //println!("[DEBUG] Refresh message sent");
            debug_log!("Refresh message sent");

            // refresh start should be when we actually send the message
            let refresh_start_time = SystemTime::now();

            // Track refresh process state
            let mut refresh_response_received = false;
            let mut compilation_started = false;
            let mut compilation_finished = false;

            let timeout_duration = Duration::from_secs(REFRESH_TOTAL_TIMEOUT_SECS);
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
                                    error_log!("Refresh failed: {}", message);

                                    // Return early with error result
                                    let duration = operation_start.elapsed().as_secs_f64();
                                    return Ok(RefreshResult {
                                        refresh_completed: false,
                                        refresh_error_message: Some(message),
                                        compilation_started: false,
                                        compilation_completed: false,
                                        problems: Vec::new(),
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
                    problems: Vec::new(),
                    duration_seconds: duration,
                });
            }

            // Wait for compilation started event for 1 second
            let compilation_wait_start = std::time::Instant::now();
            while compilation_wait_start.elapsed()
                < Duration::from_millis(COMPILATION_WAIT_TIMEOUT_MILLIS)
                && !compilation_started
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
                    match timeout(
                        Duration::from_secs(COMPILATION_FINISH_TIMEOUT_SECS),
                        event_receiver.recv(),
                    )
                    .await
                    {
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
                        refresh_error_message: Some(format!(
                            "Timeout waiting for compilation to finish after {} seconds",
                            COMPILATION_FINISH_TIMEOUT_SECS
                        )),
                        compilation_started: true,
                        compilation_completed: false,
                        problems: Vec::new(),
                        duration_seconds: duration,
                    });
                }

                // Wait additional time for error logs to arrive after compilation finishes
                debug_log!(
                    "Waiting {} second(s) for error logs after compilation finished",
                    POST_COMPILATION_WAIT_SECS
                );
                tokio::time::sleep(Duration::from_secs(POST_COMPILATION_WAIT_SECS)).await;
            } else {
                debug_log!(
                    "No compilation started within {} millisecond(s)",
                    COMPILATION_WAIT_TIMEOUT_MILLIS
                );
            }

            // Filter logs from the existing log collection based on the determined time period
            let logs: Vec<String> =
                self.collect_refresh_logs(refresh_start_time, compilation_started);

            let duration = operation_start.elapsed().as_secs_f64();
            debug_log!(
                "Refresh completed, collected {} error logs in {:.2} seconds",
                logs.len(),
                duration
            );

            Ok(RefreshResult {
                refresh_completed: !compilation_started || compilation_finished,
                refresh_error_message: None,
                compilation_started: compilation_started,
                compilation_completed: compilation_finished,
                problems: logs,
                duration_seconds: duration,
            })
        } else {
            Err(MESSAGING_CLIENT_NOT_INIT_ERROR.into())
        }
    }

    /// Collect relevant logs during refresh
    fn collect_refresh_logs(
        &mut self,
        refresh_start_time: SystemTime,
        _compilation_started: bool,
    ) -> Vec<String> {
        let mut logs: Vec<String> = Vec::new();

        // Get errors and warnings during this refresh (excluding CS warnings because there can be too many)
        if let Ok(log_manager_guard) = self.log_manager.lock() {
            let recent_logs = log_manager_guard.get_recent_logs(refresh_start_time, None, None);

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

    /// Stop the messaging client and cleanup
    pub async fn shutdown(&mut self) {
        // Clean up messaging client
        self.cleanup_messaging_client().await;

        // Clear all logs and seen logs to prevent state leakage between test runs
        self.clear_logs();

        // Add a small delay to ensure Unity has time to fully shut down
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    /// Check for any test timeout, including test finish timeout and also next test start timeout
    fn check_test_timeout(
        &self,
        test_states: &mut HashMap<String, TestState>,
        run_start_time: Option<Instant>,
    ) -> Result<(), String> {
        let mut is_running_leaf_test = false;
        let mut last_test_finish_time: Option<Instant> = None;
        for test_state in test_states.values_mut() {
            // test finished
            if let Some(finish_time) = test_state.finish_time {
                if last_test_finish_time.is_none() || last_test_finish_time.unwrap() < finish_time {
                    last_test_finish_time = Some(finish_time);
                }
            }
            // test is running
            else {
                if !test_state.adapater.has_children
                    && test_state.start_time.elapsed() > Duration::from_secs(TEST_TIMEOUT_SECS)
                {
                    return Err(format!(
                        "Test timeout waiting for test {} to finish, after {} seconds",
                        test_state.adapater.full_name, TEST_TIMEOUT_SECS
                    ));
                }

                if !test_state.adapater.has_children {
                    is_running_leaf_test = true;
                }
            }
        }

        if !is_running_leaf_test {
            if last_test_finish_time.is_some()
                && last_test_finish_time.unwrap().elapsed()
                    > Duration::from_secs(TEST_START_TIMEOUT_SECS)
            {
                return Err(format!(
                    "Test timeout waiting for the next test to start, after {} seconds",
                    TEST_START_TIMEOUT_SECS
                ));
            } else if last_test_finish_time.is_none()
                && run_start_time.is_some()
                && run_start_time.unwrap().elapsed() > Duration::from_secs(TEST_START_TIMEOUT_SECS)
            {
                return Err(format!(
                    "Test timeout waiting for the first test to start, after {} seconds",
                    TEST_START_TIMEOUT_SECS
                ));
            }
        }

        Ok(())
    }

    /// Create test result with error
    fn create_test_result_with_error(
        &self,
        root_test_adaptor: Option<TestAdaptor>,
        test_results: Vec<SimpleTestResult>,
        test_states: HashMap<String, TestState>,
        error_message: &str,
    ) -> TestExecutionResult {
        // let's count the tests
        let mut pass_count = 0;
        let mut fail_count = 0;
        for test_result in test_results.iter() {
            if test_result.passed {
                pass_count += 1;
            } else {
                fail_count += 1;
            }
        }

        let mut test_count = pass_count + fail_count;
        if root_test_adaptor.is_some() {
            test_count = root_test_adaptor.unwrap().test_count;
        }

        // also count non fishied test as failed
        for test_state in test_states.values() {
            if !test_state.adapater.has_children && test_state.finish_time.is_none() {
                fail_count += 1;
            }
        }

        // let's estimate duration by finding the first test start time
        let mut first_test_start_time: Option<Instant> = None;
        for test_state in test_states.values() {
            if first_test_start_time.is_none()
                || first_test_start_time.unwrap() > test_state.start_time
            {
                first_test_start_time = Some(test_state.start_time);
            }
        }

        let mut duration_seconds = 0.0;
        if let Some(first_test_start_time) = first_test_start_time {
            duration_seconds = first_test_start_time.elapsed().as_secs_f64();
        }

        TestExecutionResult {
            test_results,
            execution_completed: false,
            error_message: error_message.into(),
            pass_count,
            fail_count,
            duration_seconds,
            test_count: test_count,
            skip_count: test_count - pass_count - fail_count,
        }
    }
}

impl Drop for UnityManager {
    fn drop(&mut self) {
        // Clean up messaging client synchronously
        if let Some(client) = self.messaging_client.take() {
            drop(client);
        }

        self.current_unity_pid = None;
    }
}
