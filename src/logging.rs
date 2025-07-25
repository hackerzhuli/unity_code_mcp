use env_logger::{Builder, Target};
use log::LevelFilter;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Initialize logging based on whether we're running tests or the main application
pub fn init_logging() {
    // Check if we're running in test mode
    if cfg!(test) {
        // do nothing
    } else {
        // In application mode, log to file
        init_file_logging();
    }
}

/// Initialize logging for tests (output to stdout)
fn init_test_logging() {
    let mut builder = Builder::from_default_env();
    builder
        .target(Target::Stdout)
        .filter_level(LevelFilter::Debug)
        .is_test(true)
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] [{}] [{}:{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();
}

/// Initialize logging for the application (output to file)
fn init_file_logging() {
    // Get the local app data directory
    let log_dir = get_log_directory();

    // Create the directory if it doesn't exist
    if let Err(_e) = fs::create_dir_all(&log_dir) {
        //eprintln!("Failed to create log directory: {}", e);
        return;
    }

    let file_name = if cfg!(debug_assertions) {
        "unity_code_mcp_debug.log"
    } else {
        "unity_code_mcp.log"
    };
    
    // Create log file path
    let log_file = log_dir.join(file_name);

    // Try to open the log file, but don't panic if it fails
    let target = match fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_file)
    {
        Ok(file) => Box::new(file),
        Err(_e) => {
            //eprintln!("Failed to open log file {}: {}", log_file.display(), e);
            return;
        }
    };

    let mut builder = Builder::from_default_env();
    
    // Use Debug level in debug builds, Info level in release builds
    let log_level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    
    builder
        .target(Target::Pipe(target))
        .filter_level(log_level)
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] [{}] [{}:{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();

    log::info!("Logging initialized to file: {}", log_file.display());
}

/// Get the appropriate log directory based on the operating system
fn get_log_directory() -> PathBuf {
    if let Some(data_dir) = dirs::data_local_dir() {
        data_dir.join("UnityCode")
    } else {
        // Fallback to current directory if we can't determine the local data directory
        PathBuf::from("./logs")
    }
}

/// Macro for debug logging that replaces println! for debug messages
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        log::debug!($($arg)*)
    };
}

/// Macro for info logging
#[macro_export]
macro_rules! info_log {
    ($($arg:tt)*) => {
        log::info!($($arg)*)
    };
}

/// Macro for warning logging
#[macro_export]
macro_rules! warn_log {
    ($($arg:tt)*) => {
        log::warn!($($arg)*)
    };
}

/// Macro for error logging
#[macro_export]
macro_rules! error_log {
    ($($arg:tt)*) => {
        log::error!($($arg)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_directory() {
        let log_dir = get_log_directory();
        assert!(log_dir.to_string_lossy().contains("UnityCode"));
    }
}

#[cfg(test)]
#[ctor::ctor]
fn setup_logger() {
    init_test_logging();
}