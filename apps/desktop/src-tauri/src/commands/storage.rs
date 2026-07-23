use serde::Serialize;
use tauri::State;

use stora_core::model::{CategoryBreakdown, FolderAggregate, LargeFile};
use stora_core::{Result, StoraError};

use crate::state::AppState;

/// A deliberately small, conservative set of user-owned folders that may be
/// considered for an assisted move. Installed apps, games, AppData and the
/// Desktop are intentionally absent: those need their owning application or
/// Windows' Known Folder tooling, not a generic file move.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocationCandidate {
    pub name: String,
    pub path: String,
    pub allocated_size: u64,
    pub file_count: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocationCheck {
    pub label: String,
    pub passed: bool,
    pub detail: String,
}

/// A reviewed proposal only. It is intentionally not an authorization to
/// mutate the filesystem; execution must revalidate every check after an
/// explicit user confirmation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocationPlan {
    pub source: String,
    pub destination: String,
    pub estimated_bytes: u64,
    pub file_count: u64,
    pub can_proceed: bool,
    pub checks: Vec<RelocationCheck>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocationResult {
    pub source: String,
    pub destination: String,
    pub bytes_moved: u64,
    pub files_moved: u64,
}

/// Children of a folder, ordered by size. The tree loads level by level so a
/// multi-million-file drive never renders at once.
#[tauri::command]
pub fn get_folder_children(
    state: State<'_, AppState>,
    root: String,
    parent_path: String,
) -> Result<Vec<FolderAggregate>> {
    let root = stora_security::normalize(&root)?;
    let parent_path = stora_security::normalize(&parent_path)?;
    let scan_id = state.resolve_scan(&root)?;
    state.index.folder_children(scan_id, &parent_path)
}

#[tauri::command]
pub fn get_folder_details(
    state: State<'_, AppState>,
    root: String,
    path: String,
) -> Result<Option<FolderAggregate>> {
    let root = stora_security::normalize(&root)?;
    let path = stora_security::normalize(&path)?;
    let scan_id = state.resolve_scan(&root)?;
    state.index.folder_aggregate(scan_id, &path)
}

#[tauri::command]
pub fn get_large_files(
    state: State<'_, AppState>,
    root: String,
    minimum_bytes: u64,
    limit: usize,
) -> Result<Vec<LargeFile>> {
    let root = stora_security::normalize(&root)?;
    let scan_id = state.resolve_scan(&root)?;
    // Bound the result set so a query can never stall the UI thread.
    state
        .index
        .large_files(scan_id, minimum_bytes, limit.min(5_000))
}

#[tauri::command]
pub fn get_storage_breakdown(
    state: State<'_, AppState>,
    root: String,
) -> Result<Vec<CategoryBreakdown>> {
    let root = stora_security::normalize(&root)?;
    let scan_id = state.resolve_scan(&root)?;
    state.index.category_breakdown(scan_id)
}

/// Returns large personal folders from the latest completed scan. This is a
/// preview only: it performs no copying, moving, linking or configuration
/// changes. A completed scan is required so estimates are instant and do not
/// trigger another expensive filesystem traversal.
#[tauri::command]
pub fn get_relocation_candidates(
    state: State<'_, AppState>,
    root: String,
) -> Result<Vec<RelocationCandidate>> {
    let root = stora_security::normalize(&root)?;
    let scan_id = state.resolve_scan(&root)?;
    let profile = std::env::var("USERPROFILE").map_err(|_| {
        stora_core::StoraError::Internal("could not resolve the current Windows user profile".into())
    })?;

    let candidates = [
        ("Downloads", "Downloads", "Downloaded files you choose to keep."),
        ("Documents", "Documents", "Personal documents and project files."),
        ("Pictures", "Pictures", "Personal photos and image libraries."),
        ("Videos", "Videos", "Personal videos and recordings."),
    ];

    let mut result = Vec::new();
    for (name, folder, reason) in candidates {
        let path = stora_security::normalize(&format!("{profile}\\{folder}"))?;
        // A selected drive only receives suggestions that actually live on it.
        if !path
            .get(..root.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&root))
        {
            continue;
        }

        if let Some(aggregate) = state.index.folder_aggregate(scan_id, &path)? {
            if aggregate.allocated_size > 0 {
                result.push(RelocationCandidate {
                    name: name.into(),
                    path,
                    allocated_size: aggregate.allocated_size,
                    file_count: aggregate.file_count,
                    reason: reason.into(),
                });
            }
        }
    }

    result.sort_by(|a, b| b.allocated_size.cmp(&a.allocated_size));
    Ok(result)
}

