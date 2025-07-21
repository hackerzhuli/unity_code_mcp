mod unity_project_manager;

use std::path::PathBuf;
use unity_project_manager::UnityProjectManager;

#[tokio::main]
async fn main() {
    println!("Unity Code MCP Server");

    // Example usage with the embedded Unity project
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UnityProject");
    
    println!("Project path: {}", project_path.display());
    
    match UnityProjectManager::new(project_path.to_string_lossy().to_string()).await {
        Ok(mut manager) => {
            println!("Successfully initialized Unity project manager");
            
            if let Some(version) = manager.unity_version() {
                println!("Unity version: {}", version);
            }
            
            // Try to update process info
            match manager.update_process_info().await {
                Ok(()) => {
                    if let Some(pid) = manager.unity_process_id() {
                        println!("Unity process ID: {}", pid);
                    }
                },
                Err(e) => println!("Unity process not found: {}", e),
            }
        },
        Err(e) => println!("Failed to initialize Unity project manager: {}", e),
    }
}
