//! Shared models, typed errors, and background-task coordination for Stora.
//!
//! This crate is deliberately free of Windows API calls and database access so
//! it can be unit tested on any platform.

pub mod cleanup;
pub mod error;
pub mod model;
pub mod settings;
pub mod task;

pub use error::{ErrorPayload, Result, StoraError};
pub use model::*;
pub use settings::{Settings, ThemePreference, UiState};
pub use task::{TaskControl, TaskRegistry};

/// Current unix time in seconds. Centralized so tests can reason about it.
pub fn now_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Formats a byte count the way Windows does (binary units, one decimal).
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["bytes", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes_in_binary_units() {
        assert_eq!(format_bytes(0), "0 bytes");
        assert_eq!(format_bytes(512), "512 bytes");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn drive_usage_math_handles_empty_volume() {
        let drive = DriveInfo {
            root: "C:\\".into(),
            label: "Local Disk".into(),
            filesystem: "NTFS".into(),
            total_bytes: 0,
            free_bytes: 0,
            drive_type: DriveType::Fixed,
            is_removable: false,
        };
        assert_eq!(drive.percent_used(), 0.0);
        assert_eq!(drive.used_bytes(), 0);
    }
}
