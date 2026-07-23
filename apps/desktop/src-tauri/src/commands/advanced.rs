use serde::{Deserialize, Serialize};
use tauri::State;

use stora_core::cleanup::{
    CleanupItem, CleanupItemError, CleanupPlan, CleanupResult, CleanupState, DeletionMethod,
    QuarantineItem,
};
use stora_core::{Result, StoraError};
use stora_duplicates::{Candidate, DuplicateReport, KeepStrategy};
use stora_index::automation::StoredRule;
use stora_rules::{Alert, GrowthEntry, Rule, TimeRange};

use crate::state::AppState;

// ------------------------------------------------------------- duplicates

/// Finds exact duplicates among the largest files of the current scan.
///
/// Candidates come from the scan index rather than a fresh walk, so this
/// reuses work already done and stays bounded.
#[tauri::command]
pub fn find_duplicates(
    state: State<'_, AppState>,
    root: String,
    minimum_bytes: u64,
    limit: usize,
) -> Result<DuplicateReport> {
    let root = stora_security::normalize(&root)?;
    let scan_id = state.resolve_scan(&root)?;

    let files = state
        .index
        .large_files(scan_id, minimum_bytes.max(1), limit.min(20_000))?;

    let candidates: Vec<Candidate> = files
        .into_iter()
        .map(|file| Candidate {
            path: file.path,
            size: file.logical_size,
            modified: file.modified,
        })
        .collect();

    let task_id = format!("duplicates-{}", stora_core::now_seconds());
    let control = state.tasks.register("duplicates", &task_id)?;
    let outcome = stora_duplicates::find(&candidates, minimum_bytes, &control);
    state.tasks.finish(&task_id);

    outcome
}

