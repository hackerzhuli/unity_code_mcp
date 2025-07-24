use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::unity_messages::{TestAdaptor, TestFilter};

/// States of the test execution task
#[derive(Debug, Clone, PartialEq)]
enum TestExecutionState {
    /// Task has been created but not started
    NotStarted,
    /// Test execution message has been sent, waiting for TestRunStarted
    TestExecutionSent,
    /// Test run has started, waiting for tests to complete
    TestRunStarted,
    /// Test execution is complete
    Finished,
    /// Task failed or timed out
    Failed,
}

/// Tracking running state of individual tests
/// 
/// Maintains timing information and test metadata for each test during execution.
/// Used internally by UnityTestExecutionTask to monitor test progress and detect timeouts.
#[derive(Debug, Clone)]
pub struct TestState {
    pub start_time: Instant,
    pub finish_time: Option<Instant>,
    pub adaptor: TestAdaptor,
}

/// Simplified test result containing only essential information
/// 
/// A lightweight representation of a test execution result, containing all the
/// necessary information for reporting without Unity-specific implementation details.
/// This struct is designed to be easily serializable and transferable.
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

/// Comprehensive test execution result
/// 
/// Contains aggregated results from a complete test run, including individual test results,
/// summary statistics, timing information, and any error messages that occurred during execution.
/// This is the primary result type returned by the test execution system.
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

/// Task for managing Unity test execution operations
/// 
/// This struct encapsulates the entire lifecycle of Unity test execution, from initial
/// test discovery through final result collection. It provides comprehensive state management,
/// timeout handling, and result aggregation for Unity test runs.
/// 
/// The task follows this state flow:
/// 1. NotStarted -> TestExecutionSent (when test execution message is sent)
/// 2. TestExecutionSent -> TestRunStarted (when TestRunStarted event is received)
/// 3. TestRunStarted -> Finished/Failed (when TestRunFinished event is received or timeout occurs)
/// 
/// During the TestRunStarted phase, the task handles:
/// - TestStarted events (tracks individual test timing)
/// - TestFinished events (collects individual test results)
/// - Timeout monitoring for both individual tests and test start delays
/// 
/// Configurable timeouts prevent indefinite waiting at each stage of execution.
pub struct UnityTestExecutionTask {
    operation_start: Instant,
    state: TestExecutionState,
    filter: TestFilter,
    error_message: Option<String>,
    
    // Test execution state
    test_states: HashMap<String, TestState>,
    test_results: Vec<SimpleTestResult>,
    run_start_time: Option<Instant>,
    root_test_adaptor: Option<TestAdaptor>,
    
    // Configurable timeout values (all in seconds)
    test_run_start_timeout_secs: f64,
    test_timeout_secs: f64,
    test_start_timeout_secs: f64,
}

impl UnityTestExecutionTask {
    /// Create a new test execution task with configurable timeouts
    ///
    /// # Arguments
    ///
    /// * `filter` - The test filter specifying which tests to execute
    /// * `test_run_start_timeout_secs` - Timeout in seconds to wait for TestRunStarted event
    /// * `test_timeout_secs` - Timeout in seconds for an individual test to complete
    /// * `test_start_timeout_secs` - Timeout in seconds to wait for the next test to start
    pub fn new(
        filter: TestFilter,
        test_run_start_timeout_secs: f64,
        test_timeout_secs: f64,
        test_start_timeout_secs: f64,
    ) -> Self {
        Self {
            operation_start: Instant::now(),
            state: TestExecutionState::NotStarted,
            filter,
            error_message: None,
            test_states: HashMap::new(),
            test_results: Vec::new(),
            run_start_time: None,
            root_test_adaptor: None,
            test_run_start_timeout_secs,
            test_timeout_secs,
            test_start_timeout_secs,
        }
    }

    /// Mark the test execution as started
    pub fn mark_test_execution_started(&mut self) -> Result<(), String> {
        if self.state != TestExecutionState::NotStarted {
            return Err(format!("Cannot start test execution from state {:?}", self.state));
        }
        self.state = TestExecutionState::TestExecutionSent;
        Ok(())
    }

