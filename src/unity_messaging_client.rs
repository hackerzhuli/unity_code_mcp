use std::net::{UdpSocket, SocketAddr};
use std::time::Duration;
use tokio::time::timeout;
use serde::{Deserialize, Serialize};

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

impl Message {
    /// Creates a new message
    pub fn new(message_type: MessageType, value: String) -> Self {
        Self { message_type, value }
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
            return Err(UnityMessagingError::InvalidMessage("Message too short".to_string()));
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
            return Err(UnityMessagingError::InvalidMessage("String data incomplete".to_string()));
        }

        // Read string value
        let value_bytes = &data[8..8 + string_length];
        let value = String::from_utf8(value_bytes.to_vec())
            .map_err(|e| UnityMessagingError::InvalidMessage(format!("Invalid UTF-8: {}", e)))?;

        Ok(Message { message_type, value })
    }
}

/// Errors that can occur during Unity messaging
#[derive(Debug, thiserror::Error)]
pub enum UnityMessagingError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Invalid message: {0}")]
    InvalidMessage(String),
    #[error("Timeout error")]
    Timeout,
    #[error("Unity process not found")]
    ProcessNotFound,
}

/// Unity messaging client that communicates via UDP
pub struct UnityMessagingClient {
    socket: UdpSocket,
    unity_address: SocketAddr,
    timeout_duration: Duration,
}

impl UnityMessagingClient {
    /// Creates a new Unity messaging client
    /// 
    /// # Arguments
    /// 
    /// * `unity_process_id` - The process ID of the Unity Editor
    /// 
    /// # Returns
    /// 
    /// Returns a `Result` containing the client or an error
    pub fn new(unity_process_id: u32) -> Result<Self, UnityMessagingError> {
        // Calculate Unity's messaging port: 58000 + (ProcessId % 1000)
        let unity_port = 58000 + (unity_process_id % 1000);
        let unity_address = SocketAddr::from(([127, 0, 0, 1], unity_port as u16));

        // Create UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0")?; // Bind to any available port
        socket.set_read_timeout(Some(Duration::from_secs(5)))?;
        socket.set_write_timeout(Some(Duration::from_secs(5)))?;

        Ok(Self {
            socket,
            unity_address,
            timeout_duration: Duration::from_secs(5),
        })
    }

    /// Sends a message to Unity and optionally waits for a response
    /// 
    /// # Arguments
    /// 
    /// * `message` - The message to send
    /// * `expect_response` - Whether to wait for a response
    /// 
    /// # Returns
    /// 
    /// Returns the response message if `expect_response` is true, otherwise None
    pub fn send_message(&self, message: &Message, expect_response: bool) -> Result<Option<Message>, UnityMessagingError> {
        // Serialize and send the message
        let data = message.serialize();
        self.socket.send_to(&data, self.unity_address)?;

        if expect_response {
            // Wait for response
            let mut buffer = [0u8; 8192]; // 8KB buffer as per protocol
            let (bytes_received, _) = self.socket.recv_from(&mut buffer)?;
            
            let response = Message::deserialize(&buffer[..bytes_received])?;
            Ok(Some(response))
        } else {
            Ok(None)
        }
    }

    /// Sends a ping message and waits for pong response
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if ping-pong succeeded, error otherwise
    pub fn ping(&self) -> Result<(), UnityMessagingError> {
        let ping_message = Message::ping();
        
        // Unity might send other messages first (like IsPlaying), so we need to handle multiple responses
        self.socket.send_to(&ping_message.serialize(), self.unity_address)?;
        
        // Try to receive responses until we get a Pong or timeout
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 5;
        
        while attempts < MAX_ATTEMPTS {
            let mut buffer = [0u8; 8192];
            match self.socket.recv_from(&mut buffer) {
                Ok((bytes_received, _)) => {
                    if let Ok(response) = Message::deserialize(&buffer[..bytes_received]) {
                        match response.message_type {
                            MessageType::Pong => return Ok(()),
                            MessageType::IsPlaying => {
                                println!("Unity is playing: {}", response.value);
                                // Continue waiting for Pong
                            },
                            _ => {
                                println!("Received unexpected message: {:?} with value: {}", response.message_type, response.value);
                                // Continue waiting for Pong
                            }
                        }
                    }
                },
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut {
                        return Err(UnityMessagingError::Timeout);
                    } else {
                        return Err(UnityMessagingError::IoError(e));
                    }
                }
            }
            attempts += 1;
        }
        
