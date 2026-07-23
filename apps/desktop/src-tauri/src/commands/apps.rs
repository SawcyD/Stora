use serde::Serialize;
use tauri::State;

use stora_apps::{ActivitySource, AppActivity, AppFootprint, InstalledApp};
use stora_core::{Result, StoraError};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppWithActivity {
    #[serde(flatten)]
    pub app: InstalledApp,
    pub activity: AppActivity,
    /// Ready-to-display sentence, e.g. "Last observed by Stora — 143 days ago".
    pub activity_text: String,
}

/// Lists installed applications together with whatever activity Stora has
/// actually observed.
#[tauri::command]
pub fn get_installed_apps(state: State<'_, AppState>) -> Result<Vec<AppWithActivity>> {
    let apps = stora_apps::discover()?;
    let now = stora_core::now_seconds();
    let settings = state.settings()?;

    // Windows' own shell bookkeeping, read once for the whole list. It only
    // covers Explorer-launched programs, so it fills gaps rather than
    // replacing what Stora observed directly.
    let shell_history = if settings.enable_windows_activity_estimates {
        stora_activity::read_entries()
    } else {
        Vec::new()
    };

    let mut result = Vec::with_capacity(apps.len());

    for app in apps {
        // Activity is only claimed when there is a real observation behind it.
        let activity = match &app.install_location {
            Some(location) => match state.index.activity_within(location)? {
                // A launch Stora watched happen: the strongest evidence there
                // is, so it always wins.
                Some(stored) => {
                    let mut activity = AppActivity::from_source(
                        &app.id,
                        ActivitySource::ObservedByStora,
                        Some(stored.last_observed),
                    );
                    activity.first_observed = Some(stored.first_observed);
                    activity.launch_count = stored.launch_count;
                    activity.executable_path = Some(stored.executable_path);
                    activity
                }
                // Nothing observed. Fall back to the shell estimate, clearly
                // labelled as an estimate at medium confidence.
                None => match stora_activity::newest_within(&shell_history, location) {
                    Some(entry) => {
                        let mut activity = AppActivity::from_source(
                            &app.id,
                            ActivitySource::WindowsEstimate,
                            entry.last_executed,
                        );
                        activity.launch_count = entry.run_count as u64;
                        activity.executable_path = Some(entry.name.clone());
                        activity
                    }
                    None => AppActivity::unknown(&app.id),
                },
            },
            None => AppActivity::unknown(&app.id),
        };

        let activity_text = stora_apps::describe(&activity, now);

        result.push(AppWithActivity {
            app,
            activity,
            activity_text,
        });
    }

    Ok(result)
}

/// Measures an application's storage footprint, with evidence for each link.
#[tauri::command]
pub fn get_app_footprint(state: State<'_, AppState>, app_id: String) -> Result<AppFootprint> {
    let apps = stora_apps::discover()?;
    let app = apps
        .into_iter()
        .find(|candidate| candidate.id == app_id)
        .ok_or(StoraError::PathNotFound { path: app_id })?;

    let task_id = format!("footprint-{}", stora_core::now_seconds());
    let control = state.tasks.register("footprint", &task_id)?;
    let outcome = stora_apps::build_footprint(&app, &control);
    state.tasks.finish(&task_id);

    outcome
}

/// Everything the confirmation dialog needs before an uninstall runs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPreflight {
    pub app_id: String,
    pub app_name: String,
    pub method_label: String,
    pub can_uninstall: bool,
    /// Explains why, when `can_uninstall` is false.
    pub blocked_reason: Option<String>,
    /// The footprint captured now, used later to find what survived.
    pub footprint_bytes: u64,
    pub location_count: usize,
}

/// Inspects an application and reports how it would be removed.
///
/// Nothing is changed here. This exists so the confirmation dialog can state
/// the actual method and any blocker before the user commits.
#[tauri::command]
pub fn preflight_uninstall(
    state: State<'_, AppState>,
    app_id: String,
) -> Result<UninstallPreflight> {
    let app = find_app(&app_id)?;

    if !app.suggestable {
        return Ok(UninstallPreflight {
            app_id,
            app_name: app.name,
            method_label: "Not offered".into(),
            can_uninstall: false,
            blocked_reason: Some(
                "This looks like a runtime, redistributable, or driver that other \
                 software may depend on. Remove it from Windows Settings if you are sure."
                    .into(),
            ),
            footprint_bytes: 0,
            location_count: 0,
        });
    }

    let method = stora_apps::choose_method(&app, stora_winapi::winget_available());

    let blocked_reason = match &method {
        stora_apps::UninstallMethod::Unavailable => Some(
            "This application registered no uninstaller and is not known to winget. \
             Remove it from Windows Settings instead."
                .into(),
        ),
        _ => None,
    };

    // Capture the footprint now so it can be compared once the uninstaller
    // has finished.
    let task_id = format!("preflight-{}", stora_core::now_seconds());
    let control = state.tasks.register("footprint", &task_id)?;
    let footprint = stora_apps::build_footprint(&app, &control);
    state.tasks.finish(&task_id);
    let footprint = footprint?;

    state.remember_footprint(&app_id, &footprint);

    Ok(UninstallPreflight {
        app_id,
        app_name: app.name,
        method_label: method.label().to_string(),
        can_uninstall: blocked_reason.is_none(),
        blocked_reason,
        footprint_bytes: footprint.total_bytes,
        location_count: footprint.locations.len(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallStarted {
    pub started: bool,
    pub method_label: String,
    /// Stated verbatim. Never implies a restore point exists when it does not.
    pub restore_point_message: String,
    pub restore_point_created: bool,
}

/// Runs the application's own uninstaller.
///
/// Stora never removes software by deleting its directory, and never
/// uninstalls anything the user did not start here.
#[tauri::command]
pub fn start_uninstall(state: State<'_, AppState>, app_id: String) -> Result<UninstallStarted> {
    let app = find_app(&app_id)?;

    if !app.suggestable {
        return Err(StoraError::Internal(
            "This looks like a runtime or driver that other software may depend on. \
             Remove it from Windows Settings if you are sure."
                .into(),
        ));
    }

    let method = stora_apps::choose_method(&app, stora_winapi::winget_available());

    if method == stora_apps::UninstallMethod::Unavailable {
        return Err(StoraError::Internal(
            "This application registered no uninstaller. Remove it from Windows \
             Settings instead."
                .into(),
        ));
    }

    // Attempt a restore point, then report exactly what happened. A failure
    // here does not block the uninstall — it is information, not a gate.
    let restore =
        match stora_winapi::create_restore_point(&format!("Before uninstalling {}", app.name)) {
            Ok(()) => stora_apps::RestorePointOutcome::created(),
            Err(stora_winapi::RestoreFailure::Disabled) => {
                stora_apps::RestorePointOutcome::disabled()
            }
            Err(stora_winapi::RestoreFailure::NeedsElevation) => {
                stora_apps::RestorePointOutcome::needs_elevation()
            }
            Err(stora_winapi::RestoreFailure::RateLimited) => {
                stora_apps::RestorePointOutcome::rate_limited()
            }
            Err(stora_winapi::RestoreFailure::Other) => {
                stora_apps::RestorePointOutcome::failed("Windows did not report a reason")
            }
        };

    // Keep the footprint even if preflight was skipped.
    if state.remembered_footprint(&app_id).is_none() {
        let task_id = format!("uninstall-{}", stora_core::now_seconds());
        let control = state.tasks.register("footprint", &task_id)?;
        let captured = stora_apps::build_footprint(&app, &control);
        state.tasks.finish(&task_id);
        if let Ok(footprint) = captured {
            state.remember_footprint(&app_id, &footprint);
        }
    }

    match &method {
        stora_apps::UninstallMethod::RegisteredUninstaller(command) => run_uninstaller(command)?,
        stora_apps::UninstallMethod::Winget(id) => run_winget(id)?,
        stora_apps::UninstallMethod::Unavailable => unreachable!("checked above"),
    }

    Ok(UninstallStarted {
        started: true,
        method_label: method.label().to_string(),
        restore_point_message: restore.message,
        restore_point_created: restore.created,
    })
}

/// Re-measures an application's footprint after its uninstaller finished.
///
/// Called once the user confirms the uninstaller has closed. Anything still
/// present is offered through the ordinary cleanup pipeline.
#[tauri::command]
pub fn scan_uninstall_leftovers(
    state: State<'_, AppState>,
    app_id: String,
) -> Result<Vec<stora_apps::Leftover>> {
    let before = state.remembered_footprint(&app_id).ok_or_else(|| {
        StoraError::Internal(
            "No footprint was captured before this uninstall, so leftovers cannot be \
             identified."
                .into(),
        )
    })?;

    let task_id = format!("leftovers-{}", stora_core::now_seconds());
    let control = state.tasks.register("footprint", &task_id)?;

    let outcome = (|| {
        let mut leftovers = stora_apps::diff_footprint(&before, |path| {
            if !std::path::Path::new(&stora_security::to_extended_length(path)).exists() {
                return None;
            }
            stora_developer::directory_size(path, &control)
                .ok()
                .map(|(bytes, _)| bytes)
        });

        // The uninstall registry entry surviving is the usual registry
        // leftover. Reported, never removed.
        if stora_apps::discover()?
            .iter()
            .any(|candidate| candidate.id == app_id)
        {
            leftovers.push(stora_apps::registry_leftover(&format!(
                "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}",
                app_id
                    .split_once(':')
                    .map(|(_, key)| key)
                    .unwrap_or(&app_id)
            )));
        }

        Ok(leftovers)
    })();

    state.tasks.finish(&task_id);
    outcome
}

/// Turns surviving leftovers into an authorized cleanup plan.
#[tauri::command]
pub fn build_leftover_cleanup_plan(
    state: State<'_, AppState>,
    app_id: String,
    paths: Vec<String>,
) -> Result<crate::commands::cleanup::CleanupPlanResponse> {
    if paths.is_empty() {
        return Err(StoraError::Internal("no leftovers were selected".into()));
    }

    let before =
        state
            .remembered_footprint(&app_id)
            .ok_or_else(|| StoraError::CleanupPlanExpired {
                plan_id: app_id.clone(),
            })?;

    let task_id = format!("plan-{}", stora_core::now_seconds());
    let control = state.tasks.register("plan", &task_id)?;

    let outcome = (|| {
        let mut items: Vec<stora_core::cleanup::CleanupItem> = Vec::new();
        let mut total_bytes = 0u64;

        for raw in &paths {
            control.checkpoint()?;
            let path = stora_security::normalize(raw)?;
            stora_security::ensure_deletable(&path)?;

            // Only paths that were in the captured footprint may be removed.
            // The frontend cannot introduce a new one here.
            let authorized = before
                .iter()
                .any(|(known, _, _)| stora_security::is_within(&path, known));
            if !authorized {
                return Err(StoraError::PathNotAuthorized { path });
            }

            collect_leftover_files(&path, &control, &mut items, &mut total_bytes)?;
        }

        let now = stora_core::now_seconds();
        Ok(stora_core::cleanup::CleanupPlan {
            plan_id: format!("leftover-{now}-{}", items.len()),
            created_at: now,
            expires_at: now + 15 * 60,
            categories: Vec::new(),
            file_count: items.len() as u64,
            folder_count: 0,
            total_bytes,
            items,
        })
    })();

    state.tasks.finish(&task_id);
    let plan = outcome?;

    state.store_plan(plan.clone());
    Ok(crate::commands::cleanup::CleanupPlanResponse {
        plan,
        // Leftovers can include documents and settings a person may want, so
        // nothing is preselected.
        default_selection: Vec::new(),
    })
}

fn collect_leftover_files(
    root: &str,
    control: &stora_core::TaskControl,
    items: &mut Vec<stora_core::cleanup::CleanupItem>,
    total_bytes: &mut u64,
) -> Result<()> {
    let mut stack = vec![root.to_string()];

    while let Some(current) = stack.pop() {
        control.checkpoint()?;

        let Ok(read) = std::fs::read_dir(stora_security::to_extended_length(&current)) else {
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
                stack.push(path);
                continue;
            }

            *total_bytes += metadata.len();
            items.push(stora_core::cleanup::CleanupItem {
                path,
                category_id: "uninstallLeftover".into(),
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

fn find_app(app_id: &str) -> Result<InstalledApp> {
    stora_apps::discover()?
        .into_iter()
        .find(|candidate| candidate.id == app_id)
        .ok_or_else(|| StoraError::PathNotFound {
            path: app_id.to_string(),
        })
}

/// Runs a registered uninstall string.
///
/// The command comes from the application's own registry entry, never from the
/// frontend. It is handed to the shell exactly as Programs and Features would.
fn run_uninstaller(command: &str) -> Result<()> {
    #[cfg(windows)]
    {
        use std::process::Command;

        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Err(StoraError::Internal(
                "the uninstall command is empty".into(),
            ));
        }

        Command::new("cmd")
            .args(["/C", "start", "", trimmed])
            .spawn()
            .map_err(|err| {
                StoraError::Internal(format!("could not start the uninstaller: {err}"))
            })?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = command;
        Err(StoraError::UnsupportedFilesystem {
            filesystem: "uninstalling requires Windows".into(),
        })
    }
}

fn run_winget(package_id: &str) -> Result<()> {
    #[cfg(windows)]
    {
        use std::process::Command;

        // The id is passed as its own argument, so it cannot extend the
        // command line.
        Command::new("winget")
            .args(["uninstall", "--id", package_id, "--silent"])
            .spawn()
            .map_err(|err| StoraError::Internal(format!("could not start winget: {err}")))?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = package_id;
        Err(StoraError::UnsupportedFilesystem {
            filesystem: "winget requires Windows".into(),
        })
    }
}

/// Records the launches observed since the last poll.
///
/// Called by the frontend on a timer only while activity tracking is enabled.
#[tauri::command]
pub fn poll_application_activity(state: State<'_, AppState>) -> Result<u64> {
    let settings = state.settings()?;
    if !settings.track_application_launches {
        return Ok(0);
    }

    let processes = stora_activity::running_processes()?;
    let now = stora_core::now_seconds();

    let launches = state.observe_launches(&processes, now);
    if launches.is_empty() {
        return Ok(0);
    }

    let rows: Vec<(String, String, i64)> = launches
        .iter()
        .map(|launch| {
            (
                launch.executable_path.clone(),
                launch.executable_name.clone(),
                launch.observed_at,
            )
        })
        .collect();

    state.index.record_launches(&rows)?;
    Ok(rows.len() as u64)
}

/// Removes every stored launch observation.
#[tauri::command]
pub fn clear_application_activity(state: State<'_, AppState>) -> Result<()> {
    state.index.clear_activity()
}
