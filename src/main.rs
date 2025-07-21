mod unity_project_manager;

use std::path::PathBuf;
use unity_project_manager::UnityProjectManager;

#[tokio::main]
async fn main() {
    println!("Unity Code MCP Server");

    // Example usage with the embedded Unity project
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
    let manager = UnityProjectManager::new(project_path.to_string_lossy().to_string());

    println!("Project path: {}", project_path.display());
    println!("Is Unity project: {}", manager.is_unity_project());

    if manager.is_unity_project() {
        match manager.get_unity_editor_version().await {
            Ok(version) => println!("Unity version: {}", version),
            Err(e) => println!("Failed to get Unity version: {}", e),
        }

        match manager.get_unity_process_id().await {
            Ok(pid) => println!("Unity process ID: {}", pid),
            Err(e) => println!("Unity process not found: {}", e),
        }
    }
}
