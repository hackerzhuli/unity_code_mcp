use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::net::UdpSocket;

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
    /// Test run finished
    TestRunFinished,
    /// Test started
    TestStarted(String),
    /// Test finished
    TestFinished(String),
    /// Test list retrieved
    TestListRetrieved(String),
    /// Compilation finished
    CompilationFinished,
    /// Unity went online
    Online,
    /// Unity went offline
    Offline,
    /// Unity play mode changed
    IsPlaying(bool),
}

/// Log levels for Unity log messages
#[derive(Debug, Clone, PartialEq, Eq)]
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
    socket: tokio::net::UdpSocket,
    unity_address: SocketAddr,
    _timeout_duration: Duration,
    event_sender: broadcast::Sender<UnityEvent>,
    listener_task: Option<JoinHandle<()>>,
    last_response_time: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
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
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?; // Bind to any available port

        // Create broadcast channel for events
        let (event_sender, _event_receiver) = broadcast::channel(1000);

        Ok(Self {
            socket,
            unity_address,
            _timeout_duration: Duration::from_secs(5),
            event_sender,
            listener_task: None,
            last_response_time: std::sync::Arc::new(std::sync::Mutex::new(None)),
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

        // Create a new socket for the background task (since we can't clone tokio sockets)
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        let unity_address = self.unity_address;
        let event_sender = self.event_sender.clone();
        let last_response_time = self.last_response_time.clone();

        // Spawn background task for message listening
        let task = tokio::spawn(async move {
            Self::message_listener_task(socket, unity_address, event_sender, last_response_time).await;
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

    /// Background task that listens for Unity messages and broadcasts events
    async fn message_listener_task(
        socket: UdpSocket,
        unity_address: SocketAddr,
        event_sender: broadcast::Sender<UnityEvent>,
        last_response_time: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
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
                    }
                }
                
                // Listen for incoming messages
                result = socket.recv_from(&mut buffer) => {
                    match result {
                        Ok((bytes_received, _)) => {
                            if let Ok(message) = Message::deserialize(&buffer[..bytes_received]) {
                                // Update last response time for any valid message
                                if let Ok(mut time) = last_response_time.lock() {
                                    *time = Some(std::time::Instant::now());
                                }
                                
                                // Handle the message and potentially broadcast an event
                                if let Some(event) = Self::message_to_event(&message) {
                                    if let Err(_) = event_sender.send(event) {
                                        // No receivers, continue listening
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
                            break;
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
            MessageType::TestRunFinished => Some(UnityEvent::TestRunFinished),
            MessageType::TestStarted => Some(UnityEvent::TestStarted(message.value.clone())),
            MessageType::TestFinished => Some(UnityEvent::TestFinished(message.value.clone())),
            MessageType::TestListRetrieved => Some(UnityEvent::TestListRetrieved(message.value.clone())),
            
            // Compilation messages
            MessageType::CompilationFinished => Some(UnityEvent::CompilationFinished),
            
            // Internal messages (don't broadcast)
            MessageType::Ping | MessageType::Pong => None,
            
            // Other messages (don't broadcast for now)
            _ => None,
        }
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
    pub async fn send_message(&self, message: &Message, expect_response: bool) -> Result<Option<Message>, UnityMessagingError> {
        // Serialize and send the message
        let data = message.serialize();
        self.socket.send_to(&data, self.unity_address).await?;

        if expect_response {
            // Wait for response with timeout
            let mut buffer = [0u8; 8192]; // 8KB buffer as per protocol
            let (bytes_received, _) = tokio::time::timeout(
                Duration::from_secs(5),
                self.socket.recv_from(&mut buffer)
            ).await.map_err(|_| UnityMessagingError::Timeout)??;
            
            let response = Message::deserialize(&buffer[..bytes_received])?;
            Ok(Some(response))
        } else {
            Ok(None)
        }
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
    /// # Returns
    /// 
    /// Returns the Unity package version string
    pub async fn get_version(&self) -> Result<String, UnityMessagingError> {
        let version_message = Message::new(MessageType::Version, String::new());
        
        match self.send_message(&version_message, true).await? {
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
    pub async fn get_project_path(&self) -> Result<String, UnityMessagingError> {
        let project_path_message = Message::new(MessageType::ProjectPath, String::new());
        
        match self.send_message(&project_path_message, true).await? {
            Some(response) if response.message_type == MessageType::ProjectPath => Ok(response.value),
            Some(response) => Err(UnityMessagingError::InvalidMessage(
                format!("Expected ProjectPath response, got {:?}", response.message_type)
            )),
            None => Err(UnityMessagingError::InvalidMessage("No response received".to_string())),
        }
    }

    /// Sends a refresh message to Unity to refresh the asset database
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the message was sent successfully
    pub async fn send_refresh_message(&self) -> Result<(), UnityMessagingError> {
        let refresh_message = Message::new(MessageType::Refresh, String::new());
        self.socket.send_to(&refresh_message.serialize(), self.unity_address).await?;
        Ok(())
    }

    /// Gets the Unity address this client is connected to
    pub fn unity_address(&self) -> SocketAddr {
        self.unity_address
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