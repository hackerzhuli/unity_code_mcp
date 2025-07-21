use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};
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
    /// Creates a new UnityProjectManager with the given project path
    pub fn new<P: AsRef<Path>>(project_path: P) -> Self {
        Self {
            project_path: project_path.as_ref().to_path_buf(),
        }
    }

    /// Checks if the given path is a valid Unity project
    pub fn is_unity_project(&self) -> bool {
        let project_version_path = self
            .project_path
            .join("ProjectSettings")
            .join("ProjectVersion.txt");
        let assets_path = self.project_path.join("Assets");
        let packages_path = self.project_path.join("Packages");

        project_version_path.exists() && assets_path.exists() && packages_path.exists()
    }

    /// Gets the Unity editor version from ProjectVersion.txt
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

        // Parse the YAML-like content to extract the version
        for line in content.lines() {
            if line.starts_with("m_EditorVersion:") {
                if let Some(version) = line.split(':').nth(1) {
                    return Ok(version.trim().to_string());
                }
            }
        }

        Err(UnityProjectError::VersionNotFound)
    }

    /// Gets the Unity editor version synchronously
    pub fn get_unity_editor_version_sync(&self) -> Result<String, UnityProjectError> {
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
        let content = fs::read_to_string(project_version_path)?;

        // Parse the YAML-like content to extract the version
        for line in content.lines() {
            if line.starts_with("m_EditorVersion:") {
                if let Some(version) = line.split(':').nth(1) {
                    return Ok(version.trim().to_string());
                }
            }
        }

        Err(UnityProjectError::VersionNotFound)
    }

    /// Gets the Unity editor process ID from EditorInstance.json and verifies the process
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

    /// Gets the Unity editor process ID synchronously
    pub fn get_unity_process_id_sync(&self) -> Result<u32, UnityProjectError> {
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

        let content = fs::read_to_string(editor_instance_path)?;
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
        #[cfg(target_os = "windows")]
        {
            // Use tasklist command on Windows to check if process exists and is Unity.exe
            if let Ok(output) = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"])
                .output()
            {
                let output_str = String::from_utf8_lossy(&output.stdout);
                output_str.to_lowercase().contains("unity.exe")
            } else {
                false
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Use ps command on Unix-like systems
            if let Ok(output) = Command::new("ps")
                .args(["-p", &pid.to_string(), "-o", "comm="])
                .output()
            {
                let output_str = String::from_utf8_lossy(&output.stdout);
                output_str.trim().to_lowercase().contains("unity")
            } else {
                false
            }
        }
    }

    /// Gets the project path
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
        let manager = UnityProjectManager::new(project_path);
        assert!(manager.is_unity_project());
    }

    #[test]
    fn test_is_not_unity_project() {
        // Test with a non-Unity project path
        let project_path = PathBuf::from("/tmp");
        let manager = UnityProjectManager::new(project_path);
        assert!(!manager.is_unity_project());
    }

    #[tokio::test]
    async fn test_get_unity_version_with_embedded_project() {
        let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
        let manager = UnityProjectManager::new(project_path);

        if manager.is_unity_project() {
            let result = manager.get_unity_editor_version().await;
            assert!(result.is_ok());
            let version = result.unwrap();
            assert!(!version.is_empty());
            // Should match Unity version format like "6000.0.51f1"
            assert!(version.contains("."));
        }
    }

    #[test]
    fn test_get_unity_version_sync_with_embedded_project() {
        let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
        let manager = UnityProjectManager::new(project_path);

        if manager.is_unity_project() {
            let result = manager.get_unity_editor_version_sync();
            assert!(result.is_ok());
            let version = result.unwrap();
            assert!(!version.is_empty());
            // Should match Unity version format like "6000.0.51f1"
            assert!(version.contains("."));
        }
    }
}