/// Builds an inspection-only move proposal for one curated personal folder.
/// The frontend cannot submit arbitrary locations: sources are checked again
/// here, and nothing is copied, removed or linked by this command.
#[tauri::command]
pub fn build_relocation_plan(
    state: State<'_, AppState>,
    source: String,
    destination_root: String,
) -> Result<RelocationPlan> {
    let source = stora_security::normalize(&source)?;
    let destination_root = stora_security::normalize(&destination_root)?;
    let profile = std::env::var("USERPROFILE").map_err(|_| {
        stora_core::StoraError::Internal("could not resolve the current Windows user profile".into())
    })?;
    let allowed = ["Downloads", "Documents", "Pictures", "Videos"]
        .into_iter()
        .map(|folder| stora_security::normalize(&format!("{profile}\\{folder}")))
        .collect::<Result<Vec<_>>>()?;
    if !allowed.iter().any(|path| path.eq_ignore_ascii_case(&source))
        || stora_security::is_protected(&source)
        || stora_security::is_sensitive(&source)
    {
        return Err(StoraError::ProtectedPath { path: source });
    }

    let source_root = format!("{}\\", &source[..2]);
    let scan_id = state.resolve_scan(&source_root)?;
    let aggregate = state
        .index
        .folder_aggregate(scan_id, &source)?
        .ok_or_else(|| StoraError::PathChangedAfterPreview {
            path: source.clone(),
        })?;

    let destination_drive = stora_winapi::drive_for_path(&destination_root)?;
    let folder_name = stora_security::file_name_of(&source);
    let destination = stora_security::normalize(&format!(
        "{}Stora Moved\\{folder_name}",
        destination_drive.root
    ))?;

    let same_drive = destination_drive.root.eq_ignore_ascii_case(&source_root);
    let reserve = 1024 * 1024 * 1024u64;
    let required = aggregate.allocated_size.saturating_add(reserve);
    let destination_extended = stora_security::to_extended_length(&destination);
    let destination_exists = std::path::Path::new(&destination_extended).exists();
    let source_is_directory = std::fs::symlink_metadata(stora_security::to_extended_length(&source))
        .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        .unwrap_or(false);

    let checks = vec![
        RelocationCheck {
            label: "Approved personal folder".into(),
            passed: true,
            detail: "Only Downloads, Documents, Pictures, or Videos can be planned here.".into(),
        },
        RelocationCheck {
            label: "Different destination drive".into(),
            passed: !same_drive,
            detail: if same_drive { "Choose a different drive.".into() } else { format!("{} is separate from the source drive.", destination_drive.root) },
        },
        RelocationCheck {
            label: "Destination free space".into(),
            passed: destination_drive.free_bytes >= required,
            detail: format!("Needs the estimated data plus a 1 GB safety reserve."),
        },
        RelocationCheck {
            label: "Destination folder is new".into(),
            passed: !destination_exists,
            detail: if destination_exists { "A folder already exists at the proposed destination; Stora will not merge or overwrite it.".into() } else { "No existing folder will be overwritten.".into() },
        },
        RelocationCheck {
            label: "Source is still a normal folder".into(),
            passed: source_is_directory,
            detail: "Links and changed paths require a new review.".into(),
        },
    ];

    Ok(RelocationPlan {
        source,
        destination,
        estimated_bytes: aggregate.allocated_size,
        file_count: aggregate.file_count,
        can_proceed: checks.iter().all(|check| check.passed),
        checks,
    })
}

