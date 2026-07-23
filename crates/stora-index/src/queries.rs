use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use stora_core::cleanup::{
    CleanupHistoryEntry, CleanupItem, CleanupItemError, CleanupResult, CleanupState, DeletionMethod,
};
use stora_core::model::{
    CategoryBreakdown, DriveInfo, Exclusion, ExclusionKind, ExclusionReason, FileEntry,
    FolderAggregate, LargeFile, ScanState, ScanSummary, StorageCategory,
};
use stora_core::{Result, StoraError};

use crate::migrations::map_err;
use crate::Index;

fn scan_state_str(state: ScanState) -> &'static str {
    match state {
        ScanState::Idle => "idle",
        ScanState::Preparing => "preparing",
        ScanState::Scanning => "scanning",
        ScanState::Paused => "paused",
        ScanState::Cancelling => "cancelling",
        ScanState::Completed => "completed",
        ScanState::Failed => "failed",
    }
}

fn parse_scan_state(value: &str) -> ScanState {
    match value {
        "scanning" => ScanState::Scanning,
        "paused" => ScanState::Paused,
        "cancelling" => ScanState::Cancelling,
        "completed" => ScanState::Completed,
        "failed" => ScanState::Failed,
        "preparing" => ScanState::Preparing,
        _ => ScanState::Idle,
    }
}

fn cleanup_state_str(state: CleanupState) -> &'static str {
    match state {
        CleanupState::Idle => "idle",
        CleanupState::Preparing => "preparing",
        CleanupState::AwaitingApproval => "awaitingApproval",
        CleanupState::Cleaning => "cleaning",
        CleanupState::Cancelling => "cancelling",
        CleanupState::Completed => "completed",
        CleanupState::CompletedWithErrors => "completedWithErrors",
        CleanupState::Failed => "failed",
    }
}

impl Index {
    // ---------------------------------------------------------------- drives

    pub fn upsert_drives(&self, drives: &[DriveInfo], now: i64) -> Result<()> {
        self.with(|connection| {
            let tx = connection.transaction().map_err(map_err)?;
            {
                let mut statement = tx
                    .prepare(
                        "INSERT INTO drives (root, label, filesystem, total_bytes, free_bytes, \
                         drive_type, is_removable, last_seen) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                         ON CONFLICT (root) DO UPDATE SET \
                           label = excluded.label, \
                           filesystem = excluded.filesystem, \
                           total_bytes = excluded.total_bytes, \
                           free_bytes = excluded.free_bytes, \
                           drive_type = excluded.drive_type, \
                           is_removable = excluded.is_removable, \
                           last_seen = excluded.last_seen",
                    )
                    .map_err(map_err)?;

                for drive in drives {
                    statement
                        .execute(params![
                            drive.root,
                            drive.label,
                            drive.filesystem,
                            drive.total_bytes as i64,
                            drive.free_bytes as i64,
                            format!("{:?}", drive.drive_type).to_lowercase(),
                            drive.is_removable as i32,
                            now,
                        ])
                        .map_err(map_err)?;
                }
            }
            tx.commit().map_err(map_err)
        })
    }

    // ----------------------------------------------------------------- scans

