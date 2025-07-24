use chrono;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

use crate::unity_log_manager::{UnityLogEntry, UnityLogManager};
use crate::unity_log_utils::{self, extract_main_message};
use crate::unity_messaging_client::UnityMessagingClient;
use crate::unity_project_manager::{UnityProjectError, UnityProjectManager};
use crate::unity_refresh_task::{RefreshResult, UnityRefreshAssetDatabaseTask};
use crate::unity_test_task::{TestExecutionResult, UnityTestExecutionTask};
use crate::{debug_log, error_log, info_log, warn_log};

// Timing constants for refresh operations

/// Total timeout in seconds for the refresh operation(after sent, not including compilation)
const REFRESH_TOTAL_TIMEOUT_SECS: f64 = 60.0;

/// Timeout in seconds to wait for compilation to start after refresh finished
const COMPILATION_WAIT_TIMEOUT_SECS: f64 = 1.0;

/// Timeout in seconds for compilation to finish
const COMPILATION_FINISH_TIMEOUT_SECS: f64 = 60.0;

/// Wait time in seconds after compilation finishes for logs to arrive
const POST_COMPILATION_WAIT_SECS: f64 = 1.0;

// Timing constants for test execution

/// Timeout in seconds for an individual test to complete after it starts
///
/// Note: The timeout is different in debug and release build for easy testing with a debug build
const TEST_TIMEOUT_SECS: f64 = if cfg!(debug_assertions) { 30.0 } else { 180.0 };

/// Timeout in seconds to wait for the next test to start
const TEST_START_TIMEOUT_SECS: f64 = 3.0;

/// Timeout in seconds to wait for TestRunStarted event, this need to be a little bit longer, because Unity Editor can be busy(e.g. importing assets, something that we don't track, we track compilation, running tests, Play Mode, but there can be operation that we don't track happening)
const TEST_RUN_START_TIMEOUT_SECS: f64 = 30.0;

// RefreshResult is now defined in unity_refresh_task module

// Test-related structures are now imported from unity_test_task
use crate::unity_messages::{LogLevel, TestFilter, UnityEvent};

