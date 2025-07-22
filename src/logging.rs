use env_logger::{Builder, Target};
use log::LevelFilter;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Initialize logging based on whether we're running tests or the main application
pub fn init_logging() {
    // Check if we're running in test mode
    if cfg!(test) {
        // In test mode, log to stdout
        init_test_logging();
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
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] [{}:{}] {}",
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
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("Failed to create log directory: {}", e);
        return;
    }

    // Create log file path
    let log_file = log_dir.join("unity_code_mcp.log");

    // Initialize file-based logging
    let target = Box::new(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .expect("Failed to open log file"),
    );

    let mut builder = Builder::from_default_env();
    builder
        .target(Target::Pipe(target))
        .filter_level(LevelFilter::Info)
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] [{}] [{}:{}] {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
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

    #[test]
    fn test_logging_macros() {
        init_logging();

        debug_log!("This is a debug message");
        info_log!("This is an info message");
        warn_log!("This is a warning message");
        error_log!("This is an error message");
    }
}
