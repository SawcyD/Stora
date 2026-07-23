use serde::Serialize;
use tauri::State;

use stora_core::{Result, StoraError};
use stora_knowledge::Explanation;

use crate::state::AppState;

/// Explains what writes to a location and what happens if it goes.
///
/// Entirely offline and deterministic: it reads a curated, checked-in file.
/// A location with no entry returns `entry: null`, and the interface says
/// "No information available" rather than guessing.
#[tauri::command]
pub fn explain_location(path: String) -> Explanation {
    stora_knowledge::explain(&path)
}

/// A conservative answer from Stora's local policy and curated knowledge.
/// It has no authority to delete or otherwise modify a path.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorAnswer {
    pub path: String,
    /// `doNotRemove`, `reviewFirst`, or `unknown`.
    pub verdict: String,
    pub summary: String,
    pub reasons: Vec<String>,
    pub source_title: Option<String>,
    pub source_url: Option<String>,
    pub local_only: bool,
}

#[tauri::command]
pub fn advise_path(path: String) -> AdvisorAnswer {
    let normalized = stora_security::normalize(&path).unwrap_or(path);

    if stora_security::is_sensitive(&normalized) {
        return AdvisorAnswer {
            path: normalized,
            verdict: "doNotRemove".into(),
            summary: "This location appears to contain credentials or key material. Stora will not inspect, copy, or remove it through Advisor.".into(),
            reasons: vec!["Sensitive credential and key locations are hard-blocked by Stora's safety policy.".into()],
            source_title: None,
            source_url: None,
            local_only: true,
        };
    }

    if stora_security::is_protected(&normalized) {
        return AdvisorAnswer {
            path: normalized,
            verdict: "doNotRemove".into(),
            summary: "This is a protected Windows location. Stora will not offer it for removal.".into(),
            reasons: vec!["Protected Windows, boot, recovery, and program locations cannot be overridden by confirmations or Advisor output.".into()],
            source_title: None,
            source_url: None,
            local_only: true,
        };
    }

    match stora_knowledge::explain(&normalized).entry {
        Some(entry) if entry.removable => AdvisorAnswer {
            path: normalized,
            verdict: "reviewFirst".into(),
            summary: format!(
                "{} may be removable, but review it before taking action.",
                entry.title
            ),
            reasons: vec![entry.written_by, entry.if_removed],
            source_title: Some(entry.source_title),
            source_url: Some(entry.source_url),
            local_only: true,
        },
        Some(entry) => AdvisorAnswer {
            path: normalized,
            verdict: "doNotRemove".into(),
            summary: format!("{} should not be removed directly.", entry.title),
            reasons: vec![entry.written_by, entry.if_removed],
            source_title: Some(entry.source_title),
            source_url: Some(entry.source_url),
            local_only: true,
        },
        None => AdvisorAnswer {
            path: normalized,
            verdict: "unknown".into(),
            summary: "Stora cannot safely identify this location from local evidence.".into(),
            reasons: vec![
                "No curated source applies to this path. Unknown does not mean safe to delete."
                    .into(),
            ],
            source_title: None,
            source_url: None,
            local_only: true,
        },
    }
}

/// Explicit cloud fallback for an unknown path. Protected and sensitive paths
/// are rejected before a request is made, regardless of the saved API key.
#[tauri::command]
pub async fn research_advisor_path(path: String) -> Result<crate::advisor::ResearchAnswer> {
    let normalized = stora_security::normalize(&path)?;
    if stora_security::is_sensitive(&normalized) || stora_security::is_protected(&normalized) {
        return Err(StoraError::ProtectedPath { path: normalized });
    }
    crate::advisor::research_path(&normalized).await
}

/// Number of curated entries currently loaded, for the About page.
#[tauri::command]
pub fn knowledge_entry_count(state: State<'_, AppState>) -> Result<u64> {
    state.index.knowledge_entry_count()
}