    pub fn begin_scan(&self, root: &str, started_at: i64) -> Result<i64> {
        self.with(|connection| {
            connection
                .execute(
                    "INSERT INTO scans (root, started_at, state) VALUES (?1, ?2, 'scanning')",
                    params![root, started_at],
                )
                .map_err(map_err)?;
            Ok(connection.last_insert_rowid())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_scan(
        &self,
        scan_id: i64,
        finished_at: i64,
        duration_ms: u64,
        files: u64,
        folders: u64,
        bytes: u64,
        errors: u64,
        state: ScanState,
    ) -> Result<()> {
        self.with(|connection| {
            connection
                .execute(
                    "UPDATE scans SET finished_at = ?2, duration_ms = ?3, files_scanned = ?4, \
                     folders_scanned = ?5, bytes_analyzed = ?6, error_count = ?7, state = ?8 \
                     WHERE id = ?1",
                    params![
                        scan_id,
                        finished_at,
                        duration_ms as i64,
                        files as i64,
                        folders as i64,
                        bytes as i64,
                        errors as i64,
                        scan_state_str(state),
                    ],
                )
                .map_err(map_err)?;
            Ok(())
        })
    }

    /// Writes a batch of file records in one transaction.
    ///
    /// The scanner calls this every few thousand entries; a transaction per
    /// file would make a full-drive scan take hours.
    pub fn insert_entries(
        &self,
        scan_id: i64,
        entries: &[(FileEntry, StorageCategory)],
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        self.with(|connection| {
            let tx = connection.transaction().map_err(map_err)?;
            {
                let mut statement = tx
                    .prepare_cached(
                        "INSERT INTO scan_entries (scan_id, path, parent_path, name, extension, \
                         logical_size, allocated_size, created, modified, accessed, attributes, \
                         is_directory, is_reparse, category) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    )
                    .map_err(map_err)?;

                for (entry, category) in entries {
                    statement
                        .execute(params![
                            scan_id,
                            entry.path,
                            entry.parent_path,
                            entry.name,
                            entry.extension,
                            entry.logical_size as i64,
                            entry.allocated_size as i64,
                            entry.created,
                            entry.modified,
                            entry.accessed,
                            entry.attributes as i64,
                            entry.is_directory as i32,
                            entry.is_reparse_point as i32,
                            category.as_str(),
                        ])
                        .map_err(map_err)?;
                }
            }
            tx.commit().map_err(map_err)
        })
    }

    pub fn insert_aggregates(&self, scan_id: i64, aggregates: &[FolderAggregate]) -> Result<()> {
        if aggregates.is_empty() {
            return Ok(());
        }
        self.with(|connection| {
            let tx = connection.transaction().map_err(map_err)?;
            {
                let mut statement = tx
                    .prepare_cached(
                        "INSERT INTO folder_aggregates (scan_id, path, parent_path, name, \
                         logical_size, allocated_size, file_count, folder_count, modified) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                         ON CONFLICT (scan_id, path) DO UPDATE SET \
                           logical_size = excluded.logical_size, \
                           allocated_size = excluded.allocated_size, \
                           file_count = excluded.file_count, \
                           folder_count = excluded.folder_count",
                    )
                    .map_err(map_err)?;

                for aggregate in aggregates {
                    statement
                        .execute(params![
                            scan_id,
                            aggregate.path,
                            aggregate.parent_path,
                            aggregate.name,
                            aggregate.logical_size as i64,
                            aggregate.allocated_size as i64,
                            aggregate.file_count as i64,
                            aggregate.folder_count as i64,
                            aggregate.modified,
                        ])
                        .map_err(map_err)?;
                }
            }
            tx.commit().map_err(map_err)
        })
    }

    pub fn record_scan_error(&self, scan_id: i64, path: &str, error: &StoraError) -> Result<()> {
        self.with(|connection| {
            connection
                .execute(
                    "INSERT INTO scan_errors (scan_id, path, code, message) VALUES (?1, ?2, ?3, ?4)",
                    params![scan_id, path, error.code(), error.to_string()],
                )
                .map_err(map_err)?;
            Ok(())
        })
    }

    pub fn latest_scan(&self, root: &str) -> Result<Option<ScanSummary>> {
        self.with(|connection| {
            connection
                .query_row(
                    "SELECT id, root, started_at, finished_at, duration_ms, files_scanned, \
                     folders_scanned, bytes_analyzed, error_count, state \
                     FROM scans WHERE root = ?1 AND state = 'completed' \
                     ORDER BY started_at DESC LIMIT 1",
                    params![root],
                    |row| {
                        Ok(ScanSummary {
                            scan_id: row.get(0)?,
                            root: row.get(1)?,
                            started_at: row.get(2)?,
                            finished_at: row.get(3)?,
                            duration_ms: row.get::<_, i64>(4)? as u64,
                            files_scanned: row.get::<_, i64>(5)? as u64,
                            folders_scanned: row.get::<_, i64>(6)? as u64,
                            bytes_analyzed: row.get::<_, i64>(7)? as u64,
                            errors: row.get::<_, i64>(8)? as u64,
                            state: parse_scan_state(&row.get::<_, String>(9)?),
                        })
                    },
                )
                .optional()
                .map_err(map_err)
        })
    }

    /// Marks scans left in an in-progress state as failed.
    ///
    /// Run at startup: a scan row stays `scanning` if the process was killed
    /// mid-walk, and without this it would linger forever and its partial rows
    /// would never be pruned.
    pub fn fail_interrupted_scans(&self) -> Result<usize> {
        self.with(|connection| {
            let affected = connection
                .execute(
                    "UPDATE scans SET state = 'failed' \
                     WHERE state IN ('scanning', 'preparing', 'paused', 'cancelling')",
                    [],
                )
                .map_err(map_err)?;
            Ok(affected)
        })
    }

    /// Deletes scans older than the newest `keep` completed scans for a root.
    ///
    /// Cascades remove the associated entries and aggregates.
    pub fn prune_scans(&self, root: &str, keep: usize) -> Result<usize> {
        self.with(|connection| {
            let removed = connection
                .execute(
                    "DELETE FROM scans WHERE root = ?1 AND id NOT IN (\
                       SELECT id FROM scans WHERE root = ?1 ORDER BY started_at DESC LIMIT ?2)",
                    params![root, keep as i64],
                )
                .map_err(map_err)?;
            Ok(removed)
        })
    }

    // -------------------------------------------------------------- browsing

    pub fn folder_children(&self, scan_id: i64, parent_path: &str) -> Result<Vec<FolderAggregate>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT a.path, a.parent_path, a.name, a.logical_size, a.allocated_size, \
                     a.file_count, a.folder_count, a.modified, \
                     EXISTS (SELECT 1 FROM folder_aggregates c \
                             WHERE c.scan_id = a.scan_id AND c.parent_path = a.path) \
                     FROM folder_aggregates a \
                     WHERE a.scan_id = ?1 AND a.parent_path = ?2 \
                     ORDER BY a.allocated_size DESC",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map(params![scan_id, parent_path], |row| {
                    Ok(FolderAggregate {
                        path: row.get(0)?,
                        parent_path: row.get(1)?,
                        name: row.get(2)?,
                        logical_size: row.get::<_, i64>(3)? as u64,
                        allocated_size: row.get::<_, i64>(4)? as u64,
                        file_count: row.get::<_, i64>(5)? as u64,
                        folder_count: row.get::<_, i64>(6)? as u64,
                        modified: row.get(7)?,
                        has_children: row.get::<_, i32>(8)? != 0,
                    })
                })
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    pub fn folder_aggregate(&self, scan_id: i64, path: &str) -> Result<Option<FolderAggregate>> {
        self.with(|connection| {
            connection
                .query_row(
                    "SELECT path, parent_path, name, logical_size, allocated_size, file_count, \
                     folder_count, modified FROM folder_aggregates \
                     WHERE scan_id = ?1 AND path = ?2",
                    params![scan_id, path],
                    |row| {
                        Ok(FolderAggregate {
                            path: row.get(0)?,
                            parent_path: row.get(1)?,
                            name: row.get(2)?,
                            logical_size: row.get::<_, i64>(3)? as u64,
                            allocated_size: row.get::<_, i64>(4)? as u64,
                            file_count: row.get::<_, i64>(5)? as u64,
                            folder_count: row.get::<_, i64>(6)? as u64,
                            modified: row.get(7)?,
                            has_children: false,
                        })
                    },
                )
                .optional()
                .map_err(map_err)
        })
    }

    pub fn large_files(
        &self,
        scan_id: i64,
        minimum_bytes: u64,
        limit: usize,
    ) -> Result<Vec<LargeFile>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT path, name, extension, logical_size, allocated_size, created, \
                     modified, accessed FROM scan_entries \
                     WHERE scan_id = ?1 AND is_directory = 0 AND logical_size >= ?2 \
                     ORDER BY logical_size DESC LIMIT ?3",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map(
                    params![scan_id, minimum_bytes as i64, limit as i64],
                    |row| {
                        Ok(LargeFile {
                            path: row.get(0)?,
                            name: row.get(1)?,
                            extension: row.get(2)?,
                            logical_size: row.get::<_, i64>(3)? as u64,
                            allocated_size: row.get::<_, i64>(4)? as u64,
                            created: row.get(5)?,
                            modified: row.get(6)?,
                            accessed: row.get(7)?,
                        })
                    },
                )
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    /// Records the per-category totals accumulated during a scan.
    pub fn insert_category_totals(
        &self,
        scan_id: i64,
        totals: &[(StorageCategory, u64, u64)],
    ) -> Result<()> {
        if totals.is_empty() {
            return Ok(());
        }
        self.with(|connection| {
            let tx = connection.transaction().map_err(map_err)?;
            {
                let mut statement = tx
                    .prepare_cached(
                        "INSERT INTO scan_categories (scan_id, category, bytes, file_count) \
                         VALUES (?1, ?2, ?3, ?4) \
                         ON CONFLICT (scan_id, category) DO UPDATE SET \
                           bytes = excluded.bytes, file_count = excluded.file_count",
                    )
                    .map_err(map_err)?;

                for (category, bytes, count) in totals {
                    statement
                        .execute(params![
                            scan_id,
                            category.as_str(),
                            *bytes as i64,
                            *count as i64
                        ])
                        .map_err(map_err)?;
                }
            }
            tx.commit().map_err(map_err)
        })
    }

    pub fn category_breakdown(&self, scan_id: i64) -> Result<Vec<CategoryBreakdown>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT category, bytes, file_count FROM scan_categories WHERE scan_id = ?1",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map(params![scan_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)? as u64,
                        row.get::<_, i64>(2)? as u64,
                    ))
                })
                .map_err(map_err)?;

            let mut totals: HashMap<StorageCategory, (u64, u64)> = HashMap::new();
            for row in rows {
                let (name, bytes, count) = row.map_err(map_err)?;
                let entry = totals.entry(StorageCategory::parse(&name)).or_default();
                entry.0 += bytes;
                entry.1 += count;
            }

            // Always return every category so the UI layout stays stable, but
            // in descending size order.
            let mut breakdown: Vec<CategoryBreakdown> = StorageCategory::all()
                .into_iter()
                .map(|category| {
                    let (bytes, file_count) = totals.get(&category).copied().unwrap_or((0, 0));
                    CategoryBreakdown {
                        category,
                        bytes,
                        file_count,
                    }
                })
                .collect();
            breakdown.sort_by(|a, b| b.bytes.cmp(&a.bytes));
            Ok(breakdown)
        })
    }

    // ------------------------------------------------------------ exclusions

    pub fn add_exclusion(
        &self,
        pattern: &str,
        kind: ExclusionKind,
        reason: ExclusionReason,
        now: i64,
    ) -> Result<i64> {
        self.with(|connection| {
            connection
                .execute(
                    "INSERT INTO exclusions (pattern, kind, reason, created_at) \
                     VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT (pattern, kind) DO UPDATE SET reason = excluded.reason",
                    params![pattern, kind.as_str(), reason.as_str(), now],
                )
                .map_err(map_err)?;
            Ok(connection.last_insert_rowid())
        })
    }

    pub fn remove_exclusion(&self, id: i64) -> Result<()> {
        self.with(|connection| {
            connection
                .execute("DELETE FROM exclusions WHERE id = ?1", params![id])
                .map_err(map_err)?;
            Ok(())
        })
    }

    pub fn exclusions(&self) -> Result<Vec<Exclusion>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT id, pattern, kind, reason, created_at FROM exclusions \
                     ORDER BY created_at DESC",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map([], |row| {
                    Ok(Exclusion {
                        id: row.get(0)?,
                        pattern: row.get(1)?,
                        kind: ExclusionKind::parse(&row.get::<_, String>(2)?),
                        reason: ExclusionReason::parse(&row.get::<_, String>(3)?),
                        created_at: row.get(4)?,
                    })
                })
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    // --------------------------------------------------------------- cleanup

    pub fn begin_cleanup(
        &self,
        plan_id: &str,
        started_at: i64,
        method: DeletionMethod,
        files_selected: u64,
        categories: &[String],
    ) -> Result<i64> {
        let categories_json = serde_json::to_string(categories)
            .map_err(|err| StoraError::Internal(err.to_string()))?;
        self.with(|connection| {
            connection
                .execute(
                    "INSERT INTO cleanup_operations (plan_id, started_at, method, files_selected, \
                     categories, state) VALUES (?1, ?2, ?3, ?4, ?5, 'cleaning')",
                    params![
                        plan_id,
                        started_at,
                        method.as_str(),
                        files_selected as i64,
                        categories_json
                    ],
                )
                .map_err(map_err)?;
            Ok(connection.last_insert_rowid())
        })
    }

    pub fn record_cleanup_items(
        &self,
        operation_id: i64,
        removed: &[CleanupItem],
        failed: &[(CleanupItem, CleanupItemError)],
    ) -> Result<()> {
        self.with(|connection| {
            let tx = connection.transaction().map_err(map_err)?;
            {
                let mut statement = tx
                    .prepare_cached(
                        "INSERT INTO cleanup_items (operation_id, path, category_id, size, \
                         removed, error_code, error_message) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    )
                    .map_err(map_err)?;

                for item in removed {
                    statement
                        .execute(params![
                            operation_id,
                            item.path,
                            item.category_id,
                            item.size as i64,
                            1,
                            Option::<String>::None,
                            Option::<String>::None,
                        ])
                        .map_err(map_err)?;
                }
                for (item, error) in failed {
                    statement
                        .execute(params![
                            operation_id,
                            item.path,
                            item.category_id,
                            item.size as i64,
                            0,
                            Some(error.code.clone()),
                            Some(error.message.clone()),
                        ])
                        .map_err(map_err)?;
                }
            }
            tx.commit().map_err(map_err)
        })
    }

    pub fn finish_cleanup(&self, result: &CleanupResult) -> Result<()> {
        self.with(|connection| {
            connection
                .execute(
                    "UPDATE cleanup_operations SET duration_ms = ?2, files_removed = ?3, \
                     files_skipped = ?4, recovered_bytes = ?5, error_count = ?6, state = ?7 \
                     WHERE id = ?1",
                    params![
                        result.operation_id,
                        result.duration_ms as i64,
                        result.files_removed as i64,
                        result.files_skipped as i64,
                        result.recovered_bytes as i64,
                        result.errors.len() as i64,
                        cleanup_state_str(result.state),
                    ],
                )
                .map_err(map_err)?;
            Ok(())
        })
    }

    pub fn cleanup_history(&self, limit: usize) -> Result<Vec<CleanupHistoryEntry>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT id, started_at, duration_ms, categories, files_selected, \
                     files_removed, files_skipped, recovered_bytes, method, error_count, \
                     automation_rule FROM cleanup_operations \
                     ORDER BY started_at DESC LIMIT ?1",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map(params![limit as i64], |row| {
                    let categories: String = row.get(3)?;
                    Ok(CleanupHistoryEntry {
                        operation_id: row.get(0)?,
                        started_at: row.get(1)?,
                        duration_ms: row.get::<_, i64>(2)? as u64,
                        categories: serde_json::from_str(&categories).unwrap_or_default(),
                        files_selected: row.get::<_, i64>(4)? as u64,
                        files_removed: row.get::<_, i64>(5)? as u64,
                        files_skipped: row.get::<_, i64>(6)? as u64,
                        recovered_bytes: row.get::<_, i64>(7)? as u64,
                        method: DeletionMethod::parse(&row.get::<_, String>(8)?),
                        error_count: row.get::<_, i64>(9)? as u64,
                        automation_rule: row.get(10)?,
                    })
                })
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    pub fn cleanup_errors(&self, operation_id: i64) -> Result<Vec<CleanupItemError>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT path, error_code, error_message FROM cleanup_items \
                     WHERE operation_id = ?1 AND removed = 0 AND error_code IS NOT NULL",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map(params![operation_id], |row| {
                    Ok(CleanupItemError {
                        path: row.get(0)?,
                        code: row.get(1)?,
                        message: row.get(2)?,
                    })
                })
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    /// Total bytes recovered by completed cleanups since `since`.
    pub fn recovered_since(&self, since: i64) -> Result<u64> {
        self.with(|connection| {
            let total: i64 = connection
                .query_row(
                    "SELECT COALESCE(SUM(recovered_bytes), 0) FROM cleanup_operations \
                     WHERE started_at >= ?1",
                    params![since],
                    |row| row.get(0),
                )
                .map_err(map_err)?;
            Ok(total as u64)
        })
    }

    pub fn delete_history_before(&self, cutoff: i64) -> Result<usize> {
        self.with(|connection| {
            let removed = connection
                .execute(
                    "DELETE FROM cleanup_operations WHERE started_at < ?1",
                    params![cutoff],
                )
                .map_err(map_err)?;
            Ok(removed)
        })
    }

    // -------------------------------------------------------------- settings

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with(|connection| {
            connection
                .execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2) \
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )
                .map_err(map_err)?;
            Ok(())
        })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with(|connection| {
            connection
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(map_err)
        })
    }

    /// Clears every table that holds observed data, leaving settings intact.
    pub fn clear_local_data(&self) -> Result<()> {
        self.with(|connection: &mut Connection| {
            connection
                .execute_batch(
                    "DELETE FROM scan_entries; \
                     DELETE FROM folder_aggregates; \
                     DELETE FROM scan_errors; \
                     DELETE FROM scans; \
                     DELETE FROM cleanup_items; \
                     DELETE FROM cleanup_operations; \
                     DELETE FROM quarantine_items;",
                )
                .map_err(map_err)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stora_core::model::DriveType;

    fn index() -> Index {
        Index::open_in_memory().unwrap()
    }

    fn entry(path: &str, parent: &str, size: u64, is_dir: bool) -> FileEntry {
        FileEntry {
            path: path.into(),
            parent_path: parent.into(),
            name: stora_security::file_name_of(path),
            extension: stora_security::extension_of(path),
            logical_size: size,
            allocated_size: size,
            created: Some(100),
            modified: Some(200),
            accessed: Some(300),
            attributes: 0,
            is_directory: is_dir,
            is_reparse_point: false,
        }
    }

    #[test]
    fn scan_lifecycle_records_a_summary() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 1_000).unwrap();

        assert!(
            index.latest_scan("C:\\").unwrap().is_none(),
            "an in-progress scan must not be reported as the latest completed scan"
        );

        index
            .finish_scan(scan_id, 1_050, 50_000, 12, 3, 4096, 1, ScanState::Completed)
            .unwrap();

        let summary = index.latest_scan("C:\\").unwrap().expect("summary");
        assert_eq!(summary.files_scanned, 12);
        assert_eq!(summary.bytes_analyzed, 4096);
        assert_eq!(summary.errors, 1);
        assert_eq!(summary.state, ScanState::Completed);
    }

    #[test]
    fn batched_entry_writes_round_trip() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();

        let batch: Vec<_> = (0..500)
            .map(|i| {
                (
                    entry(
                        &format!("C:\\Data\\file{i}.bin"),
                        "C:\\Data",
                        (i as u64 + 1) * 1024,
                        false,
                    ),
                    StorageCategory::Documents,
                )
            })
            .collect();

        index.insert_entries(scan_id, &batch).unwrap();

        let largest = index.large_files(scan_id, 0, 5).unwrap();
        assert_eq!(largest.len(), 5);
        assert_eq!(largest[0].logical_size, 500 * 1024);
        assert!(
            largest[0].logical_size >= largest[1].logical_size,
            "results must be ordered by size"
        );
    }

    #[test]
    fn empty_batches_are_a_no_op() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();
        index.insert_entries(scan_id, &[]).unwrap();
        index.insert_aggregates(scan_id, &[]).unwrap();
        assert!(index.large_files(scan_id, 0, 10).unwrap().is_empty());
    }

    #[test]
    fn large_file_threshold_is_respected() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();
        index
            .insert_entries(
                scan_id,
                &[
                    (
                        entry("C:\\a.bin", "C:\\", 100, false),
                        StorageCategory::Other,
                    ),
                    (
                        entry("C:\\b.bin", "C:\\", 10_000, false),
                        StorageCategory::Other,
                    ),
                ],
            )
            .unwrap();

        let results = index.large_files(scan_id, 1_000, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "b.bin");
    }

    #[test]
    fn directories_are_excluded_from_large_files() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();
        index
            .insert_entries(
                scan_id,
                &[(
                    entry("C:\\Data", "C:\\", 99_999, true),
                    StorageCategory::Other,
                )],
            )
            .unwrap();
        assert!(index.large_files(scan_id, 0, 10).unwrap().is_empty());
    }

    #[test]
    fn folder_children_report_child_presence() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();

        let aggregates = vec![
            FolderAggregate {
                path: "C:\\Data".into(),
                parent_path: Some("C:\\".into()),
                name: "Data".into(),
                logical_size: 500,
                allocated_size: 512,
                file_count: 2,
                folder_count: 1,
                modified: None,
                has_children: false,
            },
            FolderAggregate {
                path: "C:\\Data\\Inner".into(),
                parent_path: Some("C:\\Data".into()),
                name: "Inner".into(),
                logical_size: 100,
                allocated_size: 128,
                file_count: 1,
                folder_count: 0,
                modified: None,
                has_children: false,
            },
        ];
        index.insert_aggregates(scan_id, &aggregates).unwrap();

        let children = index.folder_children(scan_id, "C:\\").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "Data");
        assert!(children[0].has_children, "C:\\Data contains Inner");

        let grandchildren = index.folder_children(scan_id, "C:\\Data").unwrap();
        assert_eq!(grandchildren.len(), 1);
        assert!(!grandchildren[0].has_children);
    }

    #[test]
    fn aggregates_upsert_rather_than_duplicate() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();
        let mut aggregate = FolderAggregate {
            path: "C:\\Data".into(),
            parent_path: Some("C:\\".into()),
            name: "Data".into(),
            logical_size: 100,
            allocated_size: 100,
            file_count: 1,
            folder_count: 0,
            modified: None,
            has_children: false,
        };
        index
            .insert_aggregates(scan_id, &[aggregate.clone()])
            .unwrap();

        aggregate.logical_size = 900;
        aggregate.allocated_size = 900;
        index.insert_aggregates(scan_id, &[aggregate]).unwrap();

        let children = index.folder_children(scan_id, "C:\\").unwrap();
        assert_eq!(children.len(), 1, "the row must be updated, not duplicated");
        assert_eq!(children[0].logical_size, 900);
    }

    #[test]
    fn category_breakdown_returns_every_category_sorted_by_size() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();
        index
            .insert_category_totals(
                scan_id,
                &[
                    (StorageCategory::Applications, 1_000, 1),
                    (StorageCategory::Development, 5_000, 1),
                ],
            )
            .unwrap();

        let breakdown = index.category_breakdown(scan_id).unwrap();
        assert_eq!(breakdown.len(), StorageCategory::all().len());
        assert_eq!(breakdown[0].category, StorageCategory::Development);
        assert_eq!(breakdown[0].bytes, 5_000);
        assert_eq!(breakdown[1].category, StorageCategory::Applications);
    }

    #[test]
    fn interrupted_scans_are_closed_out_on_startup() {
        let index = index();
        // Simulates the process being killed mid-walk.
        let stranded = index.begin_scan("C:\\", 100).unwrap();

        let finished = index.begin_scan("D:\\", 200).unwrap();
        index
            .finish_scan(finished, 250, 1, 1, 1, 1, 0, ScanState::Completed)
            .unwrap();

        let closed = index.fail_interrupted_scans().unwrap();
        assert_eq!(closed, 1, "only the stranded scan is affected");

        let state: String = index
            .with(|c| {
                c.query_row(
                    "SELECT state FROM scans WHERE id = ?1",
                    params![stranded],
                    |row| row.get(0),
                )
                .map_err(map_err)
            })
            .unwrap();
        assert_eq!(state, "failed");

        // The completed scan must still be reported as the latest.
        assert!(index.latest_scan("D:\\").unwrap().is_some());
    }

    #[test]
    fn category_totals_upsert_rather_than_accumulate() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();

        index
            .insert_category_totals(scan_id, &[(StorageCategory::Games, 100, 1)])
            .unwrap();
        index
            .insert_category_totals(scan_id, &[(StorageCategory::Games, 900, 4)])
            .unwrap();

        let breakdown = index.category_breakdown(scan_id).unwrap();
        let games = breakdown
            .iter()
            .find(|row| row.category == StorageCategory::Games)
            .unwrap();
        assert_eq!(games.bytes, 900, "a rewritten total replaces the old one");
        assert_eq!(games.file_count, 4);
    }

    #[test]
    fn exclusions_round_trip_and_deduplicate() {
        let index = index();
        index
            .add_exclusion(
                "C:\\Vault",
                ExclusionKind::Folder,
                ExclusionReason::UserExclusion,
                1,
            )
            .unwrap();
        // The same pattern and kind must update in place.
        index
            .add_exclusion(
                "C:\\Vault",
                ExclusionKind::Folder,
                ExclusionReason::SystemComponent,
                2,
            )
            .unwrap();

        let all = index.exclusions().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].reason, ExclusionReason::SystemComponent);

        index.remove_exclusion(all[0].id).unwrap();
        assert!(index.exclusions().unwrap().is_empty());
    }

    #[test]
    fn cleanup_history_only_counts_removed_bytes() {
        let index = index();
        let operation_id = index
            .begin_cleanup(
                "plan-1",
                1_000,
                DeletionMethod::RecycleBin,
                3,
                &["userTemp".into()],
            )
            .unwrap();

        let removed = vec![CleanupItem {
            path: "C:\\Temp\\a.tmp".into(),
            category_id: "userTemp".into(),
            size: 400,
            is_directory: false,
            modified: None,
        }];
        let failed = vec![(
            CleanupItem {
                path: "C:\\Temp\\b.tmp".into(),
                category_id: "userTemp".into(),
                size: 600,
                is_directory: false,
                modified: None,
            },
            CleanupItemError {
                path: "C:\\Temp\\b.tmp".into(),
                code: "FileLocked".into(),
                message: "in use".into(),
            },
        )];

        index
            .record_cleanup_items(operation_id, &removed, &failed)
            .unwrap();
        index
            .finish_cleanup(&CleanupResult {
                operation_id,
                state: CleanupState::CompletedWithErrors,
                recovered_bytes: 400,
                files_removed: 1,
                files_skipped: 1,
                duration_ms: 10,
                method: DeletionMethod::RecycleBin,
                errors: vec![],
            })
            .unwrap();

        let history = index.cleanup_history(10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(
            history[0].recovered_bytes, 400,
            "the skipped file's bytes must not be claimed as recovered"
        );
        assert_eq!(history[0].files_skipped, 1);
        assert_eq!(history[0].categories, vec!["userTemp".to_string()]);

        let errors = index.cleanup_errors(operation_id).unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "FileLocked");
    }

    #[test]
    fn recovered_since_sums_only_recent_operations() {
        let index = index();
        for (started, bytes) in [(100i64, 500u64), (900, 300)] {
            let id = index
                .begin_cleanup("p", started, DeletionMethod::RecycleBin, 1, &[])
                .unwrap();
            index
                .finish_cleanup(&CleanupResult {
                    operation_id: id,
                    state: CleanupState::Completed,
                    recovered_bytes: bytes,
                    files_removed: 1,
                    files_skipped: 0,
                    duration_ms: 1,
                    method: DeletionMethod::RecycleBin,
                    errors: vec![],
                })
                .unwrap();
        }
        assert_eq!(index.recovered_since(0).unwrap(), 800);
        assert_eq!(index.recovered_since(500).unwrap(), 300);
    }

    #[test]
    fn settings_round_trip() {
        let index = index();
        assert!(index.get_setting("theme").unwrap().is_none());
        index.set_setting("theme", "dark").unwrap();
        index.set_setting("theme", "light").unwrap();
        assert_eq!(index.get_setting("theme").unwrap().unwrap(), "light");
    }

    #[test]
    fn deleting_a_scan_cascades_to_its_entries() {
        let index = index();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();
        index
            .insert_entries(
                scan_id,
                &[(
                    entry("C:\\a.bin", "C:\\", 10, false),
                    StorageCategory::Other,
                )],
            )
            .unwrap();
        index
            .finish_scan(scan_id, 1, 1, 1, 0, 10, 0, ScanState::Completed)
            .unwrap();

        index.prune_scans("C:\\", 0).unwrap();
        assert!(index.large_files(scan_id, 0, 10).unwrap().is_empty());
    }

    #[test]
    fn prune_keeps_the_requested_number_of_scans() {
        let index = index();
        for i in 0..5 {
            let id = index.begin_scan("C:\\", i * 100).unwrap();
            index
                .finish_scan(id, i * 100 + 1, 1, 0, 0, 0, 0, ScanState::Completed)
                .unwrap();
        }
        let removed = index.prune_scans("C:\\", 2).unwrap();
        assert_eq!(removed, 3);
    }

    #[test]
    fn clear_local_data_preserves_settings() {
        let index = index();
        index.set_setting("theme", "dark").unwrap();
        let scan_id = index.begin_scan("C:\\", 0).unwrap();
        index
            .insert_entries(
                scan_id,
                &[(
                    entry("C:\\a.bin", "C:\\", 10, false),
                    StorageCategory::Other,
                )],
            )
            .unwrap();

        index.clear_local_data().unwrap();

        assert!(index.large_files(scan_id, 0, 10).unwrap().is_empty());
        assert_eq!(index.get_setting("theme").unwrap().unwrap(), "dark");
    }

    #[test]
    fn drives_upsert_on_repeat_observation() {
        let index = index();
        let mut drive = DriveInfo {
            root: "C:\\".into(),
            label: "Local Disk".into(),
            filesystem: "NTFS".into(),
            total_bytes: 1_000,
            free_bytes: 400,
            drive_type: DriveType::Fixed,
            is_removable: false,
        };
        index.upsert_drives(&[drive.clone()], 1).unwrap();
        drive.free_bytes = 200;
        index.upsert_drives(&[drive], 2).unwrap();

        let count: i64 = index
            .with(|c| {
                c.query_row("SELECT count(*) FROM drives", [], |row| row.get(0))
                    .map_err(map_err)
            })
            .unwrap();
        assert_eq!(count, 1);
    }
}
