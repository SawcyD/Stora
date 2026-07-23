use std::time::Instant;

use stora_core::cleanup::{
    CleanupItem, CleanupItemError, CleanupProgress, CleanupState, DeletionMethod,
};
use stora_core::{Result, StoraError, TaskControl};

/// How many items go into a single Recycle Bin shell operation.
const RECYCLE_BATCH: usize = 256;

const PROGRESS_INTERVAL_MS: u64 = 150;

/// Outcome of an execution run. Byte totals count removed items only.
#[derive(Debug, Default)]
pub struct ExecutionOutcome {
    pub removed: Vec<CleanupItem>,
    pub failed: Vec<(CleanupItem, CleanupItemError)>,
    pub recovered_bytes: u64,
    pub duration_ms: u64,
    pub cancelled: bool,
}

impl ExecutionOutcome {
    pub fn state(&self) -> CleanupState {
        if self.cancelled {
            CleanupState::Cancelling
        } else if self.failed.is_empty() {
            CleanupState::Completed
        } else {
            CleanupState::CompletedWithErrors
        }
    }
}

pub trait CleanupReporter {
    fn progress(&mut self, progress: &CleanupProgress);
}

/// Deletes the authorized items using the chosen method.
///
/// `items` must already have come from [`stora_security::authorize_selection`].
/// Every item is revalidated immediately before removal, so a file that
/// changed since the preview is skipped rather than deleted.
pub fn execute(
    task_id: &str,
    items: &[CleanupItem],
    method: DeletionMethod,
    quarantine_root: Option<&std::path::Path>,
    control: &TaskControl,
    reporter: &mut dyn CleanupReporter,
) -> Result<ExecutionOutcome> {
    let started = Instant::now();
    let mut outcome = ExecutionOutcome::default();
    let mut last_progress = Instant::now();
    let total = items.len() as u64;

    // The Recycle Bin path batches through a single shell operation, which is
    // dramatically faster and produces proper restore entries.
    if method == DeletionMethod::RecycleBin {
        for chunk in items.chunks(RECYCLE_BATCH) {
            if control.is_cancelled() {
                outcome.cancelled = true;
                break;
            }
            control.checkpoint()?;
            recycle_chunk(chunk, &mut outcome);

            report(
                reporter,
                task_id,
                &outcome,
                total,
                started,
                &mut last_progress,
                chunk.last().map(|i| i.path.as_str()).unwrap_or(""),
                true,
            );
        }
    } else {
        for item in items {
            if control.is_cancelled() {
                outcome.cancelled = true;
                break;
            }
            control.checkpoint()?;

            match remove_one(item, method, quarantine_root) {
                Ok(()) => {
                    outcome.recovered_bytes += item.size;
                    outcome.removed.push(item.clone());
                }
                Err(error) => record_failure(&mut outcome, item, error),
            }

            report(
                reporter,
                task_id,
                &outcome,
                total,
                started,
                &mut last_progress,
                &item.path,
                false,
            );
        }
    }

    outcome.duration_ms = started.elapsed().as_millis() as u64;

    reporter.progress(&CleanupProgress {
        task_id: task_id.to_string(),
        state: outcome.state(),
        completed: (outcome.removed.len() + outcome.failed.len()) as u64,
        total,
        recovered_bytes: outcome.recovered_bytes,
        current_path: String::new(),
        errors: outcome.failed.len() as u64,
        elapsed_ms: outcome.duration_ms,
    });

    Ok(outcome)
}

fn recycle_chunk(chunk: &[CleanupItem], outcome: &mut ExecutionOutcome) {
    // Revalidate first so a changed file never reaches the shell call.
    let mut valid = Vec::with_capacity(chunk.len());
    for item in chunk {
        match stora_security::revalidate(item) {
            Ok(()) => valid.push(item.clone()),
            Err(error) => record_failure(outcome, item, error),
        }
    }
    if valid.is_empty() {
        return;
    }

    let paths: Vec<String> = valid.iter().map(|item| item.path.clone()).collect();

    match stora_winapi::move_to_recycle_bin(&paths) {
        Ok(failures) => {
            for item in valid {
                match failures.iter().find(|(path, _)| *path == item.path) {
                    Some((_, error)) => {
                        let error = StoraError::Internal(error.to_string());
                        record_failure(outcome, &item, error);
                    }
                    None => {
                        // Confirm the file is actually gone before claiming
                        // its bytes as recovered.
                        if std::path::Path::new(&stora_security::to_extended_length(&item.path))
                            .exists()
                        {
                            record_failure(
                                outcome,
                                &item,
                                StoraError::Internal(
                                    "the item still exists after the operation".into(),
                                ),
                            );
                        } else {
                            outcome.recovered_bytes += item.size;
                            outcome.removed.push(item);
                        }
                    }
                }
            }
        }
        Err(error) => {
            for item in valid {
                record_failure(outcome, &item, StoraError::Internal(error.to_string()));
            }
        }
    }
}