#[tauri::command]
pub fn cancel_duplicate_scan(state: State<'_, AppState>) -> Result<()> {
    if let Some((_, control)) = state.tasks.active_of_kind("duplicates") {
        control.cancel();
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateSelection {
    /// Paths the user chose to remove.
    pub paths: Vec<String>,
}

/// Turns a duplicate selection into an authorized cleanup plan.
///
/// The plan is built from re-validated paths, and the frontend then approves
/// indices into it, exactly as every other cleanup does.
#[tauri::command]
pub fn build_duplicate_cleanup_plan(
    state: State<'_, AppState>,
    selection: DuplicateSelection,
) -> Result<crate::commands::cleanup::CleanupPlanResponse> {
    if selection.paths.is_empty() {
        return Err(StoraError::Internal("no duplicates were selected".into()));
    }

    let mut items = Vec::with_capacity(selection.paths.len());
    let mut total_bytes = 0u64;

    for raw in &selection.paths {
        let path = stora_security::normalize(raw)?;
        stora_security::ensure_deletable(&path)?;

        let metadata = std::fs::symlink_metadata(stora_security::to_extended_length(&path))
            .map_err(|err| StoraError::from_io(&err, &path))?;
        if metadata.is_dir() {
            return Err(StoraError::PathNotAuthorized { path });
        }

        total_bytes += metadata.len();
        items.push(CleanupItem {
            path,
            category_id: "duplicate".into(),
            size: metadata.len(),
            is_directory: false,
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
        });
    }

    let now = stora_core::now_seconds();
    let plan = CleanupPlan {
        plan_id: format!("dupplan-{now}-{}", items.len()),
        created_at: now,
        expires_at: now + 15 * 60,
        categories: Vec::new(),
        file_count: items.len() as u64,
        folder_count: 0,
        total_bytes,
        items,
    };

    state.store_plan(plan.clone());
    Ok(crate::commands::cleanup::CleanupPlanResponse {
        plan,
        // Duplicates are never preselected: some identical files legitimately
        // belong in separate backup or program locations.
        default_selection: Vec::new(),
    })
}

/// Applies a keep-strategy to a group, returning the paths it would remove.
#[tauri::command]
pub fn apply_keep_strategy(
    group: stora_duplicates::DuplicateGroup,
    strategy: String,
) -> Vec<String> {
    let strategy = match strategy.as_str() {
        "oldest" => KeepStrategy::Oldest,
        "shortestPath" => KeepStrategy::ShortestPath,
        _ => KeepStrategy::Newest,
    };

    stora_duplicates::selection_for(&group, strategy)
        .into_iter()
        .filter_map(|index| group.files.get(index).map(|file| file.path.clone()))
        .collect()
}

// ----------------------------------------------------------------- growth

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrowthRow {
    pub path: String,
    pub name: String,
    pub current_bytes: u64,
    pub change_bytes: i64,
    pub has_baseline: bool,
    pub compared_at: i64,
}

/// Records the current size of the top-level folders of a scan.
///
/// Called after a scan so growth can be derived from snapshot differences
/// rather than by watching every file.
#[tauri::command]
pub fn record_growth_snapshot(state: State<'_, AppState>, root: String) -> Result<u64> {
    let root = stora_security::normalize(&root)?;
    let scan_id = state.resolve_scan(&root)?;
    let now = stora_core::now_seconds();

    let children = state.index.folder_children(scan_id, &root)?;
    let mut recorded = 0u64;

    for folder in children.iter().take(200) {
        state
            .index
            .record_folder_snapshot(&folder.path, now, folder.allocated_size)?;
        recorded += 1;
    }

    Ok(recorded)
}

#[tauri::command]
pub fn get_growth_history(state: State<'_, AppState>, range: String) -> Result<Vec<GrowthRow>> {
    let range = match range.as_str() {
        "day" => TimeRange::Day,
        "month" => TimeRange::Month,
        "quarter" => TimeRange::Quarter,
        "sinceInstall" => TimeRange::SinceInstall,
        _ => TimeRange::Week,
    };

    let now = stora_core::now_seconds();
    let mut rows = Vec::new();

    for path in state.index.tracked_folders()? {
        let snapshots: Vec<stora_rules::Snapshot> = state
            .index
            .folder_snapshots(&path)?
            .into_iter()
            .map(|(taken_at, bytes)| stora_rules::Snapshot { taken_at, bytes })
            .collect();

        if snapshots.is_empty() {
            continue;
        }

        let entry: GrowthEntry = stora_rules::change_over(&snapshots, range, now);
        rows.push(GrowthRow {
            name: stora_security::file_name_of(&path),
            path,
            current_bytes: entry.current_bytes,
            change_bytes: entry.change_bytes,
            has_baseline: entry.has_baseline,
            compared_at: entry.compared_at,
        });
    }

    // Biggest movers first; that is the question this view answers.
    rows.sort_by(|a, b| b.change_bytes.abs().cmp(&a.change_bytes.abs()));
    Ok(rows)
}

/// Local alerts derived from current state. Informative, never alarming.
#[tauri::command]
pub fn get_alerts(state: State<'_, AppState>) -> Result<Vec<Alert>> {
    let settings = state.settings()?;
    if !settings.show_notifications {
        return Ok(Vec::new());
    }

    let mut alerts = Vec::new();
    let threshold = 20 * 1024 * 1024 * 1024;

    for drive in stora_winapi::enumerate_drives()? {
        if let Some(alert) = stora_rules::low_space_alert(&drive.root, drive.free_bytes, threshold)
        {
            alerts.push(alert);
        }
    }

    let now = stora_core::now_seconds();
    let growth_threshold = 8 * 1024 * 1024 * 1024;

    for path in state.index.tracked_folders()? {
        let snapshots: Vec<stora_rules::Snapshot> = state
            .index
            .folder_snapshots(&path)?
            .into_iter()
            .map(|(taken_at, bytes)| stora_rules::Snapshot { taken_at, bytes })
            .collect();

        let entry = stora_rules::change_over(&snapshots, TimeRange::Week, now);
        if !entry.has_baseline {
            continue;
        }
        if let Some(alert) =
            stora_rules::growth_alert(&path, entry.change_bytes, growth_threshold, TimeRange::Week)
        {
            alerts.push(alert);
        }
    }

    Ok(alerts)
}

// ------------------------------------------------------------- automation

#[tauri::command]
pub fn get_automation_rules(state: State<'_, AppState>) -> Result<Vec<Rule>> {
    Ok(state
        .index
        .rules()?
        .into_iter()
        .map(stored_to_rule)
        .collect())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRule {
    pub name: String,
    pub trigger: String,
    pub action: String,
    pub weekday: u8,
    pub free_space_threshold: u64,
    pub growth_threshold: u64,
    pub watched_path: Option<String>,
    pub categories: Vec<String>,
    pub minimum_age_days: u32,
}

/// Creates a rule. It is always stored disabled.
#[tauri::command]
pub fn create_automation_rule(state: State<'_, AppState>, rule: NewRule) -> Result<Vec<Rule>> {
    if rule.name.trim().is_empty() {
        return Err(StoraError::Internal("the rule needs a name".into()));
    }

    // A rule that deletes may only name categories automation is allowed to
    // remove. Rejected here as well as at evaluation time.
    if rule.action == "cleanSafeCategories" {
        let refused: Vec<String> = rule
            .categories
            .iter()
            .filter(|category| !stora_rules::SAFE_CATEGORIES.contains(&category.as_str()))
            .cloned()
            .collect();

        if !refused.is_empty() {
            return Err(StoraError::Internal(format!(
                "Automation may not remove these categories: {}. Only regeneratable caches \
                 can be cleaned automatically.",
                refused.join(", ")
            )));
        }
    }

    let watched_path = match &rule.watched_path {
        Some(path) if !path.trim().is_empty() => Some(stora_security::normalize(path)?),
        _ => None,
    };

    let stored = StoredRule {
        id: 0,
        name: rule.name.trim().to_string(),
        // Never enabled on creation.
        enabled: false,
        trigger_kind: rule.trigger,
        action_kind: rule.action,
        weekday: rule.weekday.min(6),
        free_space_threshold: rule.free_space_threshold,
        growth_threshold: rule.growth_threshold,
        watched_path,
        categories: rule.categories,
        minimum_age_days: rule.minimum_age_days,
        last_run: None,
        consecutive_errors: 0,
    };

    state
        .index
        .create_rule(&stored, stora_core::now_seconds())?;
    get_automation_rules(state)
}

#[tauri::command]
pub fn set_rule_enabled(state: State<'_, AppState>, id: i64, enabled: bool) -> Result<Vec<Rule>> {
    state.index.set_rule_enabled(id, enabled)?;
    get_automation_rules(state)
}

#[tauri::command]
pub fn delete_automation_rule(state: State<'_, AppState>, id: i64) -> Result<Vec<Rule>> {
    state.index.delete_rule(id)?;
    get_automation_rules(state)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRunRow {
    pub ran_at: i64,
    pub outcome: String,
    pub detail: String,
    pub recovered_bytes: u64,
}

#[tauri::command]
pub fn get_rule_history(state: State<'_, AppState>, id: i64) -> Result<Vec<RuleRunRow>> {
    Ok(state
        .index
        .rule_runs(id, 50)?
        .into_iter()
        .map(|run| RuleRunRow {
            ran_at: run.ran_at,
            outcome: run.outcome,
            detail: run.detail,
            recovered_bytes: run.recovered_bytes,
        })
        .collect())
}

/// Evaluates every rule and reports what would happen.
///
/// This is a dry run: it never modifies files. Rules that would fire are
/// reported so the user can see the automation working before trusting it.
#[tauri::command]
pub fn evaluate_automation_rules(state: State<'_, AppState>) -> Result<Vec<String>> {
    preview_automation_rules(&state)
}

/// Evaluates enabled rules without changing files. This powers the explicit
/// "Check rules now" control, whereas the background scheduler calls
/// `run_automation_cycle` below.
fn preview_automation_rules(state: &AppState) -> Result<Vec<String>> {
    let now = stora_core::now_seconds();
    let drives = stora_winapi::enumerate_drives().unwrap_or_default();
    let free_bytes = drives
        .iter()
        .find(|drive| drive.root.eq_ignore_ascii_case("C:\\"))
        .map(|drive| drive.free_bytes)
        .unwrap_or(u64::MAX);

    // 0 = Sunday, matching the rule model.
    let weekday = (((now / 86_400) + 4) % 7) as u8;

    let mut messages = Vec::new();

    for stored in state.index.rules()? {
        let rule = stored_to_rule(stored);

        let growth = match &rule.watched_path {
            Some(path) => {
                let snapshots: Vec<stora_rules::Snapshot> = state
                    .index
                    .folder_snapshots(path)?
                    .into_iter()
                    .map(|(taken_at, bytes)| stora_rules::Snapshot { taken_at, bytes })
                    .collect();
                let entry = stora_rules::change_over(&snapshots, TimeRange::Week, now);
                entry.change_bytes.max(0) as u64
            }
            None => 0,
        };

        let conditions = stora_rules::Conditions {
            now,
            weekday,
            free_bytes,
            weekly_growth_bytes: growth,
        };

        match stora_rules::should_run(&rule, &conditions) {
            Ok(()) => messages.push(format!("{} would run now.", rule.name)),
            Err(stora_rules::Skip::Disabled) => {}
            Err(stora_rules::Skip::ErrorLimitReached) => messages.push(format!(
                "{} is paused after repeated errors. Re-enable it to try again.",
                rule.name
            )),
            Err(stora_rules::Skip::UnsafeCategories(categories)) => messages.push(format!(
                "{} names categories automation may not remove: {}.",
                rule.name,
                categories.join(", ")
            )),
            Err(_) => {}
        }
    }

    Ok(messages)
}

/// Runs each currently eligible rule once. It is intentionally independent of
/// Tauri command input so the tray-resident scheduler can call it directly.
///
/// File-changing rules always rebuild a fresh plan, select every item only
/// after backend authorization, and use the same revalidation executor as a
/// manually approved cleanup. Nothing here accepts a path from the scheduler
/// or the UI.
pub fn run_automation_cycle(state: &AppState) -> Result<Vec<String>> {
    let now = stora_core::now_seconds();
    let drives = stora_winapi::enumerate_drives().unwrap_or_default();
    let free_bytes = drives
        .iter()
        .find(|drive| drive.root.eq_ignore_ascii_case("C:\\"))
        .map(|drive| drive.free_bytes)
        .unwrap_or(u64::MAX);
    let weekday = (((now / 86_400) + 4) % 7) as u8;
    let mut messages = Vec::new();

    for stored in state.index.rules()? {
        let rule = stored_to_rule(stored);
        let growth = weekly_growth_for(state, rule.watched_path.as_deref(), now)?;
        let conditions = stora_rules::Conditions {
            now,
            weekday,
            free_bytes,
            weekly_growth_bytes: growth,
        };

        if let Err(skip) = stora_rules::should_run(&rule, &conditions) {
            if matches!(skip, stora_rules::Skip::ErrorLimitReached) {
                messages.push(format!("{} is paused after repeated errors.", rule.name));
            }
            continue;
        }

        let result = match rule.action {
            stora_rules::Action::CleanSafeCategories => run_safe_cleanup_rule(state, &rule, now)
                .map(|recovered| {
                    (
                        format!(
                            "{} cleared {}.",
                            rule.name,
                            stora_core::format_bytes(recovered)
                        ),
                        recovered,
                    )
                }),
            stora_rules::Action::Notify => Ok((format!("{} needs your attention.", rule.name), 0)),
            stora_rules::Action::OpenCleanupReview => {
                Ok((format!("{} is ready for cleanup review.", rule.name), 0))
            }
        };

        match result {
            Ok((detail, recovered_bytes)) => {
                state.index.record_rule_run(
                    rule.id,
                    now,
                    "completed",
                    &detail,
                    recovered_bytes,
                    true,
                )?;
                messages.push(detail);
            }
            Err(error) => {
                let detail = error.to_string();
                // A failed run still gets a history record. After three
                // failures the existing rule guard pauses it until re-enabled.
                state
                    .index
                    .record_rule_run(rule.id, now, "failed", &detail, 0, false)?;
                messages.push(format!("{} could not run: {detail}", rule.name));
            }
        }
    }

    Ok(messages)
}

/// Returns weekly growth for a watched folder, treating no history as zero.
fn weekly_growth_for(state: &AppState, watched_path: Option<&str>, now: i64) -> Result<u64> {
    let Some(path) = watched_path else {
        return Ok(0);
    };
    let snapshots: Vec<stora_rules::Snapshot> = state
        .index
        .folder_snapshots(path)?
        .into_iter()
        .map(|(taken_at, bytes)| stora_rules::Snapshot { taken_at, bytes })
        .collect();
    Ok(stora_rules::change_over(&snapshots, TimeRange::Week, now)
        .change_bytes
        .max(0) as u64)
}

/// Executes a fresh, allow-listed cleanup plan for one enabled rule.
fn run_safe_cleanup_rule(state: &AppState, rule: &Rule, now: i64) -> Result<u64> {
    let categories = stora_rules::permitted_categories(rule);
    if categories.len() != rule.categories.len() || categories.is_empty() {
        return Err(StoraError::Internal(
            "automation rule contains categories that are not permitted for unattended cleanup"
                .into(),
        ));
    }

    let exclusions = state.exclusion_set()?;
    let task_id = format!("automation-plan-{}-{now}", rule.id);
    let control = state.tasks.register("cleanup", &task_id)?;
    let plan = stora_cleaner::build_plan(
        &stora_cleaner::PlanRequest {
            category_ids: categories,
            include_advanced: false,
        },
        &exclusions,
        &control,
        now,
    );
    state.tasks.finish(&task_id);
    let mut plan = plan?;

    // A missing timestamp is not good enough evidence for unattended removal.
    let minimum_modified = now - i64::from(rule.minimum_age_days) * 86_400;
    plan.items
        .retain(|item| item.modified.is_some_and(|at| at <= minimum_modified));
    if plan.items.is_empty() {
        return Ok(0);
    }

    let selected_indices: Vec<usize> = (0..plan.items.len()).collect();
    let authorized = stora_security::authorize_selection(&plan, &selected_indices, now)?;
    if authorized.is_empty() {
        return Ok(0);
    }

    let category_ids = rule.categories.clone();
    let operation_id = state.index.begin_cleanup(
        &plan.plan_id,
        now,
        DeletionMethod::Permanent,
        authorized.len() as u64,
        &category_ids,
    )?;

    let task_id = format!("automation-cleanup-{}-{now}", rule.id);
    let control = state.tasks.register("cleanup", &task_id)?;
    let mut reporter = SilentReporter;
    let outcome = stora_cleaner::execute(
        &task_id,
        &authorized,
        DeletionMethod::Permanent,
        None,
        &control,
        &mut reporter,
    );
    state.tasks.finish(&task_id);
    let outcome = outcome?;

    state
        .index
        .record_cleanup_items(operation_id, &outcome.removed, &outcome.failed)?;
    let errors: Vec<CleanupItemError> = outcome
        .failed
        .iter()
        .map(|(_, error)| error.clone())
        .collect();
    let result = CleanupResult {
        operation_id,
        state: if errors.is_empty() {
            CleanupState::Completed
        } else {
            CleanupState::CompletedWithErrors
        },
        recovered_bytes: outcome.recovered_bytes,
        files_removed: outcome.removed.len() as u64,
        files_skipped: outcome.failed.len() as u64,
        duration_ms: outcome.duration_ms,
        method: DeletionMethod::Permanent,
        errors,
    };
    state.index.finish_cleanup(&result)?;
    Ok(result.recovered_bytes)
}

struct SilentReporter;

impl stora_cleaner::CleanupReporter for SilentReporter {
    fn progress(&mut self, _progress: &stora_core::cleanup::CleanupProgress) {}
}

fn stored_to_rule(stored: StoredRule) -> Rule {
    Rule {
        id: stored.id,
        name: stored.name,
        enabled: stored.enabled,
        trigger: match stored.trigger_kind.as_str() {
            "lowFreeSpace" => stora_rules::Trigger::LowFreeSpace,
            "folderGrowth" => stora_rules::Trigger::FolderGrowth,
            _ => stora_rules::Trigger::Weekly,
        },
        action: match stored.action_kind.as_str() {
            "openCleanupReview" => stora_rules::Action::OpenCleanupReview,
            "cleanSafeCategories" => stora_rules::Action::CleanSafeCategories,
            _ => stora_rules::Action::Notify,
        },
        weekday: stored.weekday,
        free_space_threshold: stored.free_space_threshold,
        growth_threshold: stored.growth_threshold,
        watched_path: stored.watched_path,
        categories: stored.categories,
        minimum_age_days: stored.minimum_age_days,
        last_run: stored.last_run,
        consecutive_errors: stored.consecutive_errors,
    }
}

// ------------------------------------------------------------- quarantine

#[tauri::command]
pub fn get_quarantine_items(state: State<'_, AppState>) -> Result<Vec<QuarantineItem>> {
    state.index.quarantine_items()
}

/// Restores a quarantined file to where it came from.
#[tauri::command]
pub fn restore_quarantine_item(state: State<'_, AppState>, id: i64) -> Result<()> {
    let item = state
        .index
        .quarantine_item(id)?
        .ok_or_else(|| StoraError::PathNotFound {
            path: format!("quarantine item {id}"),
        })?;

    let original = stora_security::normalize(&item.original_path)?;
    stora_security::ensure_deletable(&original)?;

    // Refuse to overwrite something that now occupies the original path.
    if std::path::Path::new(&stora_security::to_extended_length(&original)).exists() {
        return Err(StoraError::PathChangedAfterPreview { path: original });
    }

    if let Some(parent) = stora_security::parent_of(&original) {
        std::fs::create_dir_all(stora_security::to_extended_length(&parent))
            .map_err(|err| StoraError::from_io(&err, &parent))?;
    }

    std::fs::rename(
        stora_security::to_extended_length(&item.quarantine_path),
        stora_security::to_extended_length(&original),
    )
    .map_err(|err| StoraError::from_io(&err, &item.quarantine_path))?;

    state.index.mark_quarantine_restored(id)
}

/// Permanently removes a quarantined file after explicit approval.
#[tauri::command]
pub fn purge_quarantine_item(state: State<'_, AppState>, id: i64) -> Result<()> {
    let item = state
        .index
        .quarantine_item(id)?
        .ok_or_else(|| StoraError::PathNotFound {
            path: format!("quarantine item {id}"),
        })?;

    // A missing file is not an error here — the record should still go.
    let _ = std::fs::remove_file(stora_security::to_extended_length(&item.quarantine_path));
    state.index.remove_quarantine_record(id)
}

/// Total bytes currently held in quarantine.
#[tauri::command]
pub fn get_quarantine_size(state: State<'_, AppState>) -> Result<u64> {
    Ok(state
        .index
        .quarantine_items()?
        .iter()
        .map(|item| item.size)
        .sum())
}