pub struct UnityManager {
    messaging_client: Option<UnityMessagingClient>,
    project_manager: UnityProjectManager,
    /// Log manager for handling Unity logs
    log_manager: Arc<Mutex<UnityLogManager>>,
    current_unity_pid: Option<u32>,
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
                                        log_manager_guard.add_log(level.clone(), message.clone());
                                    }
                                }
                            }
                            UnityEvent::CompilationStarted => {
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
            log_manager_guard.get_logs_vec()
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
        if let Some(client) = &mut self.messaging_client {
            // Check if Unity is currently running tests to avoid conflicts
            if client.is_running_tests() {
                return Err("Cannot start a new test run because Unity Editor is still running tests, please try again later.".into());
            }

            // Check if Unity is currently in Play mode to avoid conflicts
            if client.is_in_play_mode() {
                return Err("Cannot start a new test run because Unity Editor is in Play mode, please stop Play mode and try again.".into());
            }

            // Create and initialize the test execution task
            let mut test_task = UnityTestExecutionTask::new(
                filter.clone(),
                TEST_RUN_START_TIMEOUT_SECS,
                TEST_TIMEOUT_SECS,
                TEST_START_TIMEOUT_SECS,
            );

            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();

            // Send the test execution request
            debug_log!(
                "Sending test filter to Unity: '{}'",
                filter.to_filter_string()
            );

            client
                .execute_tests(filter, None)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            // Mark test execution as started
            if let Err(e) = test_task.mark_test_execution_started() {
                return Err(format!("Failed to start test execution task: {}", e).into());
            }

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
                                if let Err(e) = test_task.mark_test_run_started(container.test_adaptors) {
                                    return Err(format!("Failed to mark test run started: {}", e).into());
                                }
                            }
                            UnityEvent::TestStarted(container) => {
                                debug_log!(
                                    "Test started with {} test adaptors",
                                    container.test_adaptors.len()
                                );

                                // Log test adaptor details
                                for adaptor in &container.test_adaptors {
                                    debug_log!(
                                        "TestStarted adaptor - Id: '{}', Name: '{}', FullName: '{}', Type: {:?}",
                                        adaptor.id,
                                        adaptor.name,
                                        adaptor.full_name,
                                        adaptor.test_type
                                    );
                                }

                                if let Err(e) = test_task.handle_test_started(container.test_adaptors) {
                                    error_log!("Failed to handle test started: {}", e);
                                }
                            }
                            UnityEvent::TestFinished(container) => {
                                debug_log!(
                                    "Test finished with {} test result adaptors",
                                    container.test_result_adaptors.len()
                                );

                                // Log test result details
                                for adaptor in &container.test_result_adaptors {
                                    debug_log!(
                                        "TestFinished adaptor - TestId: '{}', PassCount: {}, FailCount: {}, ResultState: '{}'",
                                        adaptor.test_id,
                                        adaptor.pass_count,
                                        adaptor.fail_count,
                                        adaptor.result_state
                                    );
                                }

                                if let Err(e) = test_task.handle_test_finished(container.test_result_adaptors) {
                                    error_log!("Failed to handle test finished: {}", e);
                                }
                            }
                            UnityEvent::TestRunFinished(container) => {
                                debug_log!("Test run finished");
                                test_task.handle_test_run_finished(container.test_result_adaptors);
                                break;
                            }
                            _ => {
                                // Ignore other events during test execution
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        test_task.finish_with_error(
                            "Event channel closed during test execution. Hint: Unity Editor process shuts down unexpectedly, it could have crashed or been killed by user.",
                        );
                        break;
                    }
                    Err(_) => {
                        // Check for timeouts
                        test_task.update();

                        // Important: Check if Unity Editor is still running
                        self.update_unity_connection().await;
                    }
                }
            }

            if test_task.is_completed(){
                Ok(test_task.build_result())
            }else{
                Err("Test execution internal error".into())
            }
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
        if let Some(client) = &mut self.messaging_client {
            // Check if Unity is currently running tests to avoid conflicts
            if client.is_running_tests() {
                return Err("Cannot refresh asset database because Unity Editor is still running tests, please try again later.".into());
            }

            // Check if Unity is currently in Play mode to avoid conflicts
            if client.is_in_play_mode() {
                return Err("Cannot refresh asset database because Unity Editor is in Play mode, please stop Play mode and try again.".into());
            }

            // Create and initialize the refresh task
            let mut refresh_task = UnityRefreshAssetDatabaseTask::new(
                REFRESH_TOTAL_TIMEOUT_SECS,
                COMPILATION_WAIT_TIMEOUT_SECS,
                COMPILATION_FINISH_TIMEOUT_SECS,
            );

            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();

            // Send the refresh message
            client
                .send_refresh_message(None)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            debug_log!("Refresh message sent");

            // Mark refresh as started
            if let Err(e) = refresh_task.mark_refresh_started() {
                return Err(format!("Failed to start refresh task: {}", e).into());
            }

            let timeout_duration = Duration::from_secs_f64(REFRESH_TOTAL_TIMEOUT_SECS);
            let start_time = std::time::Instant::now();

            // Wait for refresh response first
            while start_time.elapsed() < timeout_duration && !refresh_task.is_completed() {
                match timeout(Duration::from_secs(5), event_receiver.recv()).await {
                    Ok(Ok(event)) => {
                        match event {
                            UnityEvent::RefreshCompleted(message) => {
                                debug_log!("Refresh completed with message: '{}'", message);
                                let success = message.is_empty();
                                let error_msg = if message.is_empty() {
                                    None
                                } else {
                                    Some(message)
                                };
                                if let Err(e) =
                                    refresh_task.mark_refresh_completed(success, error_msg.clone())
                                {
                                    warn_log!("Failed to mark refresh completed: {}", e).into()
                                }
                            }
                            UnityEvent::CompilationStarted => {
                                debug_log!("Compilation started");
                                if let Err(e) = refresh_task.mark_compilation_started() {
                                    warn_log!("Failed to mark compilation started: {}", e);
                                }
                            }
                            UnityEvent::CompilationFinished => {
                                debug_log!("Compilation finished");
                                if let Err(e) = refresh_task.mark_compilation_finished() {
                                    warn_log!("Failed to mark compilation finished: {}", e);
                                }
                            }
                            _ => {
                                // Ignore other events
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        // event channel stopped, Unity Editor process is likely shutdown
                        return Err("Event channel closed during refresh. Hint: Unity Editor process shuts down unexpectedly, it could have crashed or been killed by user.".into());
                    }
                    Err(_) => {
                    }
                }

                refresh_task.update();
                if refresh_task.is_completed() {
                    break;
                }
            }

            // Wait additional time for error logs to arrive after compilation finishes
            if refresh_task.is_successful() && refresh_task.has_compilation() {
                sleep(Duration::from_secs_f64(POST_COMPILATION_WAIT_SECS)).await;
            }

            let result = refresh_task.build_result(&self.log_manager);
            Ok(result)
        } else {
            Err(MESSAGING_CLIENT_NOT_INIT_ERROR.into())
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
    }
}
