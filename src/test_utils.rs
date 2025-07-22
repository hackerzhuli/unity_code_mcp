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

/// Deletes the test USS file and its .meta file
pub fn cleanup_test_uss_file(uss_path: &std::path::Path) {
    if uss_path.exists() {
        if let Err(e) = std::fs::remove_file(uss_path) {
            eprintln!("Warning: Failed to delete test USS file: {}", e);
        }
    }

    // Also delete the .meta file to ensure if we create the uss file again
    // unity will treat it as a new file
    let meta_path = uss_path.with_extension("uss.meta");
    if meta_path.exists() {
        if let Err(e) = std::fs::remove_file(&meta_path) {
            eprintln!("Warning: Failed to delete test USS .meta file: {}", e);
        }
    }
}

/// Creates a C# script to trigger Unity compilation
pub fn create_test_cs_script(project_path: &std::path::Path) -> std::path::PathBuf {
    let cs_path = project_path
        .join("Assets")
        .join("Scripts")
        .join("TestCompilation.cs");
    let cs_content = r#"using UnityEngine;

namespace UnityProject
{
    /// <summary>
    /// A test script to trigger Unity compilation and offline state.
    /// </summary>
    public class TestCompilation : MonoBehaviour
    {
        /// <summary>
        /// A simple test method.
        /// </summary>
        public void TestMethod()
        {
            Debug.Log("Test compilation script loaded");
        }
    }
}
"#;

    // Ensure the Scripts directory exists
    let scripts_dir = cs_path.parent().unwrap();
    if !scripts_dir.exists() {
        std::fs::create_dir_all(scripts_dir).expect("Failed to create Scripts directory");
    }

    std::fs::write(&cs_path, cs_content).expect("Failed to create test C# script");
    cs_path
}

/// Deletes the test C# script and its .meta file
pub fn cleanup_test_cs_script(cs_path: &std::path::Path) {
    if cs_path.exists() {
        if let Err(e) = std::fs::remove_file(cs_path) {
            eprintln!("Warning: Failed to delete test C# script: {}", e);
        }
    }

    // Also delete the .meta file to ensure Unity recognizes the change
    let meta_path = cs_path.with_extension("cs.meta");
    if meta_path.exists() {
        if let Err(e) = std::fs::remove_file(&meta_path) {
            eprintln!("Warning: Failed to delete test C# script .meta file: {}", e);
        }
    }
}

/// Creates a C# script with compilation errors to trigger Unity compilation errors
pub fn create_test_cs_script_with_errors(project_path: &std::path::Path) -> std::path::PathBuf {
    let cs_path = project_path
        .join("Assets")
        .join("Scripts")
        .join("TestCompilationErrors.cs");
    let cs_content = r#"using UnityEngine;
using NonExistentNamespace; // This will cause a compilation error

namespace UnityProject
{
    /// <summary>
    /// A test script with intentional compilation errors.
    /// </summary>
    public class TestCompilationErrors : MonoBehaviour
    {
        /// <summary>
        /// A method with compilation errors.
        /// </summary>
        public void TestMethodWithErrors()
        {
            // Undefined variable - compilation error
            undefinedVariable = 42;
            
            // Invalid method call - compilation error
            NonExistentClass.DoSomething();
            
            // Syntax error - missing semicolon
            Debug.Log("This line is missing a semicolon")
        }
        
        // Invalid property syntax - compilation error
        public int InvalidProperty { get set; }
    }
}
"#;

    // Ensure the Scripts directory exists
    let scripts_dir = cs_path.parent().unwrap();
    if !scripts_dir.exists() {
        std::fs::create_dir_all(scripts_dir).expect("Failed to create Scripts directory");
    }

    std::fs::write(&cs_path, cs_content).expect("Failed to create test C# script with errors");
    cs_path
}

/// Deletes the test C# script with errors and its .meta file
pub fn cleanup_test_cs_script_with_errors(cs_path: &std::path::Path) {
    if cs_path.exists() {
        if let Err(e) = std::fs::remove_file(cs_path) {
            eprintln!(
                "Warning: Failed to delete test C# script with errors: {}",
                e
            );
        }
    }

    // Also delete the .meta file to ensure Unity recognizes the change
    let meta_path = cs_path.with_extension("cs.meta");
    if meta_path.exists() {
        if let Err(e) = std::fs::remove_file(&meta_path) {
            eprintln!(
                "Warning: Failed to delete test C# script with errors .meta file: {}",
                e
            );
        }
    }
}
