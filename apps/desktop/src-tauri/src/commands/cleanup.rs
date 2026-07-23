use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use stora_cleaner::{CleanupReporter, PlanRequest};
use stora_core::cleanup::{
    CleanupCategory, CleanupHistoryEntry, CleanupItem, CleanupItemError, CleanupPlan,
    CleanupProgress, CleanupResult, CleanupState, DeletionMethod,
};
use stora_core::{Result, StoraError};

use crate::state::AppState;

pub const CLEANUP_PROGRESS_EVENT: &str = "stora://cleanup-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlanResponse {
    pub plan: CleanupPlan,
    /// Indices the UI should preselect — safe, low-risk categories only.
    pub default_selection: Vec<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRequest {
    pub plan_id: String,
    /// Indices into the plan's item list. Paths are never accepted here.
    pub selected_indices: Vec<usize>,
    pub method: String,
}

struct TauriReporter {
    app: AppHandle,
}

impl CleanupReporter for TauriReporter {
    fn progress(&mut self, progress: &CleanupProgress) {
        let _ = self.app.emit(CLEANUP_PROGRESS_EVENT, progress.clone());
    }
}

/// The full catalogue of cleanup categories, independent of any scan.
#[tauri::command]
pub fn get_cleanup_categories() -> Vec<CleanupCategory> {
    stora_cleaner::all_categories()
}

/// Inspects the requested categories and returns an authoritative plan.
#[tauri::command]
pub fn build_cleanup_plan(
    state: State<'_, AppState>,
    category_ids: Vec<String>,
) -> Result<CleanupPlanResponse> {
    let settings = state.settings()?;
    let exclusions = state.exclusion_set()?;

    let task_id = format!("plan-{}", stora_core::now_seconds());
    let control = state.tasks.register("plan", &task_id)?;

    let request = PlanRequest {
        category_ids,
        include_advanced: settings.show_advanced_categories,
    };

    let outcome =
        stora_cleaner::build_plan(&request, &exclusions, &control, stora_core::now_seconds());
    state.tasks.finish(&task_id);

    let plan = outcome?;
    let default_selection = stora_cleaner::default_selection(&plan);
    state.store_plan(plan.clone());

    Ok(CleanupPlanResponse {
        plan,
        default_selection,
    })
}

/// Executes an approved subset of a plan.
///
/// The selection is resolved and re-authorized here in Rust; the frontend's
/// claim about what is safe to delete is never trusted on its own.
#[tauri::command]
pub async fn execute_cleanup_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ExecuteRequest,
) -> Result<CleanupResult> {
    let plan = state.take_plan(&request.plan_id)?;
    let method = DeletionMethod::parse(&request.method);
    let settings = state.settings()?;

    let authorized = stora_security::authorize_selection(
        &plan,
        &request.selected_indices,
        stora_core::now_seconds(),
    )?;

    if authorized.is_empty() {
        return Err(StoraError::Internal(
            "no items were selected for cleanup".into(),
        ));
    }

    // Quarantine must be explicitly enabled before it can be used.
    if method == DeletionMethod::Quarantine && !settings.enable_quarantine {
        return Err(StoraError::Internal(
            "quarantine is turned off in Settings".into(),
        ));
    }

    let categories: Vec<String> = {
        let mut ids: Vec<String> = authorized
            .iter()
            .map(|item| item.category_id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };

    let task_id = format!("cleanup-{}", stora_core::now_seconds());
    let control = state.tasks.register("cleanup", &task_id)?;

    let operation_id = state.index.begin_cleanup(
        &plan.plan_id,
        stora_core::now_seconds(),
        method,
        authorized.len() as u64,
        &categories,
    )?;

    let index = Arc::clone(&state.index);
    let tasks = Arc::clone(&state.tasks);
    let quarantine_dir = state.quarantine_dir();
    let thread_task_id = task_id.clone();

    // Deleting thousands of files is blocking work; run it off the UI thread
    // and await the join so the command still returns a single result.
    let handle = tokio::task::spawn_blocking(move || {
        let mut reporter = TauriReporter { app };
        let outcome = stora_cleaner::execute(
            &thread_task_id,
            &authorized,
            method,
            Some(quarantine_dir.as_path()),
            &control,
            &mut reporter,
        );
        tasks.finish(&thread_task_id);
        (outcome, index)
    });

    let (outcome, index) = handle
        .await
        .map_err(|err| StoraError::Internal(format!("cleanup task failed: {err}")))?;
    let outcome = outcome?;

    index.record_cleanup_items(operation_id, &outcome.removed, &outcome.failed)?;

    // Quarantined files need a record, or they could never be restored.
    if method == DeletionMethod::Quarantine {
        let quarantine_dir = state.quarantine_dir();
        let now = stora_core::now_seconds();
        let expires_at = if settings.quarantine_retention_days > 0 {
            Some(now + settings.quarantine_retention_days as i64 * 86_400)
        } else {
            // Zero means "keep until manually removed".
            None
        };

        for item in &outcome.removed {
            let stored = quarantine_dir
                .join(stora_cleaner::execute::quarantine_file_name(&item.path))
                .to_string_lossy()
                .replace('/', "\\");

            if let Err(err) = index.record_quarantine(
                Some(operation_id),
                &item.path,
                &stored,
                item.size,
                now,
                expires_at,
            ) {
                tracing::error!(?err, "could not record a quarantined file");
            }
        }
    }

    let errors: Vec<CleanupItemError> = outcome
        .failed
        .iter()
        .map(|(_, error)| error.clone())
        .collect();

    let result = CleanupResult {
        operation_id,
        state: match outcome.state() {
            // A cancelled run still completed the items it processed.
            CleanupState::Cancelling if errors.is_empty() => CleanupState::Completed,
            CleanupState::Cancelling => CleanupState::CompletedWithErrors,
            other => other,
        },
        // Only successfully removed items contribute to this figure.
        recovered_bytes: outcome.recovered_bytes,
        files_removed: outcome.removed.len() as u64,
        files_skipped: outcome.failed.len() as u64,
        duration_ms: outcome.duration_ms,
        method,
        errors,
    };

    index.finish_cleanup(&result)?;
    Ok(result)
}

#[tauri::command]
pub fn cancel_cleanup(state: State<'_, AppState>, task_id: String) -> Result<()> {
    state.tasks.get(&task_id)?.cancel();
    Ok(())
}

/// Files in a plan belonging to one category, for the preview's detail view.
#[tauri::command]
pub fn get_plan_items(
    state: State<'_, AppState>,
    plan_id: String,
    category_id: String,
    limit: usize,
) -> Result<Vec<CleanupItem>> {
    let plan = state.take_plan(&plan_id)?;
    Ok(plan
        .items
        .into_iter()
        .filter(|item| item.category_id == category_id)
        .take(limit.min(2_000))
        .collect())
}

#[tauri::command]
pub fn get_cleanup_history(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<CleanupHistoryEntry>> {
    state.index.cleanup_history(limit.min(500))
}

#[tauri::command]
pub fn get_cleanup_errors(
    state: State<'_, AppState>,
    operation_id: i64,
) -> Result<Vec<CleanupItemError>> {
    state.index.cleanup_errors(operation_id)
}

/// Processes currently holding a file open, so the UI can explain a failure.
#[tauri::command]
pub fn get_locking_processes(path: String) -> Result<Vec<String>> {
    let path = stora_security::normalize(&path)?;
    stora_winapi::processes_locking(&path)
}
