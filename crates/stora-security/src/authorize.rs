use std::collections::HashSet;

use stora_core::cleanup::{CleanupItem, CleanupPlan};
use stora_core::{Result, StoraError};

use crate::path::normalize;
use crate::protected::ensure_deletable;

/// Resolves a frontend selection into the concrete items that may be deleted.
///
/// The frontend sends *indices into a backend-generated plan*, never paths.
/// Even so, every resolved item is re-checked against the protected-path rules
/// before it is returned, so a bug in plan generation cannot escalate into
/// deleting a system file.
pub fn authorize_selection(
    plan: &CleanupPlan,
    selected_indices: &[usize],
    now: i64,
) -> Result<Vec<CleanupItem>> {
    if plan.is_expired(now) {
        return Err(StoraError::CleanupPlanExpired {
            plan_id: plan.plan_id.clone(),
        });
    }

    let mut seen = HashSet::new();
    let mut authorized = Vec::with_capacity(selected_indices.len());

    for &index in selected_indices {
        let item = plan
            .items
            .get(index)
            .ok_or_else(|| StoraError::PathNotAuthorized {
                path: format!("item #{index}"),
            })?;

        // Duplicate indices would double-count recovered bytes.
        if !seen.insert(index) {
            continue;
        }

        let normalized = normalize(&item.path)?;
        if normalized != item.path {
            return Err(StoraError::PathNotAuthorized {
                path: item.path.clone(),
            });
        }
        ensure_deletable(&normalized)?;

        authorized.push(item.clone());
    }

    Ok(authorized)
}

/// Confirms an authorized item still matches what the user previewed.
///
/// Run immediately before deletion. Re-reading metadata here is what closes
/// the window between preview and execution: if a file grew, shrank, or was
/// replaced by a link, we skip it instead of deleting something unexpected.
pub fn revalidate(item: &CleanupItem) -> Result<()> {
    let extended = crate::path::to_extended_length(&item.path);
    let metadata = std::fs::symlink_metadata(&extended)
        .map_err(|err| StoraError::from_io(&err, &item.path))?;

    if metadata.is_dir() != item.is_directory {
        return Err(StoraError::PathChangedAfterPreview {
            path: item.path.clone(),
        });
    }

    // A path that became a link since the preview is a redirection attempt.
    if metadata.file_type().is_symlink() {
        return Err(StoraError::PathChangedAfterPreview {
            path: item.path.clone(),
        });
    }

    if !item.is_directory && metadata.len() != item.size {
        return Err(StoraError::PathChangedAfterPreview {
            path: item.path.clone(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use stora_core::cleanup::{CleanupPlan, DeletionMethod};

    fn item(path: &str) -> CleanupItem {
        CleanupItem {
            path: path.into(),
            category_id: "userTemp".into(),
            size: 10,
            is_directory: false,
            modified: None,
        }
    }

    fn plan(items: Vec<CleanupItem>, expires_at: i64) -> CleanupPlan {
        CleanupPlan {
            plan_id: "plan-1".into(),
            created_at: 0,
            expires_at,
            categories: vec![],
            total_bytes: items.iter().map(|i| i.size).sum(),
            file_count: items.len() as u64,
            folder_count: 0,
            items,
        }
    }

    #[test]
    fn authorizes_valid_selection() {
        let plan = plan(
            vec![
                item("C:\\Users\\Test\\AppData\\Local\\Temp\\a.tmp"),
                item("C:\\Users\\Test\\AppData\\Local\\Temp\\b.tmp"),
            ],
            1_000,
        );
        let approved = authorize_selection(&plan, &[0, 1], 10).unwrap();
        assert_eq!(approved.len(), 2);
    }

    #[test]
    fn rejects_indices_outside_the_plan() {
        let plan = plan(
            vec![item("C:\\Users\\Test\\AppData\\Local\\Temp\\a.tmp")],
            1_000,
        );
        let err = authorize_selection(&plan, &[0, 5], 10).unwrap_err();
        assert_eq!(err.code(), "PathNotAuthorized");
    }

    #[test]
    fn rejects_expired_plans() {
        let plan = plan(
            vec![item("C:\\Users\\Test\\AppData\\Local\\Temp\\a.tmp")],
            100,
        );
        let err = authorize_selection(&plan, &[0], 100).unwrap_err();
        assert_eq!(err.code(), "CleanupPlanExpired");
    }

    #[test]
    fn rejects_protected_items_even_inside_a_plan() {
        // Simulates a plan-generation bug: the guard must still hold.
        let plan = plan(vec![item("C:\\Windows\\System32\\kernel32.dll")], 1_000);
        let err = authorize_selection(&plan, &[0], 10).unwrap_err();
        assert_eq!(err.code(), "ProtectedPath");
    }

    #[test]
    fn rejects_denormalized_paths_in_a_plan() {
        let plan = plan(vec![item("C:\\Users\\Test\\..\\Windows\\file.dll")], 1_000);
        assert!(authorize_selection(&plan, &[0], 10).is_err());
    }

    #[test]
    fn deduplicates_repeated_indices() {
        let plan = plan(
            vec![item("C:\\Users\\Test\\AppData\\Local\\Temp\\a.tmp")],
            1_000,
        );
        let approved = authorize_selection(&plan, &[0, 0, 0], 10).unwrap();
        assert_eq!(
            approved.len(),
            1,
            "recovered bytes must not be double counted"
        );
    }

    #[test]
    fn empty_selection_authorizes_nothing() {
        let plan = plan(
            vec![item("C:\\Users\\Test\\AppData\\Local\\Temp\\a.tmp")],
            1_000,
        );
        assert!(authorize_selection(&plan, &[], 10).unwrap().is_empty());
    }

    #[test]
    fn revalidate_detects_a_size_change_after_preview() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cache.bin");
        std::fs::write(&file, b"0123456789").unwrap();

        let mut previewed = item(&file.to_string_lossy());
        previewed.size = 10;
        assert!(revalidate(&previewed).is_ok());

        std::fs::write(&file, b"much longer content than before").unwrap();
        let err = revalidate(&previewed).unwrap_err();
        assert_eq!(err.code(), "PathChangedAfterPreview");
    }

    #[test]
    fn revalidate_detects_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gone.bin");
        let previewed = item(&file.to_string_lossy());
        let err = revalidate(&previewed).unwrap_err();
        assert_eq!(err.code(), "PathNotFound");
    }

    #[test]
    fn revalidate_detects_a_type_change() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("entry");
        std::fs::create_dir(&target).unwrap();

        let previewed = item(&target.to_string_lossy()); // recorded as a file
        let err = revalidate(&previewed).unwrap_err();
        assert_eq!(err.code(), "PathChangedAfterPreview");
    }

    #[test]
    fn deletion_method_round_trips() {
        for method in [
            DeletionMethod::RecycleBin,
            DeletionMethod::Permanent,
            DeletionMethod::Quarantine,
        ] {
            assert_eq!(DeletionMethod::parse(method.as_str()), method);
        }
    }
}