    /// Mark the test run as started
    pub fn mark_test_run_started(&mut self, test_adaptors: Vec<TestAdaptor>) -> Result<(), String> {
        if self.state != TestExecutionState::TestExecutionSent {
            return Err(format!("Cannot start test run from state {:?}", self.state));
        }
        
        self.state = TestExecutionState::TestRunStarted;
        self.run_start_time = Some(Instant::now());
        
        if let Some(first_adaptor) = test_adaptors.first() {
            self.root_test_adaptor = Some(first_adaptor.clone());
        }
        
        Ok(())
    }

    /// Handle test started event
    pub fn handle_test_started(&mut self, test_adaptors: Vec<TestAdaptor>) -> Result<(), String> {
        if self.state != TestExecutionState::TestRunStarted {
            return Err(format!("Cannot handle test started from state {:?}", self.state));
        }
        
        // Build test ID to adaptor mapping from parsed data
        for adaptor in test_adaptors {
            self.test_states.insert(
                adaptor.id.clone(),
                TestState {
                    start_time: Instant::now(),
                    finish_time: None,
                    adaptor,
                },
            );
        }
        
        Ok(())
    }

    /// Handle test finished event
    pub fn handle_test_finished(&mut self, test_result_adaptors: Vec<crate::unity_messages::TestResultAdaptor>) -> Result<(), String> {
        if self.state != TestExecutionState::TestRunStarted {
            return Err(format!("Cannot handle test finished from state {:?}", self.state));
        }
        
        // Extract individual test results from parsed data
        for adaptor in test_result_adaptors {
            // Record finish time
            if let Some(test_state) = self.test_states.get_mut(&adaptor.test_id) {
                test_state.finish_time = Some(Instant::now());
            }

            // Only add tests that don't have children
            if adaptor.has_children {
                continue;
            }

            // Create SimpleTestResult from TestResultAdaptor
            if let Some(test_state) = self.test_states.get(&adaptor.test_id) {
                let simple_result = SimpleTestResult {
                    full_name: test_state.adaptor.full_name.clone(),
                    error_stack_trace: adaptor.stack_trace.clone(),
                    passed: adaptor.result_state == "Passed",
                    duration_seconds: adaptor.duration,
                    error_message: adaptor.message.clone(),
                    output_logs: adaptor.output.clone(),
                };

                self.test_results.push(simple_result);
            }
        }
        
        Ok(())
    }

    /// Handle test run finished event
    pub fn handle_test_run_finished(&mut self, test_result_adaptors: Vec<crate::unity_messages::TestResultAdaptor>) -> Result<TestExecutionResult, String> {
        if self.state != TestExecutionState::TestRunStarted {
            return Err(format!("Cannot handle test run finished from state {:?}", self.state));
        }
        
        self.state = TestExecutionState::Finished;
        
        // Extract pass/fail counts from parsed data
        if let Some(adaptor) = test_result_adaptors.first() {
            let result = TestExecutionResult {
                pass_count: adaptor.pass_count,
                fail_count: adaptor.fail_count,
                skip_count: adaptor.skip_count,
                duration_seconds: adaptor.duration,
                test_count: adaptor.pass_count + adaptor.fail_count + adaptor.skip_count,
                test_results: self.test_results.clone(),
                execution_completed: true,
                error_message: String::new(),
            };
            
            Ok(result)
        } else {
            Err("No test result adaptors in TestRunFinished".to_string())
        }
    }

    /// Check if the task has completed
    pub fn is_completed(&self) -> bool {
        matches!(self.state, TestExecutionState::Finished | TestExecutionState::Failed)
    }

    /// Get the current duration of the operation
    pub fn duration(&self) -> Duration {
        self.operation_start.elapsed()
    }

    /// Check if test execution was successful
    pub fn is_successful(&self) -> bool {
        self.state == TestExecutionState::Finished && self.error_message.is_none()
    }

    /// Get the test filter
    pub fn filter(&self) -> &TestFilter {
        &self.filter
    }

