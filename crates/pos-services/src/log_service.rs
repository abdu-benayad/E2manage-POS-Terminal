//! Log Service - Read and manage application logs
//!
//! Provides functionality to:
//! - Read recent log entries
//! - Filter logs by level
//! - Export logs to ZIP archive
//! - Get system information for debugging

use chrono::Local;
use directories::{BaseDirs, ProjectDirs};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::str::FromStr;

use crate::parse::ParseError;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Get project directories for e2manage-pos
fn get_project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "e2manage", "pos-terminal")
}

/// Get base directories (home, downloads, etc.)
fn get_base_dirs() -> Option<BaseDirs> {
    BaseDirs::new()
}

/// Log entry parsed from log file
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl FromStr for LogLevel {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "ERROR" => Ok(LogLevel::Error),
            "WARN" => Ok(LogLevel::Warn),
            "INFO" => Ok(LogLevel::Info),
            "DEBUG" => Ok(LogLevel::Debug),
            "TRACE" => Ok(LogLevel::Trace),
            _ => Err(ParseError::LogLevel(s.to_string())),
        }
    }
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }
}

#[derive(Debug)]
pub enum LogError {
    IoError(std::io::Error),
    ParseError(String),
}

impl std::fmt::Display for LogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogError::IoError(e) => write!(f, "IO error: {}", e),
            LogError::ParseError(e) => write!(f, "Parse error: {}", e),
        }
    }
}

impl std::error::Error for LogError {}

pub type LogResult<T> = Result<T, LogError>;

pub struct LogService {
    log_dir: PathBuf,
}

impl LogService {
    pub fn new() -> Self {
        let log_dir = get_project_dirs()
            .map(|p| p.data_local_dir().join("logs"))
            .unwrap_or_else(|| PathBuf::from("./data/logs"));
        Self { log_dir }
    }

    /// Get log directory path
    pub fn log_directory(&self) -> &PathBuf {
        &self.log_dir
    }

    /// Read recent log entries (last N lines)
    pub fn read_recent_logs(&self, max_lines: usize) -> LogResult<Vec<LogEntry>> {
        let log_file = self.log_dir.join("pos-terminal.log");

        if !log_file.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&log_file).map_err(LogError::IoError)?;

        let entries: Vec<LogEntry> = content
            .lines()
            .rev()
            .take(max_lines)
            .filter_map(|line| self.parse_log_line(line))
            .collect();

        Ok(entries.into_iter().rev().collect())
    }

    /// Read logs filtered by level
    pub fn read_logs_by_level(
        &self,
        level: LogLevel,
        max_lines: usize,
    ) -> LogResult<Vec<LogEntry>> {
        let all_logs = self.read_recent_logs(max_lines * 10)?;
        Ok(all_logs
            .into_iter()
            .filter(|e| e.level == level)
            .take(max_lines)
            .collect())
    }

    /// Parse a single log line
    /// Format: 2024-01-15T10:30:00.123Z INFO target: message
    fn parse_log_line(&self, line: &str) -> Option<LogEntry> {
        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        if parts.len() < 3 {
            // Try simpler format without target
            let simple_parts: Vec<&str> = line.splitn(3, ' ').collect();
            if simple_parts.len() >= 3 {
                return Some(LogEntry {
                    timestamp: simple_parts[0].to_string(),
                    // A log line whose level field is unreadable is noise, not a failure: default
                    // here, visibly, rather than letting the parser decide for every caller.
                    level: simple_parts[1].parse().unwrap_or(LogLevel::Trace),
                    target: String::new(),
                    message: simple_parts[2..].join(" "),
                });
            }
            return None;
        }

        Some(LogEntry {
            timestamp: parts[0].to_string(),
            level: parts[1].parse().unwrap_or(LogLevel::Trace),
            target: parts
                .get(2)
                .map(|s| s.trim_end_matches(':'))
                .unwrap_or("")
                .to_string(),
            message: parts.get(3).unwrap_or(&"").to_string(),
        })
    }

    /// Get list of available log files
    pub fn list_log_files(&self) -> LogResult<Vec<String>> {
        if !self.log_dir.exists() {
            return Ok(vec![]);
        }

        let files: Vec<String> = fs::read_dir(&self.log_dir)
            .map_err(LogError::IoError)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "log"))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();

        Ok(files)
    }

    /// Export all log files to a ZIP archive
    /// Returns path to created ZIP file
    pub fn export_logs(&self) -> LogResult<PathBuf> {
        // Export to home directory (or current dir as fallback)
        let export_dir = get_base_dirs()
            .map(|b| b.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let zip_name = format!("pos-logs-{}.zip", timestamp);
        let zip_path = export_dir.join(&zip_name);

        let file = fs::File::create(&zip_path).map_err(LogError::IoError)?;
        let mut zip = ZipWriter::new(BufWriter::new(file));

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        // Add all log files
        if self.log_dir.exists() {
            for entry in fs::read_dir(&self.log_dir).map_err(LogError::IoError)? {
                let entry = entry.map_err(LogError::IoError)?;
                let path = entry.path();

                if path.extension().is_some_and(|e| e == "log") {
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown.log");

                    let content = fs::read_to_string(&path).map_err(LogError::IoError)?;

                    zip.start_file(file_name, options)
                        .map_err(|e| LogError::IoError(std::io::Error::other(e)))?;
                    zip.write_all(content.as_bytes())
                        .map_err(LogError::IoError)?;
                }
            }
        }

        // Add system info file
        let system_info = self.gather_system_info();
        zip.start_file("system-info.txt", options)
            .map_err(|e| LogError::IoError(std::io::Error::other(e)))?;
        zip.write_all(system_info.as_bytes())
            .map_err(LogError::IoError)?;

        // Add database info (if exists)
        let db_path = get_project_dirs()
            .map(|p| p.data_local_dir().join("pos_terminal.db"))
            .unwrap_or_else(|| PathBuf::from("./data/pos_terminal.db"));

        if db_path.exists() {
            zip.start_file("database-info.txt", options)
                .map_err(|e| LogError::IoError(std::io::Error::other(e)))?;
            let db_info = format!(
                "Database path: {:?}\nDatabase size: {} bytes\n",
                db_path,
                fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0)
            );
            zip.write_all(db_info.as_bytes())
                .map_err(LogError::IoError)?;
        }

        zip.finish()
            .map_err(|e| LogError::IoError(std::io::Error::other(e)))?;

        Ok(zip_path)
    }

    /// Gather system information for debugging
    fn gather_system_info(&self) -> String {
        let mut info = String::new();

        info.push_str("=== E2Manage POS Terminal - System Info ===\n\n");

        // OS info
        info.push_str(&format!("OS: {}\n", std::env::consts::OS));
        info.push_str(&format!("Arch: {}\n", std::env::consts::ARCH));

        // App version
        info.push_str(&format!("App Version: {}\n", env!("CARGO_PKG_VERSION")));

        // Timestamp
        info.push_str(&format!(
            "Export Time: {}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        ));

        // Log directory
        info.push_str(&format!("Log Directory: {:?}\n", self.log_dir));

        // Log files
        if let Ok(files) = self.list_log_files() {
            info.push_str(&format!("\nLog Files ({}):\n", files.len()));
            for f in files {
                info.push_str(&format!("  - {}\n", f));
            }
        }

        info
    }

    /// Get total size of all log files
    pub fn get_logs_size(&self) -> u64 {
        if !self.log_dir.exists() {
            return 0;
        }

        fs::read_dir(&self.log_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "log"))
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    }
}

impl Default for LogService {
    fn default() -> Self {
        Self::new()
    }
}
