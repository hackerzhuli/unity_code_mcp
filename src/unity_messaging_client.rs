use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{UdpSocket, TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::sleep;

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

/// Events that can be broadcast from Unity messaging
#[derive(Debug, Clone)]
pub enum UnityEvent {
    /// Log message received from Unity
    LogMessage {
        level: LogLevel,
        message: String,
    },
    /// Unity version information
    Version(String),
    /// Unity project path
    ProjectPath(String),
    /// Test run started
    TestRunStarted,
    /// Test run finished with parsed test results
    TestRunFinished(TestResultAdaptorContainer),
    /// Test started with parsed test information
    TestStarted(TestAdaptorContainer),
    /// Test finished with parsed test results
    TestFinished(TestResultAdaptorContainer),
    /// Test list retrieved (keeping as string for now)
    TestListRetrieved(String),
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
    #[error("Timeout error")]
    Timeout,
    #[error("Unity process not found")]
    ProcessNotFound,
    #[error("Event listener task failed")]
    TaskFailed,
}

/// Unity messaging client that communicates via UDP with event broadcasting
pub struct UnityMessagingClient {
    socket: Arc<tokio::net::UdpSocket>,
    unity_address: SocketAddr,
    _timeout_duration: Duration,
    event_sender: broadcast::Sender<UnityEvent>,
    listener_task: Option<JoinHandle<()>>,
    last_response_time: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
    is_online: std::sync::Arc<std::sync::Mutex<bool>>,
}

impl UnityMessagingClient {
    /// Creates a new Unity messaging client with event broadcasting
    /// 
    /// # Arguments
    /// 
    /// * `unity_process_id` - The process ID of the Unity Editor
    /// 
    /// # Returns
    /// 
    /// Returns a `Result` containing the client or an error
    pub async fn new(unity_process_id: u32) -> Result<Self, UnityMessagingError> {
        // Calculate Unity's messaging port: 58000 + (ProcessId % 1000)
        let unity_port = 58000 + (unity_process_id % 1000);
        let unity_address = SocketAddr::from(([127, 0, 0, 1], unity_port as u16));

        // Create UDP socket using tokio
        let socket = Arc::new(tokio::net::UdpSocket::bind("0.0.0.0:0").await?); // Bind to any available port

        // Create broadcast channel for events
        let (event_sender, _event_receiver) = broadcast::channel(1000);

        Ok(Self {
            socket,
            unity_address,
            _timeout_duration: Duration::from_secs(5),
            event_sender,
            listener_task: None,
            last_response_time: std::sync::Arc::new(std::sync::Mutex::new(None)),
            is_online: std::sync::Arc::new(std::sync::Mutex::new(false)),
        })
    }

    /// Starts automatic message listening and event broadcasting
    /// 
    /// This method spawns a background task that continuously listens for Unity messages
    /// and broadcasts events to subscribers. Internal messages like ping/pong are handled
    /// automatically without broadcasting events.
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the listener was started successfully
    pub async fn start_listening(&mut self) -> Result<(), UnityMessagingError> {
        if self.listener_task.is_some() {
            return Ok(()); // Already listening
        }

        // Use the shared socket for the background task
        let socket = Arc::clone(&self.socket);
        let unity_address = self.unity_address;
        let event_sender = self.event_sender.clone();
        let last_response_time = self.last_response_time.clone();
        let is_online = self.is_online.clone();

        // Spawn background task for message listening
        let task = tokio::spawn(async move {
            Self::message_listener_task(socket, unity_address, event_sender, last_response_time, is_online).await;
        });

        self.listener_task = Some(task);
        Ok(())
    }

    /// Stops the automatic message listening
    pub fn stop_listening(&mut self) {
        if let Some(task) = self.listener_task.take() {
            task.abort();
        }
    }

    /// Gets a receiver for Unity events
    /// 
    /// # Returns
    /// 
    /// Returns a new broadcast receiver that can be used to listen for Unity events
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<UnityEvent> {
        self.event_sender.subscribe()
    }

    /// Receives a large message via TCP fallback
    /// 
    /// # Arguments
    /// 
    /// * `tcp_port` - The TCP port Unity is listening on
    /// * `expected_length` - The expected length of the message
    /// 
    /// # Returns
    /// 
    /// Returns the received message or an error
    pub(crate) async fn receive_tcp_message(tcp_port: u16, expected_length: usize) -> Result<Message, UnityMessagingError> {
        // Connect to Unity's TCP server
        let tcp_address = SocketAddr::from(([127, 0, 0, 1], tcp_port));
        let mut stream = TcpStream::connect(tcp_address).await?;
        
        // Read the message data
        let mut buffer = vec![0u8; expected_length];
        stream.read_exact(&mut buffer).await?;
        
        // Deserialize the message
        Message::deserialize(&buffer)
    }

    /// Background task that listens for Unity messages and broadcasts events
    async fn message_listener_task(
        socket: Arc<UdpSocket>,
        unity_address: SocketAddr,
        event_sender: broadcast::Sender<UnityEvent>,
        last_response_time: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
        is_online: std::sync::Arc<std::sync::Mutex<bool>>,
    ) {
        let mut buffer = [0u8; 8192];
        let mut ping_interval = tokio::time::interval(Duration::from_secs(1));
        
        loop {
            tokio::select! {
                // Send periodic ping to keep connection alive (Unity times out after 4 seconds)
                _ = ping_interval.tick() => {
                    let ping = Message::ping();
                    if let Err(e) = socket.send_to(&ping.serialize(), unity_address).await {
                        eprintln!("Failed to send ping: {}", e);
                        // Mark Unity as offline when ping fails
                        if let Ok(mut online) = is_online.lock() {
                            *online = false;
                        }
                    } else {
                        // Check if Unity has been unresponsive for too long (5 seconds)
                        let should_mark_offline = if let Ok(last_time) = last_response_time.lock() {
                            if let Some(time) = *last_time {
                                time.elapsed() > Duration::from_secs(5)
                            } else {
                                true // No response ever received
                            }
                        } else {
                            false
                        };
                        
                        if should_mark_offline {
                            if let Ok(mut online) = is_online.lock() {
                                *online = false;
                            }
                        }
                    }
                }
                
                // Listen for incoming messages
                result = socket.recv_from(&mut buffer) => {
                    match result {
                        Ok((bytes_received, _)) => {
                            if let Ok(message) = Message::deserialize(&buffer[..bytes_received]) {
                                // Debug logging for all received messages except Ping and Pong
                                if !matches!(message.message_type, MessageType::Ping | MessageType::Pong) {
                                    if message.value.len() < 100 {
                                        println!("[DEBUG] Received Unity message: {:?} with value: '{}'", message.message_type, message.value);
                                    }else{
                                        println!("[DEBUG] Received Unity message: {:?} with value length: '{}' (value omitted)", message.message_type, message.value.len());
                                    }
                                }
                                
                                // Special logging for log messages
                                if matches!(message.message_type, MessageType::Info | MessageType::Warning | MessageType::Error) {
                                    println!("[LOG] Unity {:?}: {}", message.message_type, message.value);
                                }
                                
                                // Update last response time for any valid message
                                if let Ok(mut time) = last_response_time.lock() {
                                    *time = Some(std::time::Instant::now());
                                }
                                
                                // Update online state based on message type
                                match message.message_type {
                                    MessageType::Online => {
                                        if let Ok(mut online) = is_online.lock() {
                                            //println!("[CLIENT] Received Online message, setting state to true (timestamp: {:?})", std::time::Instant::now());
                                            *online = true;
                                        }
                                    }
                                    MessageType::Offline => {
                                        if let Ok(mut online) = is_online.lock() {
                                            //println!("[CLIENT] Received Offline message, setting state to false (timestamp: {:?})", std::time::Instant::now());
                                            *online = false;
                                        }
                                    }
                                    _ => {
                                        // Any other message from Unity means it's online
                                        if let Ok(mut online) = is_online.lock() {
                                            *online = true;
                                        }
                                    }
                                }
                                
                                // Handle TCP fallback for large messages
                                if message.message_type == MessageType::Tcp {
                                    // Parse port and length from message value: "<port>:<length>"
                                    if let Some((port_str, length_str)) = message.value.split_once(':') {
                                        if let (Ok(tcp_port), Ok(expected_length)) = (port_str.parse::<u16>(), length_str.parse::<usize>()) {
                                            println!("[DEBUG] Received TCP fallback notification: port={}, length={}", tcp_port, expected_length);
                                            
                                            // Connect to Unity's TCP server and receive the large message
                                            match Self::receive_tcp_message(tcp_port, expected_length).await {
                                                Ok(large_message) => {
                                                    println!("[DEBUG] Successfully received large message via TCP: {:?} with {} bytes", large_message.message_type, large_message.value.len());
                                                    
                                                    // Process the large message normally
                                                    if let Some(event) = Self::message_to_event(&large_message) {
                                                        if let Err(_) = event_sender.send(event) {
                                                            // No receivers, continue listening
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    eprintln!("Failed to receive TCP message: {}", e);
                                                }
                                            }
                                        } else {
                                            eprintln!("Invalid TCP message format: {}", message.value);
                                        }
                                    } else {
                                        eprintln!("Invalid TCP message format: {}", message.value);
                                    }
                                } else {
                                    // Handle the message and potentially broadcast an event
                                    if let Some(event) = Self::message_to_event(&message) {
                                        if let Err(_) = event_sender.send(event) {
                                            // No receivers, continue listening
                                        }
                                    }
                                }
                                
                                // Handle internal messages (ping/pong)
                                if message.message_type == MessageType::Ping {
                                    let pong = Message::pong();
                                    if let Err(e) = socket.send_to(&pong.serialize(), unity_address).await {
                                        eprintln!("Failed to send pong response: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Socket error in message listener: {}", e);
                            // Mark Unity as offline when socket errors occur
                            if let Ok(mut online) = is_online.lock() {
                                *online = false;
                            }
                            // Try to continue listening instead of breaking immediately
                            // Unity might reconnect after compilation
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
            }
        }
    }

    /// Converts a Unity message to an event (if it should be broadcast)
    fn message_to_event(message: &Message) -> Option<UnityEvent> {
        match message.message_type {
            // Log messages
            MessageType::Info => Some(UnityEvent::LogMessage {
                level: LogLevel::Info,
                message: message.value.clone(),
            }),
            MessageType::Warning => Some(UnityEvent::LogMessage {
                level: LogLevel::Warning,
                message: message.value.clone(),
            }),
            MessageType::Error => Some(UnityEvent::LogMessage {
                level: LogLevel::Error,
                message: message.value.clone(),
            }),
            
            // Unity state messages
            MessageType::Version => Some(UnityEvent::Version(message.value.clone())),
            MessageType::ProjectPath => Some(UnityEvent::ProjectPath(message.value.clone())),
            MessageType::Online => Some(UnityEvent::Online),
            MessageType::Offline => Some(UnityEvent::Offline),
            MessageType::IsPlaying => {
                let is_playing = message.value.to_lowercase() == "true";
                Some(UnityEvent::IsPlaying(is_playing))
            },
            
            // Test messages
            MessageType::TestRunStarted => Some(UnityEvent::TestRunStarted),
            MessageType::TestRunFinished => {
                match serde_json::from_str::<TestResultAdaptorContainer>(&message.value) {
                    Ok(container) => Some(UnityEvent::TestRunFinished(container)),
                    Err(e) => {
                        eprintln!("[DEBUG] Failed to deserialize TestRunFinished data: {}", e);
                        eprintln!("[DEBUG] Raw TestRunFinished data: {}", message.value);
                        None
                    }
                }
            },
            MessageType::TestStarted => {
                match serde_json::from_str::<TestAdaptorContainer>(&message.value) {
                    Ok(container) => Some(UnityEvent::TestStarted(container)),
                    Err(e) => {
                        eprintln!("[DEBUG] Failed to deserialize TestStarted data: {}", e);
                        eprintln!("[DEBUG] Raw TestStarted data: {}", message.value);
                        None
                    }
                }
            },
            MessageType::TestFinished => {
                match serde_json::from_str::<TestResultAdaptorContainer>(&message.value) {
                    Ok(container) => Some(UnityEvent::TestFinished(container)),
                    Err(e) => {
                        eprintln!("[DEBUG] Failed to deserialize TestFinished data: {}", e);
                        eprintln!("[DEBUG] Raw TestFinished data: {}", message.value);
                        None
                    }
                }
            },
            MessageType::TestListRetrieved => Some(UnityEvent::TestListRetrieved(message.value.clone())),
            
            // Compilation messages
            MessageType::CompilationFinished => Some(UnityEvent::CompilationFinished),
            MessageType::CompilationStarted => Some(UnityEvent::CompilationStarted),
            
            // Refresh messages
            MessageType::Refresh => Some(UnityEvent::RefreshCompleted(message.value.clone())),
            
            // Internal messages (don't broadcast)
            MessageType::Ping | MessageType::Pong | MessageType::Tcp => None,
            
            // Other messages (don't broadcast for now)
            _ => None,
        }
    }

    /// Sends a message to Unity (internal use)
    /// 
    /// # Arguments
    /// 
    /// * `message` - The message to send
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the message was sent successfully
    async fn send_message_internal(&self, message: &Message) -> Result<(), UnityMessagingError> {
        // Debug logging for outgoing messages except Ping and Pong
        if !matches!(message.message_type, MessageType::Ping | MessageType::Pong) {
            println!("[DEBUG] Sending Unity message: {:?} with value: '{}'", message.message_type, message.value);
        }
        
        // Serialize and send the message
        let data = message.serialize();
        match self.socket.send_to(&data, self.unity_address).await {
            Ok(_) => Ok(()),
            Err(e) => {
                // Mark Unity as offline when send fails
                if let Ok(mut online) = self.is_online.lock() {
                    *online = false;
                }
                Err(e.into())
            }
        }
    }

    /// Determines if a message type requires stable delivery (should wait for Unity to be online)
    fn requires_stable_delivery(message_type: MessageType) -> bool {
        match message_type {
            // Messages that can be sent immediately (don't require stable delivery)
            MessageType::Ping | MessageType::Pong => false,
            
            // All other messages require stable delivery
            _ => true,
        }
    }

    /// Sends a message to Unity with automatic waiting for Unity to be online
    /// 
    /// This method will wait for Unity to be online before sending the message,
    /// but only for messages that require stable delivery. Messages like Ping, Pong,
    /// and Refresh are sent immediately regardless of Unity's online status.
    /// 
    /// # Arguments
    /// 
    /// * `message` - The message to send
    /// * `timeout_seconds` - Maximum time to wait for Unity to be online (default: 30 seconds)
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the message was sent successfully
    pub async fn send_message(&self, message: &Message, timeout_seconds: Option<u64>) -> Result<(), UnityMessagingError> {
        // Check if this message type requires stable delivery
        if Self::requires_stable_delivery(message.message_type) {
            let timeout = Duration::from_secs(timeout_seconds.unwrap_or(30));
            let start_time = Instant::now();
            
            // Wait for Unity to be online
            while !self.is_online() {
                if start_time.elapsed() >= timeout {
                    return Err(UnityMessagingError::Timeout);
                }
                
                // Sleep for a short time before checking again
                sleep(Duration::from_millis(100)).await;
            }
        }
        
        // Send the message (either immediately or after Unity is online)
        self.send_message_internal(message).await
    }

    /// Checks if Unity is currently connected and responsive
    /// 
    /// # Arguments
    /// 
    /// * `timeout_seconds` - Maximum age of last response to consider Unity connected (default: 10 seconds)
    /// 
    /// # Returns
    /// 
    /// Returns `true` if Unity has responded within the timeout period, `false` otherwise
    pub fn is_connected(&self, timeout_seconds: Option<u64>) -> bool {
        let timeout = Duration::from_secs(timeout_seconds.unwrap_or(10));
        
        if let Ok(last_time) = self.last_response_time.lock() {
            if let Some(time) = *last_time {
                return time.elapsed() <= timeout;
            }
        }
        false
    }

    /// Requests Unity version
    /// 
    /// This method sends a version request to Unity. The response will be available
    /// through the event system as a Version event.
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the request was sent successfully
    pub async fn get_version(&self) -> Result<(), UnityMessagingError> {
        let version_message = Message::new(MessageType::Version, String::new());
        self.send_message(&version_message, None).await
    }

    /// Requests Unity project path
    /// 
    /// This method sends a project path request to Unity. The response will be available
    /// through the event system as a ProjectPath event.
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the request was sent successfully
    pub async fn get_project_path(&self) -> Result<(), UnityMessagingError> {
        let project_path_message = Message::new(MessageType::ProjectPath, String::new());
        self.send_message(&project_path_message, None).await
    }

    /// Sends a ping message to Unity
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the ping was sent successfully
    pub async fn send_ping(&self) -> Result<(), UnityMessagingError> {
        let ping_message = Message::ping();
        self.send_message(&ping_message, None).await
    }

    /// Sends a refresh message to Unity to refresh the asset database
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the message was sent successfully
    pub async fn send_refresh_message(&self, timeout_seconds: Option<u64>) -> Result<(), UnityMessagingError> {
        let refresh_message = Message::new(MessageType::Refresh, String::new());
        //println!("[DEBUG] Sending refresh message to Unity at {}", self.unity_address);
        self.send_message(&refresh_message, timeout_seconds).await
    }

    /// Requests the list of available tests for the specified test mode
    /// 
    /// This method sends a test list request to Unity. The response will be available
    /// through the event system as a TestListRetrieved event.
    /// 
    /// # Arguments
    /// 
    /// * `test_mode` - The test mode ("EditMode" or "PlayMode")
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the request was sent successfully
    pub async fn retrieve_test_list(&self, test_mode: &str) -> Result<(), UnityMessagingError> {
        let test_list_message = Message::new(MessageType::RetrieveTestList, test_mode.to_string());
        self.send_message(&test_list_message, None).await
    }

    /// Executes tests based on the specified filter
    /// 
    /// This method sends a test execution request to Unity. Test events will be available
    /// through the event system (TestRunStarted, TestStarted, TestFinished, TestRunFinished).
    /// 
    /// # Arguments
    /// 
    /// * `filter` - The test filter (e.g., "EditMode", "PlayMode:MyTests.dll", "EditMode:MyNamespace.MyTestClass")
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the request was sent successfully
    pub async fn execute_tests(&self, filter: &str) -> Result<(), UnityMessagingError> {
        let execute_tests_message = Message::new(MessageType::ExecuteTests, filter.to_string());
        self.send_message(&execute_tests_message, None).await
    }

    /// Gets the Unity address this client is connected to
    pub fn unity_address(&self) -> SocketAddr {
        self.unity_address
    }

    /// Checks if Unity is currently online
    /// 
    /// Unity is considered online if:
    /// - We have received at least one message from Unity, AND
    /// - We haven't received an explicit Offline message
    /// 
    /// # Returns
    /// 
    /// Returns `true` if Unity is online, `false` otherwise
    pub fn is_online(&self) -> bool {
        if let Ok(online) = self.is_online.lock() {
            *online
        } else {
            false
        }
    }
}

/// Cleanup when the client is dropped
impl Drop for UnityMessagingClient {
    fn drop(&mut self) {
        self.stop_listening();
    }
}

#[cfg(test)]
#[path ="unity_messaging_client_tests.rs"]
mod unity_messaging_client_tests;