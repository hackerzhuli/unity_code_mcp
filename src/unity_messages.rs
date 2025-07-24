use serde::{Deserialize, Serialize};

/// Unity compile error structures for proper deserialization
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogContainer {
    #[serde(rename = "Logs")]
    pub logs: Vec<Log>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Log {
    #[serde(rename = "Message")]
    pub message: String,
    #[serde(rename = "StackTrace")]
    pub stack_trace: String,
    #[serde(rename = "Timestamp")]
    pub timestamp: i64,
}

/// Unity test result structures for proper deserialization
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestResultAdaptorContainer {
    #[serde(rename = "TestResultAdaptors")]
    pub test_result_adaptors: Vec<TestResultAdaptor>,
}

/// Unity test adaptor structures for TestStarted events
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestAdaptorContainer {
    #[serde(rename = "TestAdaptors")]
    pub test_adaptors: Vec<TestAdaptor>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestAdaptor {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "FullName")]
    pub full_name: String,
    #[serde(rename = "Type")]
    pub test_type: u32,
    #[serde(rename = "Parent")]
    pub parent: i32,
    #[serde(rename = "Source")]
    pub source: String,
    #[serde(rename = "TestCount")]
    pub test_count: u32,
    #[serde(rename = "HasChildren")]
    pub has_children: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestResultAdaptor {
    #[serde(rename = "TestId")]
    pub test_id: String,
    #[serde(rename = "PassCount")]
    pub pass_count: u32,
    #[serde(rename = "FailCount")]
    pub fail_count: u32,
    #[serde(rename = "InconclusiveCount")]
    pub inconclusive_count: u32,
    #[serde(rename = "SkipCount")]
    pub skip_count: u32,
    #[serde(rename = "ResultState")]
    pub result_state: String,
    #[serde(rename = "StackTrace")]
    pub stack_trace: String,
    #[serde(rename = "TestStatus")]
    pub test_status: u32,
    #[serde(rename = "AssertCount")]
    pub assert_count: u32,
    #[serde(rename = "Duration")]
    pub duration: f64,
    #[serde(rename = "StartTime")]
    pub start_time: i64,
    #[serde(rename = "EndTime")]
    pub end_time: i64,
    #[serde(rename = "Message")]
    pub message: String,
    #[serde(rename = "Output")]
    pub output: String,
    #[serde(rename = "HasChildren")]
    pub has_children: bool,
    #[serde(rename = "Parent")]
    pub parent: i32,
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
#[derive(Debug, Clone, PartialEq)]
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

/// Events that can be broadcast from Unity messaging
#[derive(Debug, Clone)]
pub enum UnityEvent {
    /// Log message received from Unity
    LogMessage { level: LogLevel, message: String },
    /// Unity package version information (not Unity Editor version)
    PackageVersion(String),
    /// Unity project path
    ProjectPath(String),
    /// Test run started with parsed test information
    TestRunStarted(TestAdaptorContainer),
    /// Test run finished with parsed test results
    TestRunFinished(TestResultAdaptorContainer),
    /// Test started with parsed test information
    TestStarted(TestAdaptorContainer),
    /// Test finished with parsed test results
    TestFinished(TestResultAdaptorContainer),
    /// Compilation finished
    CompilationFinished,
    /// Compilation started
    CompilationStarted,
    /// Refresh completed with optional error message
    RefreshCompleted(String),
    /// Unity went online
    Online,
    /// Unity went offline
    Offline,
    /// Unity play mode changed
    IsPlaying(bool),
    /// Compile errors received from Unity
    CompileErrors(LogContainer),
}

/// Message types as defined in the Unity Package Messaging Protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum MessageType {
    None = 0,
    Ping = 1,
    Pong = 2,
    Play = 3,
    Stop = 4,
    Pause = 5,
    Unpause = 6,
    Refresh = 8,
    Info = 9,
    Error = 10,
    Warning = 11,
    Version = 14,
    ProjectPath = 16,
    Tcp = 17,
    TestRunStarted = 18,
    TestRunFinished = 19,
    TestStarted = 20,
    TestFinished = 21,
    TestListRetrieved = 22,
    RetrieveTestList = 23,
    ExecuteTests = 24,
    ShowUsage = 25,
    CompilationFinished = 100,
    PackageName = 101,
    Online = 102,
    Offline = 103,
    IsPlaying = 104,
    CompilationStarted = 105,
    GetCompileErrors = 106,
}

impl From<i32> for MessageType {
    fn from(value: i32) -> Self {
        match value {
            0 => MessageType::None,
            1 => MessageType::Ping,
            2 => MessageType::Pong,
            3 => MessageType::Play,
            4 => MessageType::Stop,
            5 => MessageType::Pause,
            6 => MessageType::Unpause,
            8 => MessageType::Refresh,
            9 => MessageType::Info,
            10 => MessageType::Error,
            11 => MessageType::Warning,
            14 => MessageType::Version,
            16 => MessageType::ProjectPath,
            17 => MessageType::Tcp,
            18 => MessageType::TestRunStarted,
            19 => MessageType::TestRunFinished,
            20 => MessageType::TestStarted,
            21 => MessageType::TestFinished,
            22 => MessageType::TestListRetrieved,
            23 => MessageType::RetrieveTestList,
            24 => MessageType::ExecuteTests,
            25 => MessageType::ShowUsage,
            100 => MessageType::CompilationFinished,
            101 => MessageType::PackageName,
            102 => MessageType::Online,
            103 => MessageType::Offline,
            104 => MessageType::IsPlaying,
            105 => MessageType::CompilationStarted,
            106 => MessageType::GetCompileErrors,
            _ => MessageType::None,
        }
    }
}

/// A message in the Unity Package Messaging Protocol
#[derive(Debug, Clone)]
pub struct Message {
    pub message_type: MessageType,
    pub value: String,
}


/// Log levels for Unity log messages
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

/// Errors that can occur during Unity messaging
#[derive(Debug, thiserror::Error)]
pub enum UnityMessagingError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Invalid message: {0}")]
    InvalidMessage(String),
    #[error("Timeout error: {0}")]
    Timeout(String)
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

impl Message {
    /// Creates a new message
    pub fn new(message_type: MessageType, value: String) -> Self {
        Self {
            message_type,
            value,
        }
    }

    /// Creates a ping message
    pub fn ping() -> Self {
        Self::new(MessageType::Ping, String::new())
    }

    /// Creates a pong message
    pub fn pong() -> Self {
        Self::new(MessageType::Pong, String::new())
    }

    /// Serializes the message to binary format (little-endian)
    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();

        // Message type (4 bytes, little-endian)
        buffer.extend_from_slice(&(self.message_type as i32).to_le_bytes());

        // String length (4 bytes, little-endian)
        let value_bytes = self.value.as_bytes();
        buffer.extend_from_slice(&(value_bytes.len() as i32).to_le_bytes());

        // String value (UTF-8 encoded)
        buffer.extend_from_slice(value_bytes);

        buffer
    }

    /// Deserializes a message from binary format
    pub fn deserialize(data: &[u8]) -> Result<Self, UnityMessagingError> {
        if data.len() < 8 {
            return Err(UnityMessagingError::InvalidMessage(
                "Message too short".to_string(),
            ));
        }

        // Read message type (4 bytes, little-endian)
        let message_type_bytes = &data[0..4];
        let message_type_value = i32::from_le_bytes([
            message_type_bytes[0],
            message_type_bytes[1],
            message_type_bytes[2],
            message_type_bytes[3],
        ]);
        let message_type = MessageType::from(message_type_value);

        // Read string length (4 bytes, little-endian)
        let length_bytes = &data[4..8];
        let string_length = i32::from_le_bytes([
            length_bytes[0],
            length_bytes[1],
            length_bytes[2],
            length_bytes[3],
        ]) as usize;

        // Validate string length
        if data.len() < 8 + string_length {
            return Err(UnityMessagingError::InvalidMessage(
                "String data incomplete".to_string(),
            ));
        }

        // Read string value
        let value_bytes = &data[8..8 + string_length];
        let value = String::from_utf8(value_bytes.to_vec())
            .map_err(|e| UnityMessagingError::InvalidMessage(format!("Invalid UTF-8: {}", e)))?;

        Ok(Message {
            message_type,
            value,
        })
    }
}
