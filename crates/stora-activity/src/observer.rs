use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::snapshot::ProcessInfo;

/// A launch Stora actually witnessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedLaunch {
    pub executable_path: String,
    pub executable_name: String,
    pub observed_at: i64,
}

/// Turns successive process snapshots into launch observations.
///
/// A launch is recorded when a process id appears that was not present in the
/// previous snapshot. Process ids are reused by Windows, so the identity used
/// for comparison is the pid *and* the executable name together.
#[derive(Debug, Default)]
pub struct LaunchObserver {
    previous: HashSet<(u32, String)>,
    /// False until the first snapshot has been taken.
    primed: bool,
}

impl LaunchObserver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds in a snapshot and returns the launches it reveals.
    ///
    /// The very first snapshot records nothing: every process already running
    /// when Stora starts would otherwise be reported as having just launched.
    pub fn observe(&mut self, processes: &[ProcessInfo], now: i64) -> Vec<ObservedLaunch> {
        let current: HashSet<(u32, String)> = processes
            .iter()
            .map(|process| (process.pid, process.executable_name.to_ascii_lowercase()))
            .collect();

        if !self.primed {
            self.previous = current;
            self.primed = true;
            return Vec::new();
        }

        let mut launches = Vec::new();

        for process in processes {
            let identity = (process.pid, process.executable_name.to_ascii_lowercase());
            if self.previous.contains(&identity) {
                continue;
            }

            // Without an image path there is nothing durable to attribute the
            // launch to, so it is dropped rather than recorded against a bare
            // executable name that several programs could share.
            let Some(path) = &process.executable_path else {
                continue;
            };
            let Ok(normalized) = stora_security::normalize(path) else {
                continue;
            };

            launches.push(ObservedLaunch {
                executable_path: normalized,
                executable_name: process.executable_name.clone(),
                observed_at: now,
            });
        }

        self.previous = current;
        launches
    }

    pub fn is_primed(&self) -> bool {
        self.primed
    }
}

/// Matches an observed executable path to an application's install location.
///
/// Returns true only when the executable actually lives inside the folder the
/// application's own uninstall entry named.
pub fn belongs_to(executable_path: &str, install_location: &str) -> bool {
    stora_security::is_within(executable_path, install_location)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, name: &str, path: Option<&str>) -> ProcessInfo {
        ProcessInfo {
            pid,
            executable_name: name.to_string(),
            executable_path: path.map(str::to_string),
        }
    }

    #[test]
    fn the_first_snapshot_records_nothing() {
        let mut observer = LaunchObserver::new();
        let launches = observer.observe(
            &[process(
                1,
                "app.exe",
                Some("C:\\Program Files\\App\\app.exe"),
            )],
            100,
        );
        assert!(
            launches.is_empty(),
            "processes already running are not launches"
        );
        assert!(observer.is_primed());
    }

    #[test]
    fn a_new_process_is_recorded_as_a_launch() {
        let mut observer = LaunchObserver::new();
        observer.observe(&[process(1, "old.exe", Some("C:\\Old\\old.exe"))], 100);

        let launches = observer.observe(
            &[
                process(1, "old.exe", Some("C:\\Old\\old.exe")),
                process(2, "new.exe", Some("C:\\New\\new.exe")),
            ],
            200,
        );

        assert_eq!(launches.len(), 1);
        assert_eq!(launches[0].executable_name, "new.exe");
        assert_eq!(launches[0].executable_path, "C:\\New\\new.exe");
        assert_eq!(launches[0].observed_at, 200);
    }

    #[test]
    fn a_still_running_process_is_not_relaunched() {
        let mut observer = LaunchObserver::new();
        let running = [process(1, "app.exe", Some("C:\\App\\app.exe"))];

        observer.observe(&running, 100);
        assert!(observer.observe(&running, 200).is_empty());
        assert!(observer.observe(&running, 300).is_empty());
    }

    #[test]
    fn a_reused_pid_with_a_different_executable_counts_as_a_launch() {
        let mut observer = LaunchObserver::new();
        observer.observe(&[process(500, "first.exe", Some("C:\\A\\first.exe"))], 100);

        // Windows reused pid 500 for a different program.
        let launches = observer.observe(
            &[process(500, "second.exe", Some("C:\\B\\second.exe"))],
            200,
        );

        assert_eq!(launches.len(), 1);
        assert_eq!(launches[0].executable_name, "second.exe");
    }

    #[test]
    fn a_process_without_an_image_path_is_not_recorded() {
        let mut observer = LaunchObserver::new();
        observer.observe(&[], 100);

        // Protected processes cannot be attributed to an application.
        let launches = observer.observe(&[process(9, "protected.exe", None)], 200);
        assert!(launches.is_empty());
    }

    #[test]
    fn a_relaunch_after_exit_is_observed_again() {
        let mut observer = LaunchObserver::new();
        observer.observe(&[process(1, "app.exe", Some("C:\\App\\app.exe"))], 100);

        // The process exits...
        assert!(observer.observe(&[], 200).is_empty());

        // ...and starts again with a new pid.
        let launches = observer.observe(&[process(7, "app.exe", Some("C:\\App\\app.exe"))], 300);
        assert_eq!(launches.len(), 1);
        assert_eq!(launches[0].observed_at, 300);
    }

    #[test]
    fn executable_name_comparison_is_case_insensitive() {
        let mut observer = LaunchObserver::new();
        observer.observe(&[process(1, "App.exe", Some("C:\\App\\App.exe"))], 100);

        // The same process reported with different casing is not a launch.
        let launches = observer.observe(&[process(1, "APP.EXE", Some("C:\\App\\App.exe"))], 200);
        assert!(launches.is_empty());
    }

    #[test]
    fn attribution_requires_the_executable_to_live_in_the_install_folder() {
        assert!(belongs_to(
            "C:\\Program Files\\App\\bin\\app.exe",
            "C:\\Program Files\\App"
        ));
        assert!(!belongs_to(
            "C:\\Program Files\\Other\\app.exe",
            "C:\\Program Files\\App"
        ));
        // A sibling folder with a shared prefix must not match.
        assert!(!belongs_to(
            "C:\\Program Files\\App2\\app.exe",
            "C:\\Program Files\\App"
        ));
    }

    #[test]
    fn several_launches_in_one_interval_are_all_reported() {
        let mut observer = LaunchObserver::new();
        observer.observe(&[], 100);

        let launches = observer.observe(
            &[
                process(1, "a.exe", Some("C:\\A\\a.exe")),
                process(2, "b.exe", Some("C:\\B\\b.exe")),
            ],
            200,
        );
        assert_eq!(launches.len(), 2);
    }
}
