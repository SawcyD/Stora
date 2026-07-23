use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    /// Volume root, e.g. `C:\`.
    pub root: String,
    pub label: String,
    pub filesystem: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub drive_type: DriveType,
    pub is_removable: bool,
}

impl DriveInfo {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.free_bytes)
    }

    pub fn percent_used(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.used_bytes() as f64 / self.total_bytes as f64) * 100.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DriveType {
    Fixed,
    Removable,
    Network,
    CdRom,
    RamDisk,
    Unknown,
}

/// A single file record captured during a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub parent_path: String,
    pub name: String,
    pub extension: Option<String>,
    pub logical_size: u64,
    pub allocated_size: u64,
    pub created: Option<i64>,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub attributes: u32,
    pub is_directory: bool,
    pub is_reparse_point: bool,
}

/// Aggregated per-folder totals, stored separately from individual records so
/// the common tree queries stay fast on multi-million-file drives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderAggregate {
    pub path: String,
    pub parent_path: Option<String>,
    pub name: String,
    pub logical_size: u64,
    pub allocated_size: u64,
    pub file_count: u64,
    pub folder_count: u64,
    pub modified: Option<i64>,
    pub has_children: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanState {
    Idle,
    Preparing,
    Scanning,
    Paused,
    Cancelling,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub task_id: String,
    pub state: ScanState,
    pub root: String,
    pub files_scanned: u64,
    pub folders_scanned: u64,
    pub bytes_analyzed: u64,
    pub current_path: String,
    pub errors: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub scan_id: i64,
    pub root: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: u64,
    pub files_scanned: u64,
    pub folders_scanned: u64,
    pub bytes_analyzed: u64,
    pub errors: u64,
    pub state: ScanState,
}

/// Storage broken down into the categories shown on Home. Categories are
/// derived from observed paths — never invented.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryBreakdown {
    pub category: StorageCategory,
    pub bytes: u64,
    pub file_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageCategory {
    Applications,
    System,
    Development,
    Documents,
    Games,
    TemporaryFiles,
    Other,
}

impl StorageCategory {
    pub fn all() -> [StorageCategory; 7] {
        use StorageCategory::*;
        [
            Applications,
            System,
            Development,
            Documents,
            Games,
            TemporaryFiles,
            Other,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Applications => "applications",
            Self::System => "system",
            Self::Development => "development",
            Self::Documents => "documents",
            Self::Games => "games",
            Self::TemporaryFiles => "temporaryFiles",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "applications" => Self::Applications,
            "system" => Self::System,
            "development" => Self::Development,
            "documents" => Self::Documents,
            "games" => Self::Games,
            "temporaryFiles" => Self::TemporaryFiles,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    pub path: String,
    pub name: String,
    pub extension: Option<String>,
    pub logical_size: u64,
    pub allocated_size: u64,
    pub created: Option<i64>,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exclusion {
    pub id: i64,
    pub pattern: String,
    pub kind: ExclusionKind,
    pub reason: ExclusionReason,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExclusionKind {
    File,
    Folder,
    Extension,
    Volume,
    Category,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExclusionReason {
    UserExclusion,
    ProtectedWindowsPath,
    ActiveApplication,
    SystemComponent,
    ReparsePoint,
    UnsupportedVolume,
}

impl ExclusionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserExclusion => "userExclusion",
            Self::ProtectedWindowsPath => "protectedWindowsPath",
            Self::ActiveApplication => "activeApplication",
            Self::SystemComponent => "systemComponent",
            Self::ReparsePoint => "reparsePoint",
            Self::UnsupportedVolume => "unsupportedVolume",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "protectedWindowsPath" => Self::ProtectedWindowsPath,
            "activeApplication" => Self::ActiveApplication,
            "systemComponent" => Self::SystemComponent,
            "reparsePoint" => Self::ReparsePoint,
            "unsupportedVolume" => Self::UnsupportedVolume,
            _ => Self::UserExclusion,
        }
    }
}

impl ExclusionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Folder => "folder",
            Self::Extension => "extension",
            Self::Volume => "volume",
            Self::Category => "category",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "file" => Self::File,
            "extension" => Self::Extension,
            "volume" => Self::Volume,
            "category" => Self::Category,
            _ => Self::Folder,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    pub root: String,
    pub follow_symlinks: bool,
    pub follow_junctions: bool,
    pub scan_hidden: bool,
    pub scan_system: bool,
    pub concurrency: usize,
    pub use_allocated_size: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            root: String::new(),
            // Reparse points are not followed by default: doing so risks
            // counting the same data twice and walking into loops.
            follow_symlinks: false,
            follow_junctions: false,
            scan_hidden: true,
            scan_system: false,
            concurrency: 4,
            use_allocated_size: true,
        }
    }
}
