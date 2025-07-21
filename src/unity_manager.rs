use std::collections::{VecDeque, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, broadcast};
use tokio::time::timeout;

use crate::unity_messaging_client::{UnityMessagingClient, UnityEvent, UnityMessagingError, LogLevel};
use crate::unity_project_manager::UnityProjectManager;

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



    /// Stop the messaging client and cleanup
    pub fn shutdown(&mut self) {
        if let Some(client) = &mut self.messaging_client {
            client.stop_listening();
        }
        self.is_listening = false;
        self.messaging_client = None;
        self.event_receiver = None;
    }
}

impl Drop for UnityManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}