    /// Update the task state and check for timeouts
    /// 
    /// Returns `Ok(())` if no timeout occurred, or an `Err` with a timeout message if the task timed out.
    pub fn update(&mut self) -> Result<(), String> {
        if self.is_completed() {
            return Ok(());
        }
        
        let elapsed = self.duration();
        
        match self.state {
            TestExecutionState::TestExecutionSent => {
                if elapsed > Duration::from_secs_f64(self.test_run_start_timeout_secs) {
                    let message = format!(
                        "Test run didn't start within {} seconds. Hint: Unity Editor is busy and can't respond now, please try again later.",
                        self.test_run_start_timeout_secs
                    );
                    self.state = TestExecutionState::Failed;
                    self.error_message = Some(message.clone());
                    return Err(message);
                }
            }
            TestExecutionState::TestRunStarted => {
                if let Err(e) = self.check_test_timeout() {
                    self.state = TestExecutionState::Failed;
                    self.error_message = Some(e.clone());
                    return Err(e);
                }
            }
            _ => {}
        }
        
        Ok(())
    }

    /// Check for any test timeout, including test finish timeout and also next test start timeout
    fn check_test_timeout(&self) -> Result<(), String> {
        let mut is_running_leaf_test = false;
        let mut last_test_finish_time: Option<Instant> = None;
        
        for test_state in self.test_states.values() {
            // Test finished
            if let Some(finish_time) = test_state.finish_time {
                if last_test_finish_time.is_none() || last_test_finish_time.unwrap() < finish_time {
                    last_test_finish_time = Some(finish_time);
                }
            }
            // Test is running
            else {
                if !test_state.adaptor.has_children
                    && test_state.start_time.elapsed() > Duration::from_secs_f64(self.test_timeout_secs)
                {
                    return Err(format!(
                        "Test timeout waiting for test {} to finish, after {} seconds",
                        test_state.adaptor.full_name, self.test_timeout_secs
                    ));
                }

                if !test_state.adaptor.has_children {
                    is_running_leaf_test = true;
                }
            }
        }

        if !is_running_leaf_test {
            if let Some(last_finish_time) = last_test_finish_time {
                if last_finish_time.elapsed() > Duration::from_secs_f64(self.test_start_timeout_secs) {
                    return Err(format!(
                        "Test timeout waiting for the next test to start, after {} seconds",
                        self.test_start_timeout_secs
                    ));
                }
            } else if let Some(run_start_time) = self.run_start_time {
                if run_start_time.elapsed() > Duration::from_secs_f64(self.test_start_timeout_secs) {
                    return Err(format!(
                        "Test timeout waiting for the first test to start, after {} seconds",
                        self.test_start_timeout_secs
                    ));
                }
            }
        }

        Ok(())
    }

    /// Create test result with error
    pub fn create_test_result_with_error(&self, error_message: &str) -> TestExecutionResult {
        // Count the tests
        let mut pass_count = 0;
        let mut fail_count = 0;
        for test_result in self.test_results.iter() {
            if test_result.passed {
                pass_count += 1;
            } else {
                fail_count += 1;
            }
        }

        let mut test_count = pass_count + fail_count;
        if let Some(ref root_adaptor) = self.root_test_adaptor {
            test_count = root_adaptor.test_count;
        }

        // Also count non-finished test as failed
        for test_state in self.test_states.values() {
            if !test_state.adaptor.has_children && test_state.finish_time.is_none() {
                fail_count += 1;
            }
        }

        // Estimate duration by finding the first test start time
        let mut first_test_start_time: Option<Instant> = None;
        for test_state in self.test_states.values() {
            if first_test_start_time.is_none() || first_test_start_time.unwrap() > test_state.start_time {
                first_test_start_time = Some(test_state.start_time);
            }
        }

        let duration_seconds = if let Some(first_test_start_time) = first_test_start_time {
            first_test_start_time.elapsed().as_secs_f64()
        } else {
            0.0
        };

        TestExecutionResult {
            test_results: self.test_results.clone(),
            execution_completed: false,
            error_message: error_message.to_string(),
            pass_count,
            fail_count,
            duration_seconds,
            test_count,
            skip_count: test_count - pass_count - fail_count,
        }
    }
}

#[cfg(test)]
#[path = "unity_test_task_tests.rs"]
mod tests;