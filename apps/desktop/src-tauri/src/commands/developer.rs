use serde::Serialize;
use tauri::State;

use stora_core::cleanup::{CleanupItem, CleanupPlan};
use stora_core::{Result, StoraError};
use stora_developer::{DetectedArtifact, DeveloperSummary, VirtualDisk};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeveloperScanResult {
    #[serde(flatten)]
    pub summary: DeveloperSummary,
    pub package_caches: Vec<DetectedArtifact>,
}

/// Finds development projects and their generated artifacts under `root`.
#[tauri::command]
pub fn scan_developer_storage(
    state: State<'_, AppState>,
    root: String,
    include_package_caches: bool,
) -> Result<DeveloperScanResult> {
    let root = stora_security::normalize(&root)?;
    let exclusions = state.exclusion_set()?;

    let task_id = format!("developer-{}", stora_core::now_seconds());
    let control = state.tasks.register("developer", &task_id)?;

    let outcome = (|| {
        let summary = stora_developer::scan_projects(&root, &exclusions, &control)?;
        let package_caches = if include_package_caches {
            stora_developer::detect_package_caches(&control)?
        } else {
            Vec::new()
        };
        Ok(DeveloperScanResult {
            summary,
            package_caches,
        })
    })();

    state.tasks.finish(&task_id);
    outcome
}

#[tauri::command]
pub fn cancel_developer_scan(state: State<'_, AppState>) -> Result<()> {
    if let Some((_, control)) = state.tasks.active_of_kind("developer") {
        control.cancel();
    }
    Ok(())
}

/// Virtual disks belonging to WSL, Docker, and virtual machine software.
#[tauri::command]
pub fn get_virtual_disks(state: State<'_, AppState>) -> Result<Vec<VirtualDisk>> {
    let task_id = format!("virtualdisk-{}", stora_core::now_seconds());
    let control = state.tasks.register("virtualdisk", &task_id)?;
    let outcome = stora_developer::detect_virtual_disks(&control);
    state.tasks.finish(&task_id);
    outcome
}

/// Turns chosen developer artifacts into an authorized cleanup plan.
///
/// The paths are re-derived here rather than trusted from the frontend: the
/// caller names artifact paths, and every one is checked against the artifact
/// rules again before it can enter a plan.
#[tauri::command]
pub fn build_developer_cleanup_plan(
    state: State<'_, AppState>,
    artifact_paths: Vec<String>,
) -> Result<crate::commands::cleanup::CleanupPlanResponse> {
    if artifact_paths.is_empty() {
        return Err(StoraError::Internal(
            "no development caches were selected".into(),
        ));
    }

    let exclusions = state.exclusion_set()?;
    let task_id = format!("plan-{}", stora_core::now_seconds());
    let control = state.tasks.register("plan", &task_id)?;

    let outcome = (|| {
        let mut items: Vec<CleanupItem> = Vec::new();
        let mut total_bytes = 0u64;

        for raw in &artifact_paths {
            control.checkpoint()?;

            let path = stora_security::normalize(raw)?;
            stora_security::ensure_deletable(&path)?;

            // Re-prove that this really is a removable artifact of a real
            // project, rather than accepting the frontend's word for it.
            verify_removable_artifact(&path)?;

            if exclusions.excludes_directory(&path) {
                continue;
            }

            collect_files(&path, &exclusions, &control, &mut items, &mut total_bytes)?;
        }

        let now = stora_core::now_seconds();
        let plan = CleanupPlan {
            plan_id: format!("devplan-{now}-{}", items.len()),
            created_at: now,
            expires_at: now + 15 * 60,
            categories: Vec::new(),
            file_count: items.len() as u64,
            folder_count: 0,
            total_bytes,
            items,
        };

        Ok(plan)
    })();

    state.tasks.finish(&task_id);
    let plan = outcome?;

    // Development caches are opt-in per project, so nothing is preselected.
    let response = crate::commands::cleanup::CleanupPlanResponse {
        default_selection: Vec::new(),
        plan: plan.clone(),
    };
    state.store_plan(plan);
    Ok(response)
}

/// Confirms a path is a classified, removable artifact of a real project.
fn verify_removable_artifact(path: &str) -> Result<()> {
    let name = stora_security::file_name_of(path);

    // A package-manager cache is identified by its own location, not by a
    // surrounding project.
    for cache in stora_developer::PACKAGE_CACHES {
        if let Some(expanded) = stora_winapi::expand_environment(cache.pattern) {
            if let Ok(normalized) = stora_security::normalize(&expanded) {
                if normalized.eq_ignore_ascii_case(path) {
                    return Ok(());
                }
            }
        }
    }

    let parent = stora_security::parent_of(path).ok_or_else(|| StoraError::PathNotAuthorized {
        path: path.to_string(),
    })?;

    let kinds = stora_developer::detect_project(&parent);
    let rule =
        stora_developer::classify(&name, &kinds).ok_or_else(|| StoraError::PathNotAuthorized {
            path: path.to_string(),
        })?;

    if !rule.label.is_removable() {
        return Err(StoraError::PathNotAuthorized {
            path: path.to_string(),
        });
    }

    Ok(())
}

fn collect_files(
    root: &str,
    exclusions: &stora_security::ExclusionSet,
    control: &stora_core::TaskControl,
    items: &mut Vec<CleanupItem>,
    total_bytes: &mut u64,
) -> Result<()> {
    let mut stack = vec![root.to_string()];

    while let Some(current) = stack.pop() {
        control.checkpoint()?;

        let extended = stora_security::to_extended_length(&current);
        let Ok(read) = std::fs::read_dir(&extended) else {
            continue;
        };

        for entry in read.flatten() {
            let path = entry.path().to_string_lossy().replace('/', "\\");
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if stora_security::is_protected(&path) || stora_security::is_sensitive(&path) {
                continue;
            }
            if metadata.is_dir() {
                if !exclusions.excludes_directory(&path) {
                    stack.push(path);
                }
                continue;
            }
            if exclusions.excludes_file(&path) {
                continue;
            }

            *total_bytes += metadata.len();
            items.push(CleanupItem {
                path,
                category_id: "developerCache".into(),
                size: metadata.len(),
                is_directory: false,
                modified: metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64),
            });
        }
    }

    Ok(())
}
