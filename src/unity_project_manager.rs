use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use sysinfo::{System, Pid, ProcessesToUpdate, ProcessRefreshKind};
use tokio::fs as async_fs;

#[derive(Debug, Clone)]
pub struct UnityProjectManager {
    project_path: PathBuf,
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
    /// # Arguments
    /// 
    /// * `project_path` - A string representing the absolute path to the Unity project directory
    /// 
    /// # Examples
    /// 
    /// ```
    /// use unity_code_mcp::UnityProjectManager;
    /// 
    /// let manager = UnityProjectManager::new("/path/to/unity/project");
    /// ```
    pub fn new(project_path: String) -> Self {
        Self {
            project_path: PathBuf::from(project_path),
        }
    }

    /// Checks if the configured path is a valid Unity project.
    /// 
    /// A valid Unity project must contain:
    /// - `ProjectSettings/ProjectVersion.txt` file
    /// - `Assets/` directory
    /// - `Packages/` directory
    /// 
    /// # Returns
    /// 
    /// Returns `true` if the path represents a valid Unity project, `false` otherwise.
    /// 
    /// # Examples
    /// 
    /// ```
    /// use unity_code_mcp::UnityProjectManager;
    /// 
    /// let manager = UnityProjectManager::new("/path/to/unity/project".to_string());
    /// if manager.is_unity_project() {
    ///     println!("Valid Unity project found!");
    /// }
    /// ```
    pub fn is_unity_project(&self) -> bool {
        let project_version_path = self
            .project_path
            .join("ProjectSettings")
            .join("ProjectVersion.txt");
        let assets_path = self.project_path.join("Assets");
        let packages_path = self.project_path.join("Packages");

        project_version_path.exists() && assets_path.exists() && packages_path.exists()
    }

    /// Asynchronously retrieves the Unity Editor version from the project's ProjectVersion.txt file.
    /// 
    /// This method reads and parses the YAML content of `ProjectSettings/ProjectVersion.txt`
    /// to extract the Unity Editor version used by this project.
    /// 
    /// # Returns
    /// 
    /// Returns a `Result` containing:
    /// - `Ok(String)` - The Unity Editor version (e.g., "6000.0.51f1")
    /// - `Err(UnityProjectError)` - If the project is invalid, file cannot be read, or version cannot be parsed
    /// 
    /// # Errors
    /// 
    /// This function will return an error if:
    /// - The path is not a valid Unity project
    /// - The ProjectVersion.txt file cannot be read
    /// - The YAML content cannot be parsed
    /// - The version field is missing from the file
    /// 
    /// # Examples
    /// 
    /// ```
    /// use unity_code_mcp::UnityProjectManager;
    /// 
    /// #[tokio::main]
    /// async fn main() {
    ///     let manager = UnityProjectManager::new("/path/to/unity/project".to_string());
    ///     match manager.get_unity_editor_version().await {
    ///         Ok(version) => println!("Unity version: {}", version),
    ///         Err(e) => eprintln!("Error: {}", e),
    ///     }
    /// }
    /// ```
    pub async fn get_unity_editor_version(&self) -> Result<String, UnityProjectError> {
        if !self.is_unity_project() {
            return Err(UnityProjectError::NotUnityProject(format!(
                "Path '{}' is not a Unity project",
                self.project_path.display()
            )));
        }

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



    /// Asynchronously retrieves the Unity Editor process ID for this project.
    /// 
    /// This method reads the `Library/EditorInstance.json` file to get the process ID
    /// of the Unity Editor instance currently editing this project, and verifies that
    /// the process is actually running and is a Unity Editor process.
    /// 
    /// # Returns
    /// 
    /// Returns a `Result` containing:
    /// - `Ok(u32)` - The process ID of the running Unity Editor instance
    /// - `Err(UnityProjectError)` - If no Unity Editor is running for this project
    /// 
    /// # Errors
    /// 
    /// This function will return an error if:
    /// - The path is not a valid Unity project
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
    ///     let manager = UnityProjectManager::new("/path/to/unity/project".to_string());
    ///     match manager.get_unity_process_id().await {
    ///         Ok(pid) => println!("Unity Editor PID: {}", pid),
    ///         Err(e) => eprintln!("No Unity Editor running: {}", e),
    ///     }
    /// }
    /// ```
    pub async fn get_unity_process_id(&self) -> Result<u32, UnityProjectError> {
        if !self.is_unity_project() {
            return Err(UnityProjectError::NotUnityProject(format!(
                "Path '{}' is not a Unity project",
                self.project_path.display()
            )));
        }

        let editor_instance_path = self
            .project_path
            .join("Library")
            .join("EditorInstance.json");

        if !editor_instance_path.exists() {
            return Err(UnityProjectError::ProcessNotFound);
        }

        let content = async_fs::read_to_string(editor_instance_path).await?;
        let editor_instance: EditorInstance = serde_json::from_str(&content)?;

        // Verify that the process is actually running and is Unity.exe
        if self.is_unity_process_running(editor_instance.process_id) {
            Ok(editor_instance.process_id)
        } else {
            Err(UnityProjectError::ProcessNotFound)
        }
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

    #[test]
    fn test_is_unity_project_with_embedded_project() {
        // Test with the embedded Unity project
        let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
        let manager = UnityProjectManager::new(project_path.to_string_lossy().to_string());
        assert!(manager.is_unity_project());
    }

    #[test]
    fn test_is_not_unity_project() {
        // Test with a non-Unity project path
        let project_path = PathBuf::from("/tmp");
        let manager = UnityProjectManager::new(project_path.to_string_lossy().to_string());
        assert!(!manager.is_unity_project());
    }

    #[tokio::test]
    async fn test_get_unity_version_with_embedded_project() {
        let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
        let manager = UnityProjectManager::new(project_path.to_string_lossy().to_string());

        if manager.is_unity_project() {
            let result = manager.get_unity_editor_version().await;
            assert!(result.is_ok());
            let version = result.unwrap();
            assert!(!version.is_empty());
            // Should match Unity version format like "6000.0.51f1"
            assert!(version.contains("."));
        }
    }


}