fn remove_one(
    item: &CleanupItem,
    method: DeletionMethod,
    quarantine_root: Option<&std::path::Path>,
) -> Result<()> {
    // Defense in depth: the authorization layer already checked this, but the
    // filesystem may have changed since.
    stora_security::ensure_deletable(&item.path)?;
    stora_security::revalidate(item)?;

    let extended = stora_security::to_extended_length(&item.path);

    match method {
        DeletionMethod::Permanent => if item.is_directory {
            std::fs::remove_dir(&extended)
        } else {
            std::fs::remove_file(&extended)
        }
        .map_err(|err| classify_io(&err, &item.path)),
        DeletionMethod::Quarantine => {
            let root = quarantine_root.ok_or_else(|| {
                StoraError::Internal("quarantine is enabled but no folder is configured".into())
            })?;
            quarantine(item, root)
        }
        DeletionMethod::RecycleBin => {
            // Handled in batch by `recycle_chunk`.
            Err(StoraError::Internal(
                "recycle bin removal is performed in batches".into(),
            ))
        }
        DeletionMethod::WindowsCleanup | DeletionMethod::ApplicationCleanup => Err(
            StoraError::Internal("this category is handled by a supported external tool".into()),
        ),
    }
}

/// Moves an item into the quarantine folder, preserving its original location
/// in the file name so it can be restored.
fn quarantine(item: &CleanupItem, root: &std::path::Path) -> Result<()> {
    // Never copy credentials or key material into a second location.
    if stora_security::is_sensitive(&item.path) {
        return Err(StoraError::ProtectedPath {
            path: item.path.clone(),
        });
    }

    std::fs::create_dir_all(root).map_err(|err| {
        StoraError::Internal(format!("could not create quarantine folder: {err}"))
    })?;

    let target = root.join(quarantine_file_name(&item.path));

    // A rename is atomic on the same volume; fall back to copy-then-remove
    // when quarantine lives elsewhere.
    match std::fs::rename(stora_security::to_extended_length(&item.path), &target) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(stora_security::to_extended_length(&item.path), &target)
                .map_err(|err| classify_io(&err, &item.path))?;
            std::fs::remove_file(stora_security::to_extended_length(&item.path)).map_err(|err| {
                // The copy succeeded but the original survives; remove the
                // duplicate so quarantine does not consume extra space.
                let _ = std::fs::remove_file(&target);
                classify_io(&err, &item.path)
            })
        }
    }
}

/// Builds a collision-resistant quarantine file name that encodes the origin.
pub fn quarantine_file_name(original_path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    original_path.to_ascii_lowercase().hash(&mut hasher);
    let digest = hasher.finish();

    let name = stora_security::file_name_of(original_path);
    format!("{digest:016x}-{name}")
}

fn classify_io(err: &std::io::Error, path: &str) -> StoraError {
    // Windows reports a locked file as a permission error; the Restart Manager
    // tells us which it actually is.
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        if let Ok(holders) = stora_winapi::processes_locking(path) {
            if !holders.is_empty() {
                return StoraError::FileLocked { path: path.into() };
            }
        }
    }
    StoraError::from_io(err, path)
}

fn record_failure(outcome: &mut ExecutionOutcome, item: &CleanupItem, error: StoraError) {
    outcome.failed.push((
        item.clone(),
        CleanupItemError {
            path: item.path.clone(),
            code: error.code().to_string(),
            message: error.to_string(),
        },
    ));
}

