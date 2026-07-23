use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use stora_core::cleanup::CleanupPlan;
use stora_core::{Result, Settings, StoraError, TaskRegistry};
use stora_index::Index;
use stora_security::ExclusionSet;

/// An application's footprint as measured before its uninstaller ran:
/// `(path, relationship, bytes)` for each location.
pub type CapturedFootprint = Vec<(String, String, u64)>;

/// Shared application state, owned by Tauri and available to every command.
pub struct AppState {
    pub index: Arc<Index>,
    pub tasks: Arc<TaskRegistry>,
    /// Plans generated this session, keyed by plan id.
    ///
    /// Held in memory rather than the database on purpose: a plan is only
    /// valid for the few minutes between preview and execution, and a stale
    /// plan must never survive a restart.
    plans: Mutex<HashMap<String, CleanupPlan>>,
    /// The scan whose results the UI is currently browsing.
    active_scan: Mutex<Option<i64>>,
    /// Diffs successive process snapshots into launch observations.
    launch_observer: Mutex<stora_activity::LaunchObserver>,
    /// Footprints captured just before an uninstall, keyed by application id.
    ///
    /// Held in memory only: it is meaningful for the few minutes between
    /// starting an uninstaller and checking what survived, and a stale entry
    /// from a previous session would be worse than none.
    pre_uninstall_footprints: Mutex<HashMap<String, CapturedFootprint>>,
    data_dir: PathBuf,
}

impl AppState {
    pub fn new(index: Index, data_dir: PathBuf) -> Self {
        Self {
            index: Arc::new(index),
            tasks: Arc::new(TaskRegistry::new()),
            plans: Mutex::new(HashMap::new()),
            active_scan: Mutex::new(None),
            launch_observer: Mutex::new(stora_activity::LaunchObserver::new()),
            pre_uninstall_footprints: Mutex::new(HashMap::new()),
            data_dir,
        }
    }

    /// Records an application's footprint before its uninstaller runs.
    pub fn remember_footprint(&self, app_id: &str, footprint: &stora_apps::AppFootprint) {
        let captured: CapturedFootprint = footprint
            .locations
            .iter()
            .map(|location| {
                (
                    location.path.clone(),
                    location.relationship.clone(),
                    location.bytes,
                )
            })
            .collect();

        let mut guard = self
            .pre_uninstall_footprints
            .lock()
            .expect("footprint store poisoned");
        guard.insert(app_id.to_string(), captured);
    }

    pub fn remembered_footprint(&self, app_id: &str) -> Option<CapturedFootprint> {
        let guard = self
            .pre_uninstall_footprints
            .lock()
            .expect("footprint store poisoned");
        guard.get(app_id).cloned()
    }

    /// Feeds a process snapshot to the observer and returns new launches.
    pub fn observe_launches(
        &self,
        processes: &[stora_activity::ProcessInfo],
        now: i64,
    ) -> Vec<stora_activity::ObservedLaunch> {
        let mut observer = self.launch_observer.lock().expect("observer poisoned");
        observer.observe(processes, now)
    }

    pub fn quarantine_dir(&self) -> PathBuf {
        self.data_dir.join("quarantine")
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    pub fn store_plan(&self, plan: CleanupPlan) {
        let mut guard = self.plans.lock().expect("plan store poisoned");
        // Drop plans that can no longer be executed so the map cannot grow
        // without bound during a long session.
        let now = stora_core::now_seconds();
        guard.retain(|_, existing| !existing.is_expired(now));
        guard.insert(plan.plan_id.clone(), plan);
    }

    pub fn take_plan(&self, plan_id: &str) -> Result<CleanupPlan> {
        let guard = self.plans.lock().expect("plan store poisoned");
        guard
            .get(plan_id)
            .cloned()
            .ok_or_else(|| StoraError::CleanupPlanExpired {
                plan_id: plan_id.to_string(),
            })
    }

    pub fn set_active_scan(&self, scan_id: i64) {
        *self.active_scan.lock().expect("scan slot poisoned") = Some(scan_id);
    }

    /// The scan the UI should query, falling back to the newest completed scan
    /// for `root` so results survive a restart.
    pub fn resolve_scan(&self, root: &str) -> Result<i64> {
        if let Some(scan_id) = *self.active_scan.lock().expect("scan slot poisoned") {
            return Ok(scan_id);
        }
        match self.index.latest_scan(root)? {
            Some(summary) => {
                self.set_active_scan(summary.scan_id);
                Ok(summary.scan_id)
            }
            None => Err(StoraError::PathNotFound {
                path: root.to_string(),
            }),
        }
    }

    pub fn settings(&self) -> Result<Settings> {
        match self.index.get_setting("settings")? {
            Some(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
            None => Ok(Settings::default()),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let raw =
            serde_json::to_string(settings).map_err(|err| StoraError::Internal(err.to_string()))?;
        self.index.set_setting("settings", &raw)
    }

    /// Builds the exclusion set from stored rules.
    pub fn exclusion_set(&self) -> Result<ExclusionSet> {
        Ok(ExclusionSet::from_rules(&self.index.exclusions()?))
    }
}
