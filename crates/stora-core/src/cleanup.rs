use serde::{Deserialize, Serialize};

/// How confident Stora is that removing a category is safe, and how much the
/// user must think about it. Deliberately not a "health score".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    Low,
    Moderate,
    Advanced,
    UserReviewRequired,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Moderate => "moderate",
            Self::Advanced => "advanced",
            Self::UserReviewRequired => "userReviewRequired",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupTier {
    /// Regeneratable data; safe to select by default.
    Safe,
    /// May contain data the user wants; never selected by default.
    ReviewRequired,
    /// Touches Windows servicing state; disabled unless explicitly enabled.
    Advanced,
}

/// A cleanup category definition — the static description of what a category
/// is, independent of any particular scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCategory {
    pub id: String,
    pub name: String,
    /// Plain explanation of what happens after removal. No scare wording.
    pub explanation: String,
    pub tier: CleanupTier,
    pub risk: RiskLevel,
    /// True when Windows offers a supported mechanism we should prefer over
    /// deleting files ourselves.
    pub prefers_windows_mechanism: bool,
    pub learn_more: Option<String>,
}

/// A category with the results of actually looking at this machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCategoryResult {
    #[serde(flatten)]
    pub category: CleanupCategory,
    pub bytes: u64,
    pub file_count: u64,
    pub folder_count: u64,
    /// Set when the category could not be inspected (e.g. access denied).
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItem {
    pub path: String,
    pub category_id: String,
    pub size: u64,
    pub is_directory: bool,
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeletionMethod {
    RecycleBin,
    Permanent,
    Quarantine,
    WindowsCleanup,
    ApplicationCleanup,
}

impl DeletionMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RecycleBin => "recycleBin",
            Self::Permanent => "permanent",
            Self::Quarantine => "quarantine",
            Self::WindowsCleanup => "windowsCleanup",
            Self::ApplicationCleanup => "applicationCleanup",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "permanent" => Self::Permanent,
            "quarantine" => Self::Quarantine,
            "windowsCleanup" => Self::WindowsCleanup,
            "applicationCleanup" => Self::ApplicationCleanup,
            _ => Self::RecycleBin,
        }
    }
}

/// The authoritative, backend-owned set of paths a cleanup run may touch.
///
/// The frontend never supplies paths to delete: it supplies a plan id and a
/// subset of item indices, and the backend re-derives everything else.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    pub plan_id: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub categories: Vec<CleanupCategoryResult>,
    pub items: Vec<CleanupItem>,
    pub total_bytes: u64,
    pub file_count: u64,
    pub folder_count: u64,
}

impl CleanupPlan {
    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupState {
    Idle,
    Preparing,
    AwaitingApproval,
    Cleaning,
    Cancelling,
    Completed,
    CompletedWithErrors,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupProgress {
    pub task_id: String,
    pub state: CleanupState,
    pub completed: u64,
    pub total: u64,
    /// Only counts bytes for items that were actually removed.
    pub recovered_bytes: u64,
    pub current_path: String,
    pub errors: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItemError {
    pub path: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub operation_id: i64,
    pub state: CleanupState,
    /// Bytes for successfully removed items only — never the selected total.
    pub recovered_bytes: u64,
    pub files_removed: u64,
    pub files_skipped: u64,
    pub duration_ms: u64,
    pub method: DeletionMethod,
    pub errors: Vec<CleanupItemError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupHistoryEntry {
    pub operation_id: i64,
    pub started_at: i64,
    pub duration_ms: u64,
    pub categories: Vec<String>,
    pub files_selected: u64,
    pub files_removed: u64,
    pub files_skipped: u64,
    pub recovered_bytes: u64,
    pub method: DeletionMethod,
    pub error_count: u64,
    pub automation_rule: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineItem {
    pub id: i64,
    pub original_path: String,
    pub quarantine_path: String,
    pub size: u64,
    pub quarantined_at: i64,
    pub expires_at: Option<i64>,
}