#[allow(clippy::too_many_arguments)]
fn report(
    reporter: &mut dyn CleanupReporter,
    task_id: &str,
    outcome: &ExecutionOutcome,
    total: u64,
    started: Instant,
    last_progress: &mut Instant,
    current_path: &str,
    force: bool,
) {
    if !force && (last_progress.elapsed().as_millis() as u64) < PROGRESS_INTERVAL_MS {
        return;
    }
    *last_progress = Instant::now();

    reporter.progress(&CleanupProgress {
        task_id: task_id.to_string(),
        state: CleanupState::Cleaning,
        completed: (outcome.removed.len() + outcome.failed.len()) as u64,
        total,
        recovered_bytes: outcome.recovered_bytes,
        current_path: current_path.to_string(),
        errors: outcome.failed.len() as u64,
        elapsed_ms: started.elapsed().as_millis() as u64,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct SilentReporter {
        updates: Vec<CleanupProgress>,
    }

    impl CleanupReporter for SilentReporter {
        fn progress(&mut self, progress: &CleanupProgress) {
            self.updates.push(progress.clone());
        }
    }

    fn item_for(path: &std::path::Path, size: u64) -> CleanupItem {
        CleanupItem {
            path: path.to_string_lossy().replace('/', "\\"),
            category_id: "userTemp".into(),
            size,
            is_directory: false,
            modified: None,
        }
    }

    #[test]
    fn permanent_deletion_removes_files_and_counts_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.tmp");
        let b = dir.path().join("b.tmp");
        std::fs::write(&a, vec![0u8; 100]).unwrap();
        std::fs::write(&b, vec![0u8; 250]).unwrap();

        let items = vec![item_for(&a, 100), item_for(&b, 250)];
        let control = TaskControl::new();
        let mut reporter = SilentReporter::default();

        let outcome = execute(
            "task-1",
            &items,
            DeletionMethod::Permanent,
            None,
            &control,
            &mut reporter,
        )
        .unwrap();

        assert_eq!(outcome.removed.len(), 2);
        assert!(outcome.failed.is_empty());
        assert_eq!(outcome.recovered_bytes, 350);
        assert_eq!(outcome.state(), CleanupState::Completed);
        assert!(!a.exists() && !b.exists());
    }

    #[test]
    fn a_file_changed_after_preview_is_skipped_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("changed.tmp");
        std::fs::write(&file, vec![0u8; 100]).unwrap();

        // The plan recorded 100 bytes; the file has since grown.
        let items = vec![item_for(&file, 100)];
        std::fs::write(&file, vec![0u8; 900]).unwrap();

        let control = TaskControl::new();
        let mut reporter = SilentReporter::default();
        let outcome = execute(
            "task-1",
            &items,
            DeletionMethod::Permanent,
            None,
            &control,
            &mut reporter,
        )
        .unwrap();

        assert!(outcome.removed.is_empty());
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].1.code, "PathChangedAfterPreview");
        assert_eq!(outcome.recovered_bytes, 0);
        assert!(file.exists(), "the changed file must survive");
        assert_eq!(outcome.state(), CleanupState::CompletedWithErrors);
    }

    #[test]
    fn a_missing_file_is_reported_without_claiming_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone.tmp");

        let control = TaskControl::new();
        let mut reporter = SilentReporter::default();
        let outcome = execute(
            "task-1",
            &[item_for(&missing, 500)],
            DeletionMethod::Permanent,
            None,
            &control,
            &mut reporter,
        )
        .unwrap();

        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.recovered_bytes, 0);
    }

    #[test]
    fn protected_paths_are_refused_at_execution_time() {
        let item = CleanupItem {
            path: "C:\\Windows\\System32\\kernel32.dll".into(),
            category_id: "userTemp".into(),
            size: 100,
            is_directory: false,
            modified: None,
        };

        let control = TaskControl::new();
        let mut reporter = SilentReporter::default();
        let outcome = execute(
            "task-1",
            &[item],
            DeletionMethod::Permanent,
            None,
            &control,
            &mut reporter,
        )
        .unwrap();

        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].1.code, "ProtectedPath");
    }

    #[test]
    fn cancellation_stops_before_the_next_item() {
        let dir = tempfile::tempdir().unwrap();
        let mut items = Vec::new();
        for i in 0..10 {
            let path = dir.path().join(format!("f{i}.tmp"));
            std::fs::write(&path, vec![0u8; 10]).unwrap();
            items.push(item_for(&path, 10));
        }

        let control = TaskControl::new();
        control.cancel();

        let mut reporter = SilentReporter::default();
        let outcome = execute(
            "task-1",
            &items,
            DeletionMethod::Permanent,
            None,
            &control,
            &mut reporter,
        )
        .unwrap();

        assert!(outcome.cancelled);
        assert!(
            outcome.removed.is_empty(),
            "nothing deleted after cancelling"
        );
        assert_eq!(outcome.state(), CleanupState::Cancelling);
    }

    #[test]
    fn quarantine_moves_the_file_and_keeps_it_restorable() {
        let dir = tempfile::tempdir().unwrap();
        let quarantine_dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.tmp");
        std::fs::write(&file, vec![0u8; 100]).unwrap();

        let control = TaskControl::new();
        let mut reporter = SilentReporter::default();
        let outcome = execute(
            "task-1",
            &[item_for(&file, 100)],
            DeletionMethod::Quarantine,
            Some(quarantine_dir.path()),
            &control,
            &mut reporter,
        )
        .unwrap();

        assert_eq!(outcome.removed.len(), 1);
        assert!(!file.exists(), "the original is moved, not copied");

        let quarantined: Vec<_> = std::fs::read_dir(quarantine_dir.path())
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(std::fs::metadata(quarantined[0].path()).unwrap().len(), 100);
    }

    #[test]
    fn quarantine_refuses_credential_files() {
        let item = CleanupItem {
            path: "C:\\Users\\Test\\.ssh\\id_rsa".into(),
            category_id: "userTemp".into(),
            size: 100,
            is_directory: false,
            modified: None,
        };
        let quarantine_dir = tempfile::tempdir().unwrap();

        let control = TaskControl::new();
        let mut reporter = SilentReporter::default();
        let outcome = execute(
            "task-1",
            &[item],
            DeletionMethod::Quarantine,
            Some(quarantine_dir.path()),
            &control,
            &mut reporter,
        )
        .unwrap();

        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.recovered_bytes, 0);
    }

    #[test]
    fn quarantine_without_a_configured_folder_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.tmp");
        std::fs::write(&file, b"data").unwrap();

        let control = TaskControl::new();
        let mut reporter = SilentReporter::default();
        let outcome = execute(
            "task-1",
            &[item_for(&file, 4)],
            DeletionMethod::Quarantine,
            None,
            &control,
            &mut reporter,
        )
        .unwrap();

        assert_eq!(outcome.failed.len(), 1);
        assert!(file.exists(), "the file must survive a configuration error");
    }

    #[test]
    fn quarantine_names_encode_origin_and_avoid_collisions() {
        let a = quarantine_file_name("C:\\Temp\\cache.bin");
        let b = quarantine_file_name("D:\\Other\\cache.bin");

        assert!(a.ends_with("cache.bin"));
        assert!(b.ends_with("cache.bin"));
        assert_ne!(
            a, b,
            "same file name from different folders must not collide"
        );

        // Stable for the same input, and case-insensitive like NTFS.
        assert_eq!(
            a,
            quarantine_file_name("c:\\temp\\CACHE.BIN".to_lowercase().as_str())
        );
    }

    #[test]
    fn an_empty_selection_completes_without_work() {
        let control = TaskControl::new();
        let mut reporter = SilentReporter::default();
        let outcome = execute(
            "task-1",
            &[],
            DeletionMethod::Permanent,
            None,
            &control,
            &mut reporter,
        )
        .unwrap();

        assert_eq!(outcome.recovered_bytes, 0);
        assert_eq!(outcome.state(), CleanupState::Completed);
    }

    #[test]
    fn progress_is_reported_at_least_once_at_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.tmp");
        std::fs::write(&file, b"data").unwrap();

        let control = TaskControl::new();
        let mut reporter = SilentReporter::default();
        execute(
            "task-1",
            &[item_for(&file, 4)],
            DeletionMethod::Permanent,
            None,
            &control,
            &mut reporter,
        )
        .unwrap();

        let final_update = reporter.updates.last().expect("a final update");
        assert_eq!(final_update.state, CleanupState::Completed);
        assert_eq!(final_update.completed, 1);
        assert_eq!(final_update.recovered_bytes, 4);
    }
}
