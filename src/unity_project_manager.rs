use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use sysinfo::{System, Pid, ProcessesToUpdate, ProcessRefreshKind};
use tokio::fs as async_fs;

#[derive(Debug, Clone)]
pub struct UnityProjectManager {
    project_path: PathBuf,
    unity_version: Option<String>,
    unity_process_id: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
struct EditorInstance {
    process_id: u32,
    version: String,
    app_path: String,
    app_contents_path: String,
}

#[derive(Debug, Deserialize)]
struct ProjectVersion {
    #[serde(rename = "m_EditorVersion")]
    editor_version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum UnityProjectError {
    #[error("Not a Unity project: {0}")]
    NotUnityProject(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Unity version not found")]
    VersionNotFound,
    #[error("Unity process not found")]
    ProcessNotFound,
}

impl UnityProjectManager {
    /// Creates a new UnityProjectManager with the given project path.
    /// 
    /// This method initializes the manager and attempts to load the Unity version.
    /// The process ID will be None initially and should be updated using `update_process_info()`.
    /// 
    /// # Arguments
    /// 
    /// * `project_path` - A string representing the absolute path to the Unity project directory
    /// 
    /// # Examples
    /// 
    /// ```
    /// use unity_code_mcp::UnityProjectManager;
    /// 
    /// let manager = UnityProjectManager::new("/path/to/unity/project".to_string());
    /// ```
    pub async fn new(project_path: String) -> Result<Self, UnityProjectError> {
        let project_path = PathBuf::from(project_path);
        let mut manager = Self {
            project_path,
            unity_version: None,
            unity_process_id: None,
        };
        
        // Load Unity version during initialization
        if manager.is_unity_project() {
            manager.unity_version = Some(manager.load_unity_editor_version().await?);
        } else {
            return Err(UnityProjectError::NotUnityProject(format!(
                "Path '{}' is not a Unity project",
                manager.project_path.display()
            )));
        }
        
        Ok(manager)
    }

    /// Checks if the configured path is a valid Unity project.
    /// 
    /// A valid Unity project must contain:
    /// - `ProjectSettings/ProjectVersion.txt` file
    /// - `Assets/` directory
    /// - `Packages/` directory
    fn is_unity_project(&self) -> bool {
        let project_version_path = self
            .project_path
            .join("ProjectSettings")
            .join("ProjectVersion.txt");
        let assets_path = self.project_path.join("Assets");
        let packages_path = self.project_path.join("Packages");

        project_version_path.exists() && assets_path.exists() && packages_path.exists()
    }

    /// Loads the Unity Editor version from the project's ProjectVersion.txt file.
    /// 
    /// This is a private method used during initialization to cache the Unity version.
    async fn load_unity_editor_version(&self) -> Result<String, UnityProjectError> {
        let project_version_path = self
            .project_path
            .join("ProjectSettings")
            .join("ProjectVersion.txt");
        let content = async_fs::read_to_string(project_version_path).await?;

        // Parse the YAML content to extract the version
        let project_version: ProjectVersion = serde_yaml::from_str(&content)
            .map_err(|_| UnityProjectError::VersionNotFound)?;
        
        Ok(project_version.editor_version)
    }

    /// Returns the cached Unity Editor version.
    /// 
    /// # Returns
    /// 
    /// Returns `Some(String)` with the Unity Editor version if available, `None` otherwise.
    /// 
    /// # Examples
    /// 
    /// ```
    /// use unity_code_mcp::UnityProjectManager;
    /// 
    /// #[tokio::main]
    /// async fn main() {
    ///     let manager = UnityProjectManager::new("/path/to/unity/project".to_string()).await.unwrap();
    ///     if let Some(version) = manager.unity_version() {
    ///         println!("Unity version: {}", version);
    ///     }
    /// }
    /// ```
    pub fn unity_version(&self) -> Option<&String> {
        self.unity_version.as_ref()
    }



    /// Updates the Unity Editor process information by reading the current EditorInstance.json file.
    /// 
    /// This method should be called periodically to keep the process ID information up to date.
    /// It reads the `Library/EditorInstance.json` file and verifies that the process is actually running.
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the process information was successfully updated, or an error if:
    /// - The EditorInstance.json file doesn't exist (no Unity Editor is running)
    /// - The JSON content cannot be parsed
    /// - The process ID in the file doesn't correspond to a running Unity Editor process
    /// 
    /// # Examples
    /// 
    /// ```
    /// use unity_code_mcp::UnityProjectManager;
    /// 
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut manager = UnityProjectManager::new("/path/to/unity/project".to_string()).await.unwrap();
    ///     match manager.update_process_info().await {
    ///         Ok(()) => println!("Process info updated"),
    ///         Err(e) => eprintln!("No Unity Editor running: {}", e),
    ///     }
    /// }
    /// ```
    pub async fn update_process_info(&mut self) -> Result<(), UnityProjectError> {
        let editor_instance_path = self
            .project_path
            .join("Library")
            .join("EditorInstance.json");

        if !editor_instance_path.exists() {
            self.unity_process_id = None;
            return Err(UnityProjectError::ProcessNotFound);
        }

        let content = async_fs::read_to_string(editor_instance_path).await?;
        let editor_instance: EditorInstance = serde_json::from_str(&content)?;

        // Verify that the process is actually running and is Unity.exe
        if self.is_unity_process_running(editor_instance.process_id) {
            self.unity_process_id = Some(editor_instance.process_id);
            Ok(())
        } else {
            self.unity_process_id = None;
            Err(UnityProjectError::ProcessNotFound)
        }
    }

    /// Returns the cached Unity Editor process ID.
    /// 
    /// # Returns
    /// 
    /// Returns `Some(u32)` with the process ID if a Unity Editor is currently running for this project,
    /// `None` otherwise. Call `update_process_info()` to refresh this information.
    /// 
    /// # Examples
    /// 
    /// ```
    /// use unity_code_mcp::UnityProjectManager;
    /// 
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut manager = UnityProjectManager::new("/path/to/unity/project".to_string()).await.unwrap();
    ///     manager.update_process_info().await.ok();
    ///     if let Some(pid) = manager.unity_process_id() {
    ///         println!("Unity Editor PID: {}", pid);
    ///     }
    /// }
    /// ```
    pub fn unity_process_id(&self) -> Option<u32> {
        self.unity_process_id
    }



    /// Checks if a process with the given PID is running and is Unity.exe
    fn is_unity_process_running(&self, pid: u32) -> bool {
        let mut system = System::new();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::everything());
        
        if let Some(process) = system.process(Pid::from(pid as usize)) {
            let process_name = process.name().to_string_lossy();
            // Check for Unity.exe on Windows, Unity on other platforms
            #[cfg(target_os = "windows")]
            {
                process_name.eq_ignore_ascii_case("Unity.exe")
            }
            #[cfg(not(target_os = "windows"))]
            {
                process_name.eq_ignore_ascii_case("Unity")
            }
        } else {
            false
        }
    }

    /// Returns a reference to the project path.
    /// 
    /// # Returns
    /// 
    /// Returns a `&Path` reference to the Unity project directory path that was
    /// provided when creating this `UnityProjectManager` instance.
    /// 
    /// # Examples
    /// 
    /// ```
    /// use unity_code_mcp::UnityProjectManager;
    /// 
    /// let manager = UnityProjectManager::new("/path/to/unity/project".to_string());
    /// println!("Project path: {}", manager.project_path().display());
    /// ```
    pub fn project_path(&self) -> &Path {
        &self.project_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_new_with_embedded_project() {
        // Test with the embedded Unity project
        let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
        let result = UnityProjectManager::new(project_path.to_string_lossy().to_string()).await;
        assert!(result.is_ok());
        
        let manager = result.unwrap();
        assert!(manager.unity_version().is_some());
        assert!(manager.unity_process_id().is_none()); // Should be None initially
    }

    #[tokio::test]
    async fn test_new_with_invalid_project() {
        // Test with a non-Unity project path
        let project_path = PathBuf::from("/tmp");
        let result = UnityProjectManager::new(project_path.to_string_lossy().to_string()).await;
        assert!(result.is_err());
        
        match result {
            Err(UnityProjectError::NotUnityProject(_)) => {}, // Expected
            _ => panic!("Expected NotUnityProject error"),
        }
    }

    #[tokio::test]
    async fn test_cached_unity_version() {
        let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
        let manager = UnityProjectManager::new(project_path.to_string_lossy().to_string()).await.unwrap();

        let version = manager.unity_version();
        assert!(version.is_some());
        assert!(!version.unwrap().is_empty());
        // Should match Unity version format like "6000.0.51f1"
        assert!(version.unwrap().contains("."));
    }

    #[tokio::test]
    async fn test_update_process_info() {
        let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
        let mut manager = UnityProjectManager::new(project_path.to_string_lossy().to_string()).await.unwrap();

        // Initially should be None
        assert!(manager.unity_process_id().is_none());
        
        // Try to update process info
        let result = manager.update_process_info().await;
        
        // The result depends on whether Unity Editor is actually running
        // If EditorInstance.json exists but process isn't running, it should fail
        // If no EditorInstance.json exists, it should also fail
        // If Unity is actually running, it should succeed
        match result {
            Ok(()) => {
                // Unity is running, process ID should be set
                assert!(manager.unity_process_id().is_some());
            },
            Err(_) => {
                // Unity is not running or EditorInstance.json doesn't exist
                assert!(manager.unity_process_id().is_none());
            }
        }
    }
}
