use serde::{Deserialize, Serialize};

/// User settings. Every field has a conservative default: nothing is deleted,
/// tracked, or followed unless the user turns it on.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    // General
    pub start_with_windows: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub show_notifications: bool,
    pub theme: ThemePreference,
    /// What a double-click on the notification-area icon opens. This never
    /// performs a destructive operation: automated cleanup stays a separately
    /// enabled rule with its own safeguards.
    pub tray_double_click_action: TrayDoubleClickAction,

    // Scanning
    pub scan_all_local_drives: bool,
    pub follow_symlinks: bool,
    pub follow_junctions: bool,
    pub scan_hidden_files: bool,
    pub scan_system_files: bool,
    pub scan_concurrency: u32,
    pub use_allocated_size: bool,

    // Cleanup
    pub default_deletion_method: String,
    pub prefer_recycle_bin: bool,
    pub enable_quarantine: bool,
    pub quarantine_retention_days: u32,
    pub confirm_permanent_deletion: bool,
    pub show_advanced_categories: bool,

    // Applications
    /// Observation is off until the user turns it on, and stays on-device.
    pub track_application_launches: bool,
    pub enable_windows_activity_estimates: bool,
    pub show_confidence_levels: bool,
    pub exclude_background_utilities: bool,

    // Developer
    pub detect_development_projects: bool,
    pub scan_package_caches: bool,
    pub detect_virtual_disks: bool,

    // Privacy
    pub store_scan_history: bool,
    pub store_cleanup_history: bool,
    pub history_retention_days: u32,

    // Advanced
    pub debug_logging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayDoubleClickAction {
    Open,
    Scan,
    Cleanup,
    LargeFiles,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_with_windows: false,
            minimize_to_tray: true,
            close_to_tray: true,
            show_notifications: true,
            theme: ThemePreference::System,
            tray_double_click_action: TrayDoubleClickAction::Open,

            scan_all_local_drives: false,
            follow_symlinks: false,
            follow_junctions: false,
            scan_hidden_files: true,
            scan_system_files: false,
            scan_concurrency: 4,
            use_allocated_size: true,

            default_deletion_method: "recycleBin".into(),
            prefer_recycle_bin: true,
            enable_quarantine: false,
            quarantine_retention_days: 7,
            confirm_permanent_deletion: true,
            show_advanced_categories: false,

            // Tracking anything about the user requires an explicit opt-in.
            track_application_launches: false,
            enable_windows_activity_estimates: false,
            show_confidence_levels: true,
            exclude_background_utilities: true,

            detect_development_projects: true,
            scan_package_caches: true,
            detect_virtual_disks: true,

            store_scan_history: true,
            store_cleanup_history: true,
            history_retention_days: 90,

            debug_logging: false,
        }
    }
}

/// Window and navigation state Stora restores between sessions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub selected_page: Option<String>,
    pub sidebar_collapsed: Option<bool>,
    pub selected_drive: Option<String>,
    pub large_file_sort: Option<String>,
}
