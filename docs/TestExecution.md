# Unity Test Execution

This document describes how to use the Unity Code MCP library to execute Unity tests programmatically.

## Overview

The `UnityManager` provides comprehensive test execution capabilities that allow you to:

- Retrieve lists of available tests
- Execute tests with various filtering options
- Monitor test execution progress and results
- Handle test execution timeouts and errors

## Test Modes

Unity supports two test execution modes:

- **EditMode**: Tests that run in the Unity Editor without entering play mode
- **PlayMode**: Tests that run in Unity's play mode

```rust
use unity_code_mcp::TestMode;

let edit_mode = TestMode::EditMode;
let play_mode = TestMode::PlayMode;
```

## Test Filters

You can execute tests using different filter criteria:

### Execute All Tests
```rust
use unity_code_mcp::{UnityManager, TestMode};

let mut unity_manager = UnityManager::new(project_path).await?;
unity_manager.initialize_messaging().await?;

// Execute all EditMode tests
let result = unity_manager.execute_all_tests(TestMode::EditMode).await?;
```

### Execute Tests in Specific Assembly
```rust
// Execute tests in a specific assembly
let result = unity_manager.execute_assembly_tests(
    TestMode::EditMode, 
    "MyTests.dll".to_string()
).await?;
```

### Execute Specific Test
```rust
// Execute a specific test by its full name
let result = unity_manager.execute_specific_test(
    TestMode::EditMode,
    "MyNamespace.MyTestClass.MyTestMethod".to_string()
).await?;
```

### Custom Filter
```rust
use unity_code_mcp::TestFilter;

// Execute tests matching a custom filter
let custom_filter = TestFilter::Custom {
    mode: TestMode::EditMode,
    filter: "MyTests".to_string(),
};
let result = unity_manager.execute_tests(custom_filter).await?;
```

## Retrieving Test Lists

Before executing tests, you can retrieve the list of available tests:

```rust
// Get list of EditMode tests
let test_list_json = unity_manager.retrieve_test_list(TestMode::EditMode).await?;
println!("Available tests: {}", test_list_json);
```

The test list is returned as a JSON string containing the complete test hierarchy.

## Test Execution Results

Test execution returns a `TestExecutionResult` struct containing:

```rust
pub struct TestExecutionResult {
    pub started_tests: Vec<String>,     // List of tests that started
    pub finished_tests: Vec<String>,    // List of tests that finished
    pub test_list: Option<String>,      // Optional test list (if retrieved during execution)
    pub execution_completed: bool,      // Whether execution completed successfully
}
```

### Example: Processing Results
```rust
let result = unity_manager.execute_all_tests(TestMode::EditMode).await?;

if result.execution_completed {
    println!("Test execution completed successfully!");
    println!("Tests started: {}", result.started_tests.len());
    println!("Tests finished: {}", result.finished_tests.len());
    
    // Print individual test results
    for test in &result.finished_tests {
        println!("Finished: {}", test);
    }
} else {
    println!("Test execution did not complete properly");
}
```

## Error Handling

Test execution methods return `Result` types. Common error scenarios include:

- Unity Editor not running
- Messaging client not initialized
- Timeout waiting for test completion
- Communication errors with Unity

```rust
match unity_manager.execute_all_tests(TestMode::EditMode).await {
    Ok(result) => {
        // Handle successful execution
        println!("Tests completed: {}", result.execution_completed);
    },
    Err(e) => {
        // Handle errors
        eprintln!("Test execution failed: {}", e);
    }
}
```

## Timeouts

The test execution methods have built-in timeouts:

- **Test List Retrieval**: 30 seconds
- **Test Execution**: 5 minutes (300 seconds)
- **Individual Event Timeout**: 10 seconds

These timeouts help prevent hanging when Unity becomes unresponsive.

## Running the Example

A complete example is provided in `examples/test_execution_example.rs`:

```bash
# Run the example with a specific Unity project path
cargo run --example test_execution_example -- "C:\\Path\\To\\Your\\Unity\\Project"

# Or run with default path
cargo run --example test_execution_example
```

## Prerequisites

1. Unity Editor must be running with your project open
2. The Unity project must have the Unity Package Messaging Protocol package installed
3. The messaging client must be initialized before executing tests

## Best Practices

1. Always initialize the messaging client before attempting test operations
2. Handle timeouts gracefully in your application
3. Check the `execution_completed` flag in results to ensure tests ran successfully
4. Use appropriate test filters to avoid running unnecessary tests
5. Monitor the started and finished test lists to track progress