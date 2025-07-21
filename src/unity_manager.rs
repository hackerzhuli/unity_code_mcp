use std::collections::{VecDeque, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, broadcast};
use tokio::time::timeout;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::unity_messaging_client::{UnityMessagingClient, UnityEvent, UnityMessagingError, LogLevel};
use crate::unity_project_manager::UnityProjectManager;

/// Unity test result structures for proper deserialization
#[derive(Debug, Deserialize, Serialize)]
struct TestResultAdaptorContainer {
    #[serde(rename = "TestResultAdaptors")]
    test_result_adaptors: Vec<TestResultAdaptor>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TestResultAdaptor {
    #[serde(rename = "TestId")]
    test_id: String,
    #[serde(rename = "PassCount")]
    pass_count: u32,
    #[serde(rename = "FailCount")]
    fail_count: u32,
    #[serde(rename = "InconclusiveCount")]
    inconclusive_count: u32,
    #[serde(rename = "SkipCount")]
    skip_count: u32,
    #[serde(rename = "ResultState")]
    result_state: String,
    #[serde(rename = "StackTrace")]
    stack_trace: String,
    #[serde(rename = "TestStatus")]
    test_status: u32,
    #[serde(rename = "AssertCount")]
    assert_count: u32,
    #[serde(rename = "Duration")]
    duration: f64,
    #[serde(rename = "StartTime")]
    start_time: i64,
    #[serde(rename = "EndTime")]
    end_time: i64,
    #[serde(rename = "Message")]
    message: String,
    #[serde(rename = "Output")]
    output: String,
    #[serde(rename = "HasChildren")]
    has_children: bool,
    #[serde(rename = "Parent")]
    parent: i32,
}

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
    Specific {
        mode: TestMode,
        test_name: String,
    },
    /// Execute tests matching a custom filter string
    Custom {
        mode: TestMode,
        filter: String,
    },
}

impl TestFilter {
    /// Convert to the filter string format expected by Unity
    pub fn to_filter_string(&self) -> String {
        match self {
            TestFilter::All(mode) => mode.as_str().to_string(),
            TestFilter::Assembly { mode, assembly_name } => {
                format!("{}:{}", mode.as_str(), assembly_name)
            },
            TestFilter::Specific { mode, test_name } => {
                format!("{}:{}", mode.as_str(), test_name)
            },
            TestFilter::Custom { mode, filter } => {
                format!("{}:{}", mode.as_str(), filter)
            },
        }
    }
}

/// Test execution result
#[derive(Debug, Clone)]
pub struct TestExecutionResult {
    pub started_tests: Vec<String>,
    pub finished_tests: Vec<String>,
    pub test_list: Option<String>,
    pub execution_completed: bool,
    pub pass_count: u32,
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
}

impl UnityManager {
    /// Create a new UnityManager for the given Unity project path
    pub async fn new(project_path: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let project_manager = UnityProjectManager::new(project_path).await?;
        
        Ok(UnityManager {
            messaging_client: None,
            project_manager,
            logs: Arc::new(Mutex::new(VecDeque::new())),
            seen_logs: Arc::new(Mutex::new(HashSet::new())),
            max_logs: 1000, // Keep last 1000 log entries
            event_receiver: None,
            is_listening: false,
        })
    }

    /// Initialize the messaging client if Unity is running
    pub async fn initialize_messaging(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Update process info to check if Unity is running
        self.project_manager.update_process_info().await?;
        
        if let Some(unity_pid) = self.project_manager.unity_process_id() {
            let mut client = UnityMessagingClient::new(unity_pid).await?;
            
            // Subscribe to events before starting listener
            let event_receiver = client.subscribe_to_events();
            
            // Start listening for Unity events
            client.start_listening().await?;
            self.is_listening = true;
            
            self.messaging_client = Some(client);
            
            // Start the log collection task with the event receiver
            self.start_log_collection_with_receiver(event_receiver).await;
            
            Ok(())
        } else {
            Err("Unity Editor is not running".into())
        }
    }