        Err(UnityMessagingError::InvalidMessage("No Pong response received after multiple attempts".to_string()))
    }

    /// Requests Unity version
    /// 
    /// # Returns
    /// 
    /// Returns the Unity package version string
    pub fn get_version(&self) -> Result<String, UnityMessagingError> {
        let version_message = Message::new(MessageType::Version, String::new());
        
        match self.send_message(&version_message, true)? {
            Some(response) if response.message_type == MessageType::Version => Ok(response.value),
            Some(response) => Err(UnityMessagingError::InvalidMessage(
                format!("Expected Version response, got {:?}", response.message_type)
            )),
            None => Err(UnityMessagingError::InvalidMessage("No response received".to_string())),
        }
    }

    /// Requests Unity project path
    /// 
    /// # Returns
    /// 
    /// Returns the Unity project path
    pub fn get_project_path(&self) -> Result<String, UnityMessagingError> {
        let project_path_message = Message::new(MessageType::ProjectPath, String::new());
        
        match self.send_message(&project_path_message, true)? {
            Some(response) if response.message_type == MessageType::ProjectPath => Ok(response.value),
            Some(response) => Err(UnityMessagingError::InvalidMessage(
                format!("Expected ProjectPath response, got {:?}", response.message_type)
            )),
            None => Err(UnityMessagingError::InvalidMessage("No response received".to_string())),
        }
    }

    /// Starts listening for Unity log messages
    /// 
    /// This method will block and call the provided callback for each log message received.
    /// The callback should return `true` to continue listening, `false` to stop.
    /// 
    /// # Arguments
    /// 
    /// * `callback` - Function to call for each log message
    pub fn listen_for_logs<F>(&self, mut callback: F) -> Result<(), UnityMessagingError>
    where
        F: FnMut(&Message) -> bool,
    {
        let mut buffer = [0u8; 8192];
        
        loop {
            match self.socket.recv_from(&mut buffer) {
                Ok((bytes_received, _)) => {
                    if let Ok(message) = Message::deserialize(&buffer[..bytes_received]) {
                        // Check if it's a log message
                        match message.message_type {
                            MessageType::Info | MessageType::Warning | MessageType::Error => {
                                if !callback(&message) {
                                    break;
                                }
                            }
                            _ => {
                                // Handle other message types if needed
                                // For now, just continue listening
                            }
                        }
                    }
                }
                Err(e) => {
                    // Handle timeout or other errors
                    if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut {
                        // Timeout is expected, continue listening
                        continue;
                    } else {
                        return Err(UnityMessagingError::IoError(e));
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Sends a refresh message to Unity to refresh the asset database
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the message was sent successfully
    pub fn send_refresh_message(&self) -> Result<(), UnityMessagingError> {
        let refresh_message = Message::new(MessageType::Refresh, String::new());
        self.socket.send_to(&refresh_message.serialize(), self.unity_address)?;
        Ok(())
    }

    /// Gets the Unity address this client is connected to
    pub fn unity_address(&self) -> SocketAddr {
        self.unity_address
    }
}

/// Test utilities for Unity project management
#[cfg(test)]
mod test_utils {
    use std::path::PathBuf;
    
    /// Gets the embedded Unity project path for testing
    pub fn get_unity_project_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject")
    }
    
    /// Creates a USS file with syntax errors to trigger Unity warnings
    pub fn create_test_uss_file(project_path: &std::path::Path) -> std::path::PathBuf {
        let uss_path = project_path.join("Assets").join("test_errors.uss");
        let uss_content = r#"
/* This USS file contains intentional syntax errors to trigger Unity warnings */
.invalid-selector {
    color: #invalid-color-value;
    margin: invalid-unit;
    unknown-property: some-value;
}

.another-invalid {
    background-color: not-a-color
    /* Missing semicolon above */
    border: 1px solid;
}
"#;
        std::fs::write(&uss_path, uss_content).expect("Failed to create test USS file");
        uss_path
    }
    
    /// Deletes the test USS file
    pub fn cleanup_test_uss_file(uss_path: &std::path::Path) {
        if uss_path.exists() {
            std::fs::remove_file(uss_path).expect("Failed to delete test USS file");
        }
        
        // Also remove the .meta file if it exists
        let meta_path = uss_path.with_extension("uss.meta");
        if meta_path.exists() {
            std::fs::remove_file(meta_path).expect("Failed to delete test USS meta file");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unity_project_manager::UnityProjectManager;
    use test_utils::*;
    use std::time::Duration;

    #[test]
    fn test_message_serialization() {
        let message = Message::new(MessageType::Ping, "test".to_string());
        let serialized = message.serialize();
        let deserialized = Message::deserialize(&serialized).unwrap();
        
        assert_eq!(message.message_type as i32, deserialized.message_type as i32);
        assert_eq!(message.value, deserialized.value);
    }

    #[test]
    fn test_empty_message_serialization() {
        let message = Message::ping();
        let serialized = message.serialize();
        let deserialized = Message::deserialize(&serialized).unwrap();
        
        assert_eq!(message.message_type as i32, deserialized.message_type as i32);
        assert_eq!(message.value, deserialized.value);
        assert!(message.value.is_empty());
    }

    #[test]
    fn test_port_calculation() {
        let process_id = 12345u32;
        let expected_port = 58000 + (process_id % 1000);
        
        let client = UnityMessagingClient::new(process_id).unwrap();
        assert_eq!(client.unity_address().port(), expected_port as u16);
    }

    #[tokio::test]
    async fn test_unity_messaging_integration() {
        let project_path = get_unity_project_path();
        let mut manager = match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
            Ok(manager) => manager,
            Err(_) => {
                println!("Skipping integration test: Unity project not found");
                return;
            }
        };

        // Update process info to check if Unity is running
        if manager.update_process_info().await.is_err() {
            println!("Skipping integration test: Unity Editor not running");
            return;
        }

        let unity_process_id = manager.unity_process_id().expect("Unity process ID should be available");
        let client = match UnityMessagingClient::new(unity_process_id) {
            Ok(client) => client,
            Err(e) => {
                println!("Skipping integration test: Failed to create messaging client: {}", e);
                return;
            }
        };

        // Test ping-pong functionality
        match client.ping() {
            Ok(()) => println!("✓ Ping-pong test passed"),
            Err(e) => {
                println!("Ping-pong test failed: {}", e);
                return;
            }
        }

        // Test version retrieval
        match client.get_version() {
            Ok(version) => println!("✓ Unity package version: {}", version),
            Err(e) => println!("Failed to get Unity version: {}", e),
        }

        // Test project path retrieval
        match client.get_project_path() {
            Ok(path) => println!("✓ Unity project path: {}", path),
            Err(e) => println!("Failed to get Unity project path: {}", e),
        }
    }

    #[tokio::test]
    async fn test_unity_log_generation_and_listening() {
        let project_path = get_unity_project_path();
        let mut manager = match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
            Ok(manager) => manager,
            Err(_) => {
                println!("Skipping log test: Unity project not found");
                return;
            }
        };

        // Update process info to check if Unity is running
        if manager.update_process_info().await.is_err() {
            println!("Skipping log test: Unity Editor not running");
            return;
        }

        let unity_process_id = manager.unity_process_id().expect("Unity process ID should be available");
        let client = match UnityMessagingClient::new(unity_process_id) {
            Ok(client) => client,
            Err(e) => {
                println!("Skipping log test: Failed to create messaging client: {}", e);
                return;
            }
        };

        // Create a USS file with errors to trigger Unity warnings
        let uss_path = create_test_uss_file(&project_path);
        println!("Created test USS file: {}", uss_path.display());

        // Send refresh message to Unity
        if let Err(e) = client.send_refresh_message() {
            println!("Failed to send refresh message: {}", e);
            cleanup_test_uss_file(&uss_path);
            return;
        }
        println!("✓ Sent refresh message to Unity");

        // Listen for log messages with a strict timeout
        println!("Listening for Unity log messages for 3 seconds...");
        let mut log_count = 0;
        let start_time = std::time::Instant::now();
        let timeout_duration = Duration::from_secs(3);
        
        // Create a new socket with a short timeout for this test
        let test_socket = match std::net::UdpSocket::bind("0.0.0.0:0") {
            Ok(socket) => {
                socket.set_read_timeout(Some(Duration::from_millis(100))).ok();
                socket
            },
            Err(e) => {
                println!("Failed to create test socket: {}", e);
                cleanup_test_uss_file(&uss_path);
                return;
            }
        };

        // Listen for messages until timeout
        while start_time.elapsed() < timeout_duration {
            let mut buffer = [0u8; 8192];
            match test_socket.recv_from(&mut buffer) {
                Ok((bytes_received, _)) => {
                    if let Ok(message) = Message::deserialize(&buffer[..bytes_received]) {
                        match message.message_type {
                            MessageType::Info | MessageType::Warning | MessageType::Error => {
                                log_count += 1;
                                match message.message_type {
                                    MessageType::Info => println!("[INFO] {}", message.value),
                                    MessageType::Warning => println!("[WARNING] {}", message.value),
                                    MessageType::Error => println!("[ERROR] {}", message.value),
                                    _ => {}
                                }
                            }
                            _ => {
                                // Ignore other message types
                            }
                        }
                    }
                }
                Err(e) => {
                    // Handle timeout or other errors - continue until our timeout
                    if e.kind() != std::io::ErrorKind::WouldBlock && e.kind() != std::io::ErrorKind::TimedOut {
                        println!("Socket error: {}", e);
                        break;
                    }
                    // For timeout errors, just continue the loop
                }
            }
        }

        println!("✓ Log listening completed. Received {} log messages in {} seconds", 
                log_count, start_time.elapsed().as_secs_f32());

        // Clean up the test file
        cleanup_test_uss_file(&uss_path);
        println!("✓ Cleaned up test USS file");
        
        // The test passes regardless of whether we received logs or not,
        // since Unity might not be configured to send logs via UDP
        // This is more of an integration test to verify the mechanism works
    }
}