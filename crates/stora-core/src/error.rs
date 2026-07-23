use serde::Serialize;

/// Typed error surface shared by every Stora crate.
///
/// Variants map one-to-one onto a user-facing message in the frontend; raw
/// `io::Error` text and stack traces never reach the UI.
#[derive(Debug, thiserror::Error)]
pub enum StoraError {
    #[error("access denied: {path}")]
    AccessDenied { path: String },

    #[error("path not found: {path}")]
    PathNotFound { path: String },

    #[error("volume unavailable: {volume}")]
    VolumeUnavailable { volume: String },

    #[error("reparse point loop detected at {path}")]
    ReparseLoop { path: String },

    #[error("file is locked: {path}")]
    FileLocked { path: String },

    #[error("operation requires elevation")]
    ElevationRequired,

    #[error("cleanup plan {plan_id} has expired")]
    CleanupPlanExpired { plan_id: String },

    #[error("path changed after preview: {path}")]
    PathChangedAfterPreview { path: String },

    #[error("path is not authorized by the cleanup plan: {path}")]
    PathNotAuthorized { path: String },

    #[error("path is protected and cannot be modified: {path}")]
    ProtectedPath { path: String },

    #[error("invalid path: {reason}")]
    InvalidPath { reason: String },

    #[error("database is busy")]
    DatabaseBusy,

    #[error("database error: {0}")]
    Database(String),

    #[error("scan was cancelled")]
    ScanCancelled,

    #[error("unsupported filesystem: {filesystem}")]
    UnsupportedFilesystem { filesystem: String },

    #[error("task {task_id} was not found")]
    TaskNotFound { task_id: String },

    #[error("a {kind} task is already running")]
    TaskAlreadyRunning { kind: String },

    #[error("windows api error: {0}")]
    Windows(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, StoraError>;

/// Machine-readable code so the frontend can localize without parsing text.
impl StoraError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AccessDenied { .. } => "AccessDenied",
            Self::PathNotFound { .. } => "PathNotFound",
            Self::VolumeUnavailable { .. } => "VolumeUnavailable",
            Self::ReparseLoop { .. } => "ReparseLoop",
            Self::FileLocked { .. } => "FileLocked",
            Self::ElevationRequired => "ElevationRequired",
            Self::CleanupPlanExpired { .. } => "CleanupPlanExpired",
            Self::PathChangedAfterPreview { .. } => "PathChangedAfterPreview",
            Self::PathNotAuthorized { .. } => "PathNotAuthorized",
            Self::ProtectedPath { .. } => "ProtectedPath",
            Self::InvalidPath { .. } => "InvalidPath",
            Self::DatabaseBusy => "DatabaseBusy",
            Self::Database(_) => "Database",
            Self::ScanCancelled => "ScanCancelled",
            Self::UnsupportedFilesystem { .. } => "UnsupportedFilesystem",
            Self::TaskNotFound { .. } => "TaskNotFound",
            Self::TaskAlreadyRunning { .. } => "TaskAlreadyRunning",
            Self::Windows(_) => "Windows",
            Self::Internal(_) => "Internal",
        }
    }

    /// The path this error concerns, when the UI can usefully show one.
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::AccessDenied { path }
            | Self::PathNotFound { path }
            | Self::ReparseLoop { path }
            | Self::FileLocked { path }
            | Self::PathChangedAfterPreview { path }
            | Self::PathNotAuthorized { path }
            | Self::ProtectedPath { path } => Some(path),
            _ => None,
        }
    }

    pub fn from_io(err: &std::io::Error, path: &str) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::PathNotFound { path: path.into() },
            std::io::ErrorKind::PermissionDenied => Self::AccessDenied { path: path.into() },
            _ => Self::Internal(format!("{path}: {err}")),
        }
    }
}

/// Wire representation sent to the frontend. Deliberately structured rather
/// than a formatted string so the UI controls all presentation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

impl From<StoraError> for ErrorPayload {
    fn from(err: StoraError) -> Self {
        Self {
            code: err.code().to_string(),
            message: err.to_string(),
            path: err.path().map(str::to_string),
        }
    }
}

impl serde::Serialize for StoraError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let payload = ErrorPayload {
            code: self.code().to_string(),
            message: self.to_string(),
            path: self.path().map(str::to_string),
        };
        payload.serialize(serializer)
    }
}
