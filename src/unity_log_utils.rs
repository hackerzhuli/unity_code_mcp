/// Utility functions for processing Unity log messages

/// Extract the main message from a Unity log, removing stack trace information
/// 
/// Unity logs often contain stack traces after the main message. This function
/// extracts only the relevant part before the stack trace begins.
/// 
/// # Arguments
/// * `log_message` - The full Unity log message
/// 
/// # Returns
/// The cleaned message without stack trace
/// 
/// # Examples
/// ```
/// let log = "Assets/UI/hello.uss (line 2): warning: Unknown property 'back' (did you mean 'background-size'?) \n     back: 10px \n UnityEditor.AssetDatabase:Refresh ()";
/// let cleaned = extract_main_message(log);
/// assert_eq!(cleaned, "Assets/UI/hello.uss (line 2): warning: Unknown property 'back' (did you mean 'background-size'?) \n     back: 10px");
/// ```
pub fn extract_main_message(log_message: &str) -> String {
    let lines: Vec<&str> = log_message.lines().collect();
    let mut result_lines = Vec::new();
    
    for line in lines {
        let trimmed = line.trim();
        
        // Check if this line looks like a stack trace entry
        // Stack traces typically contain method calls with parentheses and file paths
        if is_stack_trace_line(trimmed) {
            break;
        }
        
        result_lines.push(line);
    }
    
    // Join the lines back together, preserving original formatting
    let result = result_lines.join("\n");
    
    // Trim trailing whitespace but preserve the main content
    result.trim_end().to_string()
}

/// Check if a string is a valid C# identifier (method name, variable name, etc.)
fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    
    let chars: Vec<char> = name.chars().collect();
    if chars.is_empty() {
        return false;
    }
    
    // First character should be letter or underscore
    if !chars[0].is_alphabetic() && chars[0] != '_' {
        return false;
    }
    
    // Rest should be alphanumeric or underscore
    for &ch in &chars[1..] {
        if !ch.is_alphanumeric() && ch != '_' {
            return false;
        }
    }
    
    true
}

/// Check if a string looks like a C# class name (with optional namespace)
fn is_class_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    
    // Split by dots to handle namespaces
    let parts: Vec<&str> = name.split('.').collect();
    
    // Each part should be a valid identifier (starts with letter or underscore, contains only alphanumeric and underscores)
    for part in parts {
        if part.is_empty() {
            return false;
        }
        
        let chars: Vec<char> = part.chars().collect();
        if chars.is_empty() {
            return false;
        }
        
        // First character should be letter or underscore
        if !chars[0].is_alphabetic() && chars[0] != '_' {
            return false;
        }
        
        // Rest should be alphanumeric or underscore
        for &ch in &chars[1..] {
            if !ch.is_alphanumeric() && ch != '_' {
                return false;
            }
        }
    }
    
    true
}

