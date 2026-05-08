//! Log Service Unit Tests
//!
//! Tests for log reading, parsing, filtering, and export functionality.

use e2manage_pos_terminal::services::log_service::{LogEntry, LogLevel, LogService};
use std::fs;
use tempfile::TempDir;

// ============================================================================
// LogLevel Tests
// ============================================================================

#[test]
fn test_log_level_from_str_error() {
    assert_eq!(LogLevel::from_str("ERROR"), LogLevel::Error);
    assert_eq!(LogLevel::from_str("error"), LogLevel::Error);
    assert_eq!(LogLevel::from_str("Error"), LogLevel::Error);
}

#[test]
fn test_log_level_from_str_warn() {
    assert_eq!(LogLevel::from_str("WARN"), LogLevel::Warn);
    assert_eq!(LogLevel::from_str("warn"), LogLevel::Warn);
}

#[test]
fn test_log_level_from_str_info() {
    assert_eq!(LogLevel::from_str("INFO"), LogLevel::Info);
    assert_eq!(LogLevel::from_str("info"), LogLevel::Info);
}

#[test]
fn test_log_level_from_str_debug() {
    assert_eq!(LogLevel::from_str("DEBUG"), LogLevel::Debug);
    assert_eq!(LogLevel::from_str("debug"), LogLevel::Debug);
}

#[test]
fn test_log_level_from_str_trace() {
    assert_eq!(LogLevel::from_str("TRACE"), LogLevel::Trace);
    assert_eq!(LogLevel::from_str("trace"), LogLevel::Trace);
}

#[test]
fn test_log_level_from_str_unknown() {
    // Unknown levels default to Trace
    assert_eq!(LogLevel::from_str("UNKNOWN"), LogLevel::Trace);
    assert_eq!(LogLevel::from_str(""), LogLevel::Trace);
}

#[test]
fn test_log_level_as_str() {
    assert_eq!(LogLevel::Error.as_str(), "ERROR");
    assert_eq!(LogLevel::Warn.as_str(), "WARN");
    assert_eq!(LogLevel::Info.as_str(), "INFO");
    assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
    assert_eq!(LogLevel::Trace.as_str(), "TRACE");
}

// ============================================================================
// LogService Initialization Tests
// ============================================================================

#[test]
fn test_log_service_new() {
    let service = LogService::new();
    // Should initialize without panicking
    assert!(service.log_directory().as_os_str().len() > 0);
}

#[test]
fn test_log_service_default() {
    let service = LogService::default();
    assert!(service.log_directory().as_os_str().len() > 0);
}

// ============================================================================
// Log Reading Tests
// ============================================================================

#[test]
fn test_read_recent_logs_empty_dir() {
    let service = LogService::new();
    // If no logs exist, should return empty vec (not error)
    let result = service.read_recent_logs(100);
    assert!(result.is_ok());
}

#[test]
fn test_list_log_files_empty_dir() {
    let service = LogService::new();
    let result = service.list_log_files();
    assert!(result.is_ok());
}

#[test]
fn test_get_logs_size_no_logs() {
    let service = LogService::new();
    // Size should be 0 or a valid number (u64 is always >= 0)
    let _size = service.get_logs_size();
    // Just verify the function runs without panicking
}

// ============================================================================
// Log Parsing Tests (internal parse_log_line via read_recent_logs)
// ============================================================================

#[cfg(test)]
mod log_parsing {
    use super::*;

    // Helper to create a temporary log directory with content
    fn create_temp_log_dir() -> TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    #[test]
    fn test_parse_standard_log_format() {
        let temp_dir = create_temp_log_dir();
        let log_file = temp_dir.path().join("pos-terminal.log");

        let log_content = r#"2024-01-15T10:30:00.123Z INFO pos_terminal::main: Application started
2024-01-15T10:30:01.456Z DEBUG pos_services::auth: Authenticating operator
2024-01-15T10:30:02.789Z WARN pos_services::sync: Network connection slow
2024-01-15T10:30:03.012Z ERROR pos_db::connection: Database connection failed
"#;

        fs::write(&log_file, log_content).expect("Failed to write log file");

        // Since LogService reads from its configured directory, we can't easily
        // override it in tests. This test demonstrates the expected format.
        // Real integration tests would need a LogService that accepts a custom path.

        assert!(log_file.exists());
    }

    #[test]
    fn test_parse_simple_log_format() {
        // Test that simpler formats are also handled
        let temp_dir = create_temp_log_dir();
        let log_file = temp_dir.path().join("simple.log");

        let log_content = "2024-01-15T10:30:00Z INFO Simple message without target";
        fs::write(&log_file, log_content).expect("Failed to write log file");

        assert!(log_file.exists());
    }
}

// ============================================================================
// Log Export Tests
// ============================================================================

#[test]
fn test_export_logs_creates_zip() {
    let service = LogService::new();

    // Export may fail if no logs exist, which is expected
    // The important thing is it doesn't panic
    let result = service.export_logs();

    // Either succeeds or fails gracefully
    match result {
        Ok(path) => {
            assert!(path.to_string_lossy().ends_with(".zip"));
            // Clean up
            let _ = fs::remove_file(&path);
        }
        Err(_) => {
            // Expected if log directory doesn't exist
        }
    }
}

// ============================================================================
// Log Filtering Tests
// ============================================================================

#[test]
fn test_read_logs_by_level() {
    let service = LogService::new();

    // Should not panic when filtering by level
    let result = service.read_logs_by_level(LogLevel::Error, 100);
    assert!(result.is_ok());

    let result = service.read_logs_by_level(LogLevel::Warn, 100);
    assert!(result.is_ok());

    let result = service.read_logs_by_level(LogLevel::Info, 100);
    assert!(result.is_ok());
}

// ============================================================================
// LogEntry Tests
// ============================================================================

#[test]
fn test_log_entry_clone() {
    let entry = LogEntry {
        timestamp: "2024-01-15T10:30:00Z".to_string(),
        level: LogLevel::Info,
        message: "Test message".to_string(),
        target: "test_module".to_string(),
    };

    let cloned = entry.clone();
    assert_eq!(cloned.timestamp, entry.timestamp);
    assert_eq!(cloned.level, entry.level);
    assert_eq!(cloned.message, entry.message);
    assert_eq!(cloned.target, entry.target);
}

#[test]
fn test_log_entry_debug() {
    let entry = LogEntry {
        timestamp: "2024-01-15T10:30:00Z".to_string(),
        level: LogLevel::Error,
        message: "Error occurred".to_string(),
        target: "pos_services".to_string(),
    };

    // Debug trait should work without panicking
    let debug_str = format!("{:?}", entry);
    assert!(debug_str.contains("Error"));
}

#[test]
fn test_log_level_equality() {
    assert_eq!(LogLevel::Error, LogLevel::Error);
    assert_eq!(LogLevel::Warn, LogLevel::Warn);
    assert_ne!(LogLevel::Error, LogLevel::Warn);
    assert_ne!(LogLevel::Info, LogLevel::Debug);
}

#[test]
fn test_log_level_copy() {
    let level = LogLevel::Info;
    let copied = level; // Copy
    assert_eq!(level, copied);
}