/// Copies an approved personal folder, verifies the copy, then changes its
/// Windows Known Folder location. The source is removed only after Windows has
/// accepted the new location; any failure before that leaves the source alone.
#[tauri::command]
pub fn execute_relocation(
    state: State<'_, AppState>,
    source: String,
    destination_root: String,
) -> Result<RelocationResult> {
    let plan = build_relocation_plan(state, source, destination_root)?;
    if !plan.can_proceed {
        return Err(StoraError::PathChangedAfterPreview { path: plan.source });
    }

    let source_path = std::path::PathBuf::from(stora_security::to_extended_length(&plan.source));
    let destination_path = std::path::PathBuf::from(stora_security::to_extended_length(&plan.destination));
    let destination_parent = destination_path.parent().ok_or_else(|| StoraError::InvalidPath {
        reason: "destination has no parent folder".into(),
    })?;
    std::fs::create_dir_all(destination_parent)
        .map_err(|error| StoraError::from_io(&error, &plan.destination))?;

    copy_directory(&source_path, &destination_path, &plan.source)?;
    let source_totals = directory_totals(&source_path, &plan.source)?;
    let destination_totals = directory_totals(&destination_path, &plan.destination)?;
    if source_totals != destination_totals {
        // Preserve both copies for inspection rather than deleting anything
        // after a failed verification.
        return Err(StoraError::Internal(
            "copy verification failed; the original and copied folder were left intact".into(),
        ));
    }

    let folder_name = stora_security::file_name_of(&plan.source);
    stora_winapi::redirect_known_folder(&folder_name, &plan.destination)?;

    if let Err(error) = std::fs::remove_dir_all(&source_path) {
        // The data now exists twice. Restore the Windows location so the old
        // folder remains authoritative, and deliberately keep the verified
        // copy for the user to inspect instead of risking data loss.
        let _ = stora_winapi::redirect_known_folder(&folder_name, &plan.source);
        return Err(StoraError::from_io(&error, &plan.source));
    }

    Ok(RelocationResult {
        source: plan.source,
        destination: plan.destination,
        bytes_moved: source_totals.1,
        files_moved: source_totals.0,
    })
}

/// Copies only ordinary files and folders. Reparse points are rejected rather
/// than followed, so a personal-folder move can never unexpectedly include a
/// different volume or a linked sensitive location.
fn copy_directory(source: &std::path::Path, destination: &std::path::Path, display: &str) -> Result<()> {
    let metadata = std::fs::symlink_metadata(source).map_err(|error| StoraError::from_io(&error, display))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StoraError::PathChangedAfterPreview { path: display.into() });
    }
    std::fs::create_dir(destination).map_err(|error| StoraError::from_io(&error, &destination.display().to_string()))?;
    for entry in std::fs::read_dir(source).map_err(|error| StoraError::from_io(&error, display))? {
        let entry = entry.map_err(|error| StoraError::from_io(&error, display))?;
        let child_source = entry.path();
        let child_destination = destination.join(entry.file_name());
        let child_metadata = std::fs::symlink_metadata(&child_source)
            .map_err(|error| StoraError::from_io(&error, &child_source.display().to_string()))?;
        if child_metadata.file_type().is_symlink() {
            return Err(StoraError::PathChangedAfterPreview { path: child_source.display().to_string() });
        }
        if child_metadata.is_dir() {
            copy_directory(&child_source, &child_destination, &child_source.display().to_string())?;
        } else if child_metadata.is_file() {
            std::fs::copy(&child_source, &child_destination)
                .map_err(|error| StoraError::from_io(&error, &child_source.display().to_string()))?;
        }
    }
    Ok(())
}

/// Returns `(files, logical_bytes)` without following links.
fn directory_totals(path: &std::path::Path, display: &str) -> Result<(u64, u64)> {
    let mut totals = (0, 0);
    for entry in std::fs::read_dir(path).map_err(|error| StoraError::from_io(&error, display))? {
        let entry = entry.map_err(|error| StoraError::from_io(&error, display))?;
        let child = entry.path();
        let metadata = std::fs::symlink_metadata(&child)
            .map_err(|error| StoraError::from_io(&error, &child.display().to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(StoraError::PathChangedAfterPreview { path: child.display().to_string() });
        }
        if metadata.is_dir() {
            let nested = directory_totals(&child, &child.display().to_string())?;
            totals.0 += nested.0;
            totals.1 += nested.1;
        } else if metadata.is_file() {
            totals.0 += 1;
            totals.1 += metadata.len();
        }
    }
    Ok(totals)
}

/// Opens a path in File Explorer, selecting it when it is a file.
#[tauri::command]
pub fn reveal_in_explorer(path: String) -> Result<()> {
    let path = stora_security::normalize(&path)?;

    #[cfg(windows)]
    {
        use std::process::Command;
        // `/select,` requires the argument unquoted and adjacent; Command
        // handles the escaping, and the path is normalized above.
        let is_file = std::path::Path::new(&path).is_file();
        let mut command = Command::new("explorer.exe");
        if is_file {
            command.arg(format!("/select,{path}"));
        } else {
            command.arg(&path);
        }
        command.spawn().map_err(|err| {
            stora_core::StoraError::Internal(format!("could not open Explorer: {err}"))
        })?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        Err(stora_core::StoraError::UnsupportedFilesystem {
            filesystem: "File Explorer requires Windows".into(),
        })
    }
}