/// Check if a line appears to be part of a stack trace
fn is_stack_trace_line(line: &str) -> bool {
    // Common patterns in Unity stack traces:
    // - Contains method calls with parentheses: "MethodName ()"
    // - Contains file paths with line numbers: "(at ./path/file.cs:123)"
    // - Starts with Unity namespace: "UnityEditor.", "UnityEngine."
    // - Contains package cache paths: "./Library/PackageCache/"
    
    if line.is_empty() {
        return false;
    }
    
    // Check for C# class method calls (ClassName:MethodName() or Namespace.ClassName:MethodName())
    if line.contains(":") && line.contains("()") {
        // Split by colon to check if we have a class:method pattern
        if let Some(colon_pos) = line.find(':') {
            let before_colon = &line[..colon_pos];
            let after_colon = &line[colon_pos + 1..];
            
            // Check if the part before colon looks like a class name (may include namespace)
            // and the part after colon looks like a method call
            if is_class_name(before_colon.trim()) {
                // Extract method name from "MethodName () (optional additional info)"
                let method_part = after_colon.trim();
                if let Some(paren_pos) = method_part.find('(') {
                    let method_name = method_part[..paren_pos].trim();
                    if is_valid_identifier(method_name) && method_part[paren_pos..].starts_with("()") {
                        return true;
                    }
                }
            }
        }
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_main_message_with_stack_trace() {
        let log_message = "Assets/UI/hello.uss (line 2): warning: Unknown property 'back' (did you mean 'background-size'?) \n     back: 10px \n UnityEditor.AssetDatabase:Refresh () \n Hackerzhuli.Code.Editor.CodeEditorIntegrationCore:RefreshAssetDatabase () (at ./Library/PackageCache/com.hackerzhuli.code@c0eb44f77de4/Editor/CodeEditorIntegrationCore.cs:325) \n Hackerzhuli.Code.Editor.CodeEditorIntegrationCore:Update () (at ./Library/PackageCache/com.hackerzhuli.code@c0eb44f77de4/Editor/CodeEditorIntegrationCore.cs:212) \n UnityEditor.EditorApplication:Internal_CallUpdateFunctions ()";
        
        let result = extract_main_message(log_message);
        let expected = "Assets/UI/hello.uss (line 2): warning: Unknown property 'back' (did you mean 'background-size'?) \n     back: 10px";
        
        assert_eq!(result, expected);
    }

    #[test]
    fn test_extract_main_message_without_stack_trace() {
        let log_message = "Simple error message without stack trace";
        let result = extract_main_message(log_message);
        assert_eq!(result, "Simple error message without stack trace");
    }

    #[test]
    fn test_extract_main_message_empty() {
        let log_message = "";
        let result = extract_main_message(log_message);
        assert_eq!(result, "");
    }

    #[test]
    fn test_extract_main_message_only_stack_trace() {
        let log_message = "UnityEditor.AssetDatabase:Refresh () \n UnityEngine.Debug:Log()";
        let result = extract_main_message(log_message);
        assert_eq!(result, "");
    }

    #[test]
    fn test_is_valid_identifier() {
        // Valid identifiers
        assert!(is_valid_identifier("MyMethod"));
        assert!(is_valid_identifier("_validMethod"));
        assert!(is_valid_identifier("method123"));
        assert!(is_valid_identifier("My_Method_Name"));
        assert!(is_valid_identifier("Refresh"));
        assert!(is_valid_identifier("Internal_CallUpdateFunctions"));
        
        // Invalid identifiers
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("123InvalidStart"));
        assert!(!is_valid_identifier("Invalid-Name"));
        assert!(!is_valid_identifier("Invalid Name"));
        assert!(!is_valid_identifier("Invalid.Name"));
    }

    #[test]
    fn test_is_class_name() {
        // Valid class names
        assert!(is_class_name("MyClass"));
        assert!(is_class_name("UnityEditor"));
        assert!(is_class_name("UnityEngine"));
        assert!(is_class_name("Namespace.MyClass"));
        assert!(is_class_name("UnityEditor.AssetDatabase"));
        assert!(is_class_name("Hackerzhuli.Code.Editor.CodeEditorIntegrationCore"));
        assert!(is_class_name("_ValidClass"));
        assert!(is_class_name("Class123"));
        assert!(is_class_name("My_Class_Name"));
        
        // Invalid class names
        assert!(!is_class_name(""));
        assert!(!is_class_name("123InvalidStart"));
        assert!(!is_class_name("Invalid-Name"));
        assert!(!is_class_name("Invalid.123Start"));
        assert!(!is_class_name("Invalid..Double.Dot"));
        assert!(!is_class_name(".StartWithDot"));
        assert!(!is_class_name("EndWithDot."));
        assert!(!is_class_name("Invalid Space"));
    }

    #[test]
    fn test_is_stack_trace_line() {
        // Should detect stack trace lines
        assert!(is_stack_trace_line("UnityEditor.AssetDatabase:Refresh ()"));
        assert!(is_stack_trace_line("Hackerzhuli.Code.Editor.CodeEditorIntegrationCore:RefreshAssetDatabase () (at ./Library/PackageCache/com.hackerzhuli.code@c0eb44f77de4/Editor/CodeEditorIntegrationCore.cs:325)"));
        assert!(is_stack_trace_line("UnityEditor.EditorApplication:Internal_CallUpdateFunctions ()"));
        assert!(is_stack_trace_line("MyNamespace.MyClass:MyMethod ()"));
        assert!(is_stack_trace_line("SimpleClass:SimpleMethod ()"));
        
        // Should not detect regular log content
        assert!(!is_stack_trace_line("Assets/UI/hello.uss (line 2): warning: Unknown property 'back'"));
        assert!(!is_stack_trace_line("     back: 10px"));
        assert!(!is_stack_trace_line("Simple error message"));
        assert!(!is_stack_trace_line(""));
        assert!(!is_stack_trace_line("Invalid:123Method ()"));
        assert!(!is_stack_trace_line("123Invalid:Method ()"));
    }

    #[test]
    fn test_multiline_message_with_indentation() {
        let log_message = "Compilation failed because of multiple errors:\nError 1: Missing semicolon\nError 2: Undefined variable\n UnityEditor.Compilation:CompileScripts()\n UnityEngine.Debug:LogError()";
        
        let result = extract_main_message(log_message);
        let expected = "Compilation failed because of multiple errors:\nError 1: Missing semicolon\nError 2: Undefined variable";
        
        assert_eq!(result, expected);
    }
}