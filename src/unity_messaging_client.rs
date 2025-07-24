use crate::unity_messages::{
    LogContainer, LogLevel, Message, MessageType, TestAdaptorContainer, TestFilter, TestResultAdaptorContainer,
    UnityEvent, UnityMessagingError,
};
use crate::{debug_log, error_log, info_log};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::sleep;

/// Unity messaging client that communicates via UDP with event broadcasting
pub struct UnityMessagingClient {
    socket: Arc<tokio::net::UdpSocket>,
    unity_address: SocketAddr,
    _timeout_duration: Duration,
    event_sender: broadcast::Sender<UnityEvent>,
    listener_task: Option<JoinHandle<()>>,
    last_response_time: Arc<Mutex<Option<std::time::Instant>>>,
    is_online: Arc<Mutex<bool>>,
    /// ID of the current test run in Unity Editor
    current_test_run_id: Arc<Mutex<Option<String>>>,
    /// Whether Unity Editor is currently in play mode
    is_in_play_mode: Arc<Mutex<bool>>,
    /// Timestamp of the last compilation finished event
    last_compilation_finished: Arc<Mutex<Option<SystemTime>>>,
    /// Compile errors collected after compilation finishes
    last_compile_errors: Arc<Mutex<Vec<String>>>,
    /// Whether we have received the first message from Unity (used to trigger initial GetCompileErrors)
    first_message_received: Arc<Mutex<bool>>,
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
        let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?); // Bind to any available port

        // Create broadcast channel for events
        let (event_sender, _event_receiver) = broadcast::channel(1000);

        Ok(Self {
            socket,
            unity_address,
            _timeout_duration: Duration::from_secs(5),
            event_sender,
            listener_task: None,
            last_response_time: Arc::new(Mutex::new(None)),
            is_online: Arc::new(Mutex::new(false)),
            current_test_run_id: Arc::new(Mutex::new(None)),
            is_in_play_mode: Arc::new(Mutex::new(false)),
            last_compilation_finished: Arc::new(Mutex::new(None)),
            last_compile_errors: Arc::new(Mutex::new(Vec::new())),
            first_message_received: Arc::new(Mutex::new(false)),
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
        let current_test_run_id = self.current_test_run_id.clone();
        let is_in_play_mode = self.is_in_play_mode.clone();
        let first_message_received = self.first_message_received.clone();

        // Spawn background task for message listening
        let task = tokio::spawn(async move {
            Self::message_listener_task(
                socket,
                unity_address,
                event_sender,
                last_response_time,
                is_online,
                current_test_run_id,
                is_in_play_mode,
                first_message_received
            )
            .await;
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
    pub(crate) async fn receive_tcp_message(
        tcp_port: u16,
        expected_length: usize,
    ) -> Result<Message, UnityMessagingError> {
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
        last_response_time: Arc<Mutex<Option<std::time::Instant>>>,
        is_online: Arc<Mutex<bool>>,
        current_test_run_id: Arc<Mutex<Option<String>>>,
        is_in_play_mode: Arc<Mutex<bool>>,
        first_message_received: Arc<Mutex<bool>>,
    ) {
        let mut buffer = [0u8; 8192];
        let mut ping_interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                // Send periodic ping to keep connection alive (Unity times out after 4 seconds)
                _ = ping_interval.tick() => {
                    let ping = Message::ping();
                    if let Err(e) = socket.send_to(&ping.serialize(), unity_address).await {
                        error_log!("Failed to send ping: {}", e);
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
                                        debug_log!("Received Unity message: {:?} with value: '{}'", message.message_type, message.value);
                                    }else{
                                        debug_log!("Received Unity message: {:?} with value length: '{}' (value omitted)", message.message_type, message.value.len());
                                    }
                                }

                                // Special logging for log messages
                                if matches!(message.message_type, MessageType::Info | MessageType::Warning | MessageType::Error) {
                                    info_log!("Unity {:?}: {}", message.message_type, message.value);
                                }

                                let mut just_received_first_message = false;
                                // Check if this is the first message from Unity (excluding Ping/Pong)
                                if let Ok(mut first_received) = first_message_received.lock() {
                                    if !*first_received {
                                        *first_received = true;
                                        debug_log!("First message received from Unity, requesting initial compile errors");
                                        just_received_first_message = true;
                                    }
                                }
                                
                                if just_received_first_message {
                                    // Send GetCompileErrors request to get initial compile errors
                                    let get_compile_errors_message = Message::new(MessageType::GetCompileErrors, String::new());
                                    if let Err(e) = socket.send_to(&get_compile_errors_message.serialize(), unity_address).await {
                                        error_log!("Failed to send initial GetCompileErrors request: {}", e);
                                    }
                                }

                                // Update last response time for any valid message
                                if let Ok(mut time) = last_response_time.lock() {
                                    *time = Some(std::time::Instant::now());
                                }

                                // Update online state based on message type
                                match message.message_type {
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
                                            debug_log!("Received TCP fallback notification: port={}, length={}", tcp_port, expected_length);

                                            // Connect to Unity's TCP server and receive the large message
                                            match Self::receive_tcp_message(tcp_port, expected_length).await {
                                                Ok(large_message) => {
                                                    debug_log!("Successfully received large message via TCP: {:?} with {} bytes", large_message.message_type, large_message.value.len());

                                                    // Process the large message normally
                                                    if let Some(event) = Self::message_to_event(&large_message) {
                                                        if let Err(_) = event_sender.send(event) {
                                                            // No receivers, continue listening
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    error_log!("Failed to receive TCP message: {}", e);
                                                }
                                            }
                                        } else {
                                            error_log!("Invalid TCP message format: {}", message.value);
                                        }
                                    } else {
                                        error_log!("Invalid TCP message format: {}", message.value);
                                    }
                                } else {
                                    // Handle the message and potentially broadcast an event
                                    if let Some(event) = Self::message_to_event(&message) {
                                        // Update internal state based on the event
                                        match &event {
                                            UnityEvent::TestRunStarted(container) => {
                                                 if let Ok(mut test_run_id) = current_test_run_id.lock() {
                                                     if let Some(first_adaptor) = container.test_adaptors.first() {
                                                         *test_run_id = Some(first_adaptor.id.clone());
                                                     }
                                                 }
                                            }
                                            UnityEvent::TestRunFinished(container) => {
                                                if let Some(first_adaptor) = container.test_result_adaptors.first() {
                                                    if let Ok(mut test_run_id) = current_test_run_id.lock() {
                                                        if *test_run_id == Some(first_adaptor.test_id.clone()) {
                                                            *test_run_id = None;
                                                        }
                                                    }
                                                }
                                            }
                                            UnityEvent::IsPlaying(is_playing) => {
                                                if let Ok(mut play_mode) = is_in_play_mode.lock() {
                                                    *play_mode = *is_playing;
                                                }
                                            }
                                            _ => {}
                                        }

                                        if let Err(_) = event_sender.send(event) {
                                            // No receivers, continue listening
                                        }
                                    }
                                }

                                // Handle internal messages (ping/pong)
                                if message.message_type == MessageType::Ping {
                                    let pong = Message::pong();
                                    if let Err(e) = socket.send_to(&pong.serialize(), unity_address).await {
                                        error_log!("Failed to send pong response: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error_log!("Socket error in message listener: {}", e);
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
            MessageType::Version => Some(UnityEvent::PackageVersion(message.value.clone())),
            MessageType::ProjectPath => Some(UnityEvent::ProjectPath(message.value.clone())),
            MessageType::Online => Some(UnityEvent::Online),
            MessageType::Offline => Some(UnityEvent::Offline),
            MessageType::IsPlaying => {
                let is_playing = message.value.to_lowercase() == "true";
                Some(UnityEvent::IsPlaying(is_playing))
            }

            // Test messages
            MessageType::TestRunStarted => {
                match serde_json::from_str::<TestAdaptorContainer>(&message.value) {
                    Ok(container) => Some(UnityEvent::TestRunStarted(container)),
                    Err(e) => {
                        error_log!("Failed to deserialize TestRunStarted data: {}", e);
                        debug_log!("Raw TestRunStarted data: {}", message.value);
                        None
                    }
                }
            }
            MessageType::TestRunFinished => {
                match serde_json::from_str::<TestResultAdaptorContainer>(&message.value) {
                    Ok(container) => Some(UnityEvent::TestRunFinished(container)),
                    Err(e) => {
                        error_log!("Failed to deserialize TestRunFinished data: {}", e);
                        debug_log!("Raw TestRunFinished data: {}", message.value);
                        None
                    }
                }
            }
            MessageType::TestStarted => {
                match serde_json::from_str::<TestAdaptorContainer>(&message.value) {
                    Ok(container) => Some(UnityEvent::TestStarted(container)),
                    Err(e) => {
                        error_log!("Failed to deserialize TestStarted data: {}", e);
                        debug_log!("Raw TestStarted data: {}", message.value);
                        None
                    }
                }
            }
            MessageType::TestFinished => {
                match serde_json::from_str::<TestResultAdaptorContainer>(&message.value) {
                    Ok(container) => Some(UnityEvent::TestFinished(container)),
                    Err(e) => {
                        error_log!("Failed to deserialize TestFinished data: {}", e);
                        debug_log!("Raw TestFinished data: {}", message.value);
                        None
                    }
                }
            }

            // Compilation messages
            MessageType::CompilationFinished => Some(UnityEvent::CompilationFinished),
            MessageType::CompilationStarted => Some(UnityEvent::CompilationStarted),
            MessageType::GetCompileErrors => {
                match serde_json::from_str::<LogContainer>(&message.value) {
                    Ok(container) => Some(UnityEvent::CompileErrors(container)),
                    Err(e) => {
                        error_log!("Failed to deserialize GetCompileErrors data: {}", e);
                        debug_log!("Raw GetCompileErrors data: {}", message.value);
                        None
                    }
                }
            }

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
            debug_log!(
                "Sending Unity message: {:?} with value: '{}'",
                message.message_type,
                message.value
            );
        }

        // Serialize and send the message
        let data = message.serialize();
        match self.socket.send_to(&data, self.unity_address).await {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
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
    pub async fn send_message(
        &self,
        message: &Message,
        timeout_seconds: Option<u64>,
    ) -> Result<(), UnityMessagingError> {
        // Check if this message type requires stable delivery
        if Self::requires_stable_delivery(message.message_type) {
            let timeout = Duration::from_secs(timeout_seconds.unwrap_or(30));
            let start_time = Instant::now();

            // Wait for Unity to be online
            while !self.is_online() {
                if start_time.elapsed() >= timeout {
                    return Err(UnityMessagingError::Timeout(format!(
                        "Timeout waiting for Unity Editor to be available after {:?}, Unity Editor is busy compiling scripts, please try again later.",
                        timeout
                    )));
                }

                // Sleep for a short time before checking again
                sleep(Duration::from_millis(100)).await;
            }
        }

        // Send the message (either immediately or after Unity is online)
        self.send_message_internal(message).await
    }

    /// Requests Unity package version (not Unity Editor version)
    ///
    /// This method sends a version request to Unity. The response will be available
    /// through the event system as a PackageVersion event.
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

    /// Sends a refresh message to Unity to refresh the asset database
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the message was sent successfully
    pub async fn send_refresh_message(
        &self,
        timeout_seconds: Option<u64>,
    ) -> Result<(), UnityMessagingError> {
        let refresh_message = Message::new(MessageType::Refresh, String::new());
        //println!("[DEBUG] Sending refresh message to Unity at {}", self.unity_address);
        self.send_message(&refresh_message, timeout_seconds).await
    }

    /// Requests compile errors from Unity
    ///
    /// This method sends a compile errors request to Unity. The response will be available
    /// through the event system as a CompileErrors event.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the request was sent successfully
    pub async fn get_compile_errors(&self) -> Result<(), UnityMessagingError> {
        let compile_errors_message = Message::new(MessageType::GetCompileErrors, String::new());
        self.send_message(&compile_errors_message, None).await
    }

    /// Executes tests based on the specified filter
    ///
    /// This method sends a test execution request to Unity. Test events will be available
    /// through the event system (TestRunStarted, TestStarted, TestFinished, TestRunFinished).
    ///
    /// # Arguments
    ///
    /// * `filter` - The test filter specifying which tests to execute
    /// * `timeout_seconds` - Maximum time to wait for Unity to be online (default: 30 seconds)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the request was sent successfully
    pub async fn execute_tests(
        &self,
        filter: TestFilter,
        timeout_seconds: Option<u64>,
    ) -> Result<(), UnityMessagingError> {
        let filter_string = filter.to_filter_string();
        let execute_tests_message = Message::new(MessageType::ExecuteTests, filter_string);
        self.send_message(&execute_tests_message, timeout_seconds)
            .await
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

    /// Checks if Unity is currently running tests
    ///
    /// # Returns
    ///
    /// Returns `true` if Unity is currently running tests, `false` otherwise
    pub fn is_running_tests(&self) -> bool {
        if let Ok(test_run_id) = self.current_test_run_id.lock() {
            test_run_id.is_some()
        } else {
            false
        }
    }

    /// Checks if Unity is currently in play mode
    ///
    /// # Returns
    ///
    /// Returns `true` if Unity is in play mode, `false` otherwise
    pub fn is_in_play_mode(&self) -> bool {
        if let Ok(play_mode) = self.is_in_play_mode.lock() {
            *play_mode
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
#[path = "unity_messaging_client_tests.rs"]
mod unity_messaging_client_tests;