    /// Start collecting logs from Unity events with a specific receiver
    async fn start_log_collection_with_receiver(&mut self, mut event_receiver: broadcast::Receiver<UnityEvent>) {
        let logs = Arc::clone(&self.logs);
        let seen_logs = Arc::clone(&self.seen_logs);
        let max_logs = self.max_logs;
        
        tokio::spawn(async move {
            println!("[DEBUG] Log collection task started");
            loop {
                match event_receiver.recv().await {
                    Ok(event) => {
                        println!("[DEBUG] Log collection received event: {:?}", event);
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
                                    
                                    println!("[DEBUG] Adding log entry: [{:?}] {}", log_entry.level, log_entry.message);
                                    
                                    if let Ok(mut logs_guard) = logs.lock() {
                                        logs_guard.push_back(log_entry);
                                        
                                        // Keep only the last max_logs entries
                                        while logs_guard.len() > max_logs {
                                            logs_guard.pop_front();
                                        }
                                        
                                        println!("[DEBUG] Total logs now: {}", logs_guard.len());
                                    }
                                } else {
                                    println!("[DEBUG] Skipping duplicate log: [{:?}] {}", level, message);
                                }
                            },
                            _ => {
                                // Log other events for debugging but don't spam
                                if !matches!(event, UnityEvent::IsPlaying(_)) {
                                    println!("[DEBUG] Non-log event received: {:?}", event);
                                }
                            }
                        }
                    },
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        println!("[DEBUG] Log collection lagged, skipped {} messages", skipped);
                        continue;
                    },
                    Err(broadcast::error::RecvError::Closed) => {
                        println!("[DEBUG] Log collection channel closed, exiting task");
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
        self.messaging_client.as_ref()
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
        self.messaging_client.as_ref()
            .map(|client| client.is_connected(timeout_seconds))
            .unwrap_or(false)
    }

    /// Send a ping to Unity
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the ping was sent successfully
    pub async fn send_ping(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &self.messaging_client {
            client.send_ping().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Get Unity version
    pub async fn get_unity_version(&mut self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();
            
            // Send the version request
            client.get_version().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            
            // Wait for the Version event response
            let timeout_duration = Duration::from_secs(10);
            
            match timeout(timeout_duration, async {
                loop {
                    match event_receiver.recv().await {
                        Ok(UnityEvent::Version(version)) => return Ok(version),
                        Ok(_) => continue,
                        Err(_) => return Err("Event channel closed".into()),
                    }
                }
            }).await {
                Ok(result) => result,
                Err(_) => Err("Timeout waiting for Unity version response".into()),
            }
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Get Unity project path
    pub async fn get_project_path(&mut self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();
            
            // Send the project path request
            client.get_project_path().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            
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
            }).await {
                Ok(result) => result,
                Err(_) => Err("Timeout waiting for Unity project path response".into()),
            }
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Send refresh message to Unity
    pub async fn refresh_unity(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            client.send_refresh_message().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        } else {
            Err("Messaging client not initialized".into())
        }
    }

    /// Retrieve the list of available tests for the specified test mode
    /// 
    /// # Arguments
    /// 
    /// * `test_mode` - The test mode to retrieve tests for
    /// 
    /// # Returns
    /// 
    /// Returns the test list as a JSON string containing the complete test hierarchy
    pub async fn retrieve_test_list(&mut self, test_mode: TestMode) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();
            
            // Send the test list request
            client.retrieve_test_list(test_mode.as_str()).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            
            // Wait for the TestListRetrieved event response
            let timeout_duration = Duration::from_secs(30);
            
            match timeout(timeout_duration, async {
                loop {
                    match event_receiver.recv().await {
                        Ok(UnityEvent::TestListRetrieved(test_list)) => {
                            // The test list format is "TestMode:JsonData"
                            // We need to extract just the JSON part
                            if let Some(colon_pos) = test_list.find(':') {
                                return Ok(test_list[colon_pos + 1..].to_string());
                            } else {
                                return Ok(test_list);
                            }
                        },
                        Ok(_) => continue,
                        Err(_) => return Err("Event channel closed".into()),
                    }
                }
            }).await {
                Ok(result) => result,
                Err(_) => Err("Timeout waiting for Unity test list response".into()),
            }
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
    pub async fn execute_tests(&mut self, filter: TestFilter) -> Result<TestExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(client) = &mut self.messaging_client {
            // Subscribe to events before sending request
            let mut event_receiver = client.subscribe_to_events();
            
            // Send the test execution request
            let filter_string = filter.to_filter_string();
            println!("[DEBUG] Sending test filter to Unity: '{}'", filter_string);
            client.execute_tests(&filter_string).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            
            // Collect test execution events
            let mut result = TestExecutionResult {
                started_tests: Vec::new(),
                finished_tests: Vec::new(),
                test_list: None,
                execution_completed: false,
                pass_count: 0,
                fail_count: 0,
            };
            
            let timeout_duration = Duration::from_secs(300); // 5 minutes for test execution
            let start_time = std::time::Instant::now();
            
            // Wait for test execution to complete
            while start_time.elapsed() < timeout_duration {
                match timeout(Duration::from_secs(10), event_receiver.recv()).await {
                    Ok(Ok(event)) => {
                        match event {
                            UnityEvent::TestRunStarted => {
                                println!("[DEBUG] Test run started");
                            },
                            UnityEvent::TestStarted(test_info) => {
                                println!("[DEBUG] Test started: {}", test_info);
                                result.started_tests.push(test_info);
                            },
                            UnityEvent::TestFinished(test_result) => {
                                println!("[DEBUG] Test finished: {}", test_result);
                                result.finished_tests.push(test_result);
                            },
                            UnityEvent::TestRunFinished(data) => {
                                println!("[DEBUG] Test run finished");
                                
                                // Only process the first TestRunFinished event to avoid accumulation
                                if !result.execution_completed {
                                    result.execution_completed = true;
                                    
                                    // Parse the TestResultAdaptorContainer using proper deserialization
                                    match serde_json::from_str::<TestResultAdaptorContainer>(&data) {
                                        Ok(container) => {
                                            let mut pass_count = 0;
                                            let mut fail_count = 0;
                                            
                                            for adaptor in &container.test_result_adaptors {
                                                pass_count += adaptor.pass_count;
                                                fail_count += adaptor.fail_count;
                                            }
                                            
                                            result.pass_count = pass_count;
                                            result.fail_count = fail_count;
                                            println!("[DEBUG] Extracted counts: {} passed, {} failed", pass_count, fail_count);
                                        },
                                        Err(e) => {
                                            println!("[DEBUG] Failed to deserialize TestResultAdaptorContainer: {}", e);
                                            println!("[DEBUG] Raw data: {}", data);
                                        }
                                    }
                                }
                                break;
                            },
                            UnityEvent::TestListRetrieved(test_list) => {
                                // Sometimes Unity sends the test list during execution
                                result.test_list = Some(test_list);
                            },
                            _ => {
                                // Ignore other events during test execution
                            }
                        }
                    },
                    Ok(Err(_)) => {
                        return Err("Event channel closed during test execution".into());
                    },
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

    /// Execute all tests in the specified mode
    /// 
    /// # Arguments
    /// 
    /// * `test_mode` - The test mode to execute all tests for
    /// 
    /// # Returns
    /// 
    /// Returns a TestExecutionResult containing information about the test execution
    pub async fn execute_all_tests(&mut self, test_mode: TestMode) -> Result<TestExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        self.execute_tests(TestFilter::All(test_mode)).await
    }

    /// Execute tests in a specific assembly
    /// 
    /// # Arguments
    /// 
    /// * `test_mode` - The test mode
    /// * `assembly_name` - The name of the assembly (e.g., "MyTests.dll")
    /// 
    /// # Returns
    /// 
    /// Returns a TestExecutionResult containing information about the test execution
    pub async fn execute_assembly_tests(&mut self, test_mode: TestMode, assembly_name: String) -> Result<TestExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        self.execute_tests(TestFilter::Assembly { mode: test_mode, assembly_name }).await
    }

    /// Execute a specific test by its full name
    /// 
    /// # Arguments
    /// 
    /// * `test_mode` - The test mode
    /// * `test_name` - The full name of the test (e.g., "MyNamespace.MyTestClass.MyTestMethod")
    /// 
    /// # Returns
    /// 
    /// Returns a TestExecutionResult containing information about the test execution
    pub async fn execute_specific_test(&mut self, test_mode: TestMode, test_name: String) -> Result<TestExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        self.execute_tests(TestFilter::Specific { mode: test_mode, test_name }).await
    }



    /// Stop the messaging client and cleanup
    pub async fn shutdown(&mut self) {
        if let Some(client) = &mut self.messaging_client {
            client.stop_listening();
        }
        self.is_listening = false;
        self.messaging_client = None;
        self.event_receiver = None;
        
        // Clear all logs and seen logs to prevent state leakage between test runs
        self.clear_logs();
        
        // Add a small delay to ensure Unity has time to fully shut down
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

impl Drop for UnityManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}