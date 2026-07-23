use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use stora_core::model::{
    DriveInfo, FileEntry, FolderAggregate, ScanOptions, ScanProgress, ScanState, ScanSummary,
    StorageCategory,
};
use stora_core::{Result, StoraError, TaskControl};
use stora_index::Index;
use stora_scanner::{ScanSink, ScanTotals, Walker};

use crate::state::AppState;

/// Event name the frontend listens on for scan progress.
pub const SCAN_PROGRESS_EVENT: &str = "stora://scan-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStarted {
    pub task_id: String,
    pub scan_id: i64,
}

/// Persists scanner output and forwards progress to the UI.
struct TauriSink {
    app: AppHandle,
    index: Arc<Index>,
    scan_id: i64,
    task_id: String,
    root: String,
}

impl ScanSink for TauriSink {
    fn entries(&mut self, entries: &[(FileEntry, StorageCategory)]) -> Result<()> {
        self.index.insert_entries(self.scan_id, entries)
    }

    fn aggregates(&mut self, aggregates: &[FolderAggregate]) -> Result<()> {
        self.index.insert_aggregates(self.scan_id, aggregates)
    }

    fn categories(&mut self, totals: &[(StorageCategory, u64, u64)]) -> Result<()> {
        self.index.insert_category_totals(self.scan_id, totals)
    }

    fn error(&mut self, path: &str, error: &StoraError) {
        // A scan error is recorded and the walk continues; only a failure to
        // record it is worth logging.
        if let Err(err) = self.index.record_scan_error(self.scan_id, path, error) {
            tracing::warn!(?err, "could not record scan error");
        }
    }

    fn progress(
        &mut self,
        state: ScanState,
        totals: ScanTotals,
        current_path: &str,
        elapsed_ms: u64,
    ) {
        let payload = ScanProgress {
            task_id: self.task_id.clone(),
            state,
            root: self.root.clone(),
            files_scanned: totals.files,
            folders_scanned: totals.folders,
            bytes_analyzed: totals.bytes,
            current_path: current_path.to_string(),
            errors: totals.errors,
            elapsed_ms,
        };
        let _ = self.app.emit(SCAN_PROGRESS_EVENT, payload);
    }
}

/// Lists local volumes and refreshes their stored capacity figures.
#[tauri::command]
pub fn list_drives(state: State<'_, AppState>) -> Result<Vec<DriveInfo>> {
    let drives = stora_winapi::enumerate_drives()?;
    state
        .index
        .upsert_drives(&drives, stora_core::now_seconds())?;
    Ok(drives)
}

/// Starts a scan on a background thread and returns immediately.
#[tauri::command]
pub fn start_scan(app: AppHandle, state: State<'_, AppState>, root: String) -> Result<ScanStarted> {
    let root = stora_security::normalize(&root)?;

    // Confirm the volume exists before creating a scan row.
    stora_winapi::drive_for_path(&root)?;

    let settings = state.settings()?;
    let exclusions = state.exclusion_set()?;

    let task_id = format!("scan-{}", stora_core::now_seconds());
    let control = state.tasks.register("scan", &task_id)?;

    let scan_id = state.index.begin_scan(&root, stora_core::now_seconds())?;
    state.set_active_scan(scan_id);

    let options = ScanOptions {
        root: root.clone(),
        follow_symlinks: settings.follow_symlinks,
        follow_junctions: settings.follow_junctions,
        scan_hidden: settings.scan_hidden_files,
        scan_system: settings.scan_system_files,
        concurrency: settings.scan_concurrency as usize,
        use_allocated_size: settings.use_allocated_size,
    };

    let index = Arc::clone(&state.index);
    let tasks = Arc::clone(&state.tasks);
    let thread_task_id = task_id.clone();

    // Filesystem walking is blocking work; keeping it off the async runtime
    // and off the UI thread is what keeps the window responsive.
    std::thread::Builder::new()
        .name("stora-scanner".into())
        .spawn(move || {
            run_scan(
                app,
                index,
                tasks,
                control,
                thread_task_id,
                scan_id,
                root,
                options,
                exclusions,
            );
        })
        .map_err(|err| StoraError::Internal(format!("could not start scan thread: {err}")))?;

    Ok(ScanStarted { task_id, scan_id })
}

#[allow(clippy::too_many_arguments)]
fn run_scan(
    app: AppHandle,
    index: Arc<Index>,
    tasks: Arc<stora_core::TaskRegistry>,
    control: Arc<TaskControl>,
    task_id: String,
    scan_id: i64,
    root: String,
    options: ScanOptions,
    exclusions: stora_security::ExclusionSet,
) {
    let mut sink = TauriSink {
        app: app.clone(),
        index: Arc::clone(&index),
        scan_id,
        task_id: task_id.clone(),
        root: root.clone(),
    };

    let mut walker = Walker::new(options, &exclusions, &control, &mut sink);
    let outcome = walker.run();

    // Always flush: a cancelled scan's partial results are still useful.
    if let Err(err) = walker.flush() {
        tracing::error!(?err, "could not flush scan results");
    }

    let totals = walker.totals();
    let elapsed = walker.elapsed_ms();

    let state = match &outcome {
        Ok(_) => ScanState::Completed,
        Err(StoraError::ScanCancelled) => ScanState::Idle,
        Err(_) => ScanState::Failed,
    };

    if let Err(err) = index.finish_scan(
        scan_id,
        stora_core::now_seconds(),
        elapsed,
        totals.files,
        totals.folders,
        totals.bytes,
        totals.errors,
        state,
    ) {
        tracing::error!(?err, "could not finalize scan");
    }

    // Growth compares snapshots rather than tracking every filesystem change.
    // A completed scan gives us a fresh, low-cost observation; cap it at one
    // snapshot per folder per day so frequent manual scans do not distort the
    // history or grow the database unnecessarily.
    if state == ScanState::Completed {
        record_daily_growth_snapshots(&index, scan_id, &root, stora_core::now_seconds());
    }

    // Keep only a bounded history so the database does not grow without limit.
    if let Err(err) = index.prune_scans(&root, 2) {
        tracing::warn!(?err, "could not prune old scans");
    }

    tasks.finish(&task_id);

    let _ = app.emit(
        SCAN_PROGRESS_EVENT,
        ScanProgress {
            task_id,
            state,
            root,
            files_scanned: totals.files,
            folders_scanned: totals.folders,
            bytes_analyzed: totals.bytes,
            current_path: String::new(),
            errors: totals.errors,
            elapsed_ms: elapsed,
        },
    );
}

fn record_daily_growth_snapshots(index: &Index, scan_id: i64, root: &str, now: i64) {
    let Ok(children) = index.folder_children(scan_id, root) else {
        return;
    };

    for folder in children.iter().take(200) {
        let should_record = index
            .folder_snapshots(&folder.path)
            .map(|snapshots| {
                snapshots
                    .last()
                    .is_none_or(|(taken_at, _)| now.saturating_sub(*taken_at) >= 86_400)
            })
            .unwrap_or(false);
        if should_record {
            if let Err(error) =
                index.record_folder_snapshot(&folder.path, now, folder.allocated_size)
            {
                tracing::warn!(?error, path = %folder.path, "could not record growth snapshot");
            }
        }
    }
}

#[tauri::command]
pub fn pause_scan(state: State<'_, AppState>, task_id: String) -> Result<()> {
    state.tasks.get(&task_id)?.pause();
    Ok(())
}

#[tauri::command]
pub fn resume_scan(state: State<'_, AppState>, task_id: String) -> Result<()> {
    state.tasks.get(&task_id)?.resume();
    Ok(())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>, task_id: String) -> Result<()> {
    state.tasks.get(&task_id)?.cancel();
    Ok(())
}

#[tauri::command]
pub fn get_scan_summary(state: State<'_, AppState>, root: String) -> Result<Option<ScanSummary>> {
    let root = stora_security::normalize(&root)?;
    state.index.latest_scan(&root)
}
