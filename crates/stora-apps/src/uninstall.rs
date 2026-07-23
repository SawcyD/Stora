//! Uninstall orchestration and leftover detection.
//!
//! Stora never removes software by deleting its folder. It runs the vendor's
//! own uninstaller, then looks at what survived and offers the remainder
//! through the ordinary cleanup pipeline, where the same plan-and-revalidate
//! rules apply as everywhere else.

use serde::{Deserialize, Serialize};

use crate::model::InstalledApp;

/// How Stora intends to remove an application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum UninstallMethod {
    /// The uninstall command the application registered with Windows.
    RegisteredUninstaller(String),
    /// `winget uninstall`, when the application registered no uninstaller.
    Winget(String),
    /// Nothing usable was found; the user is pointed at Windows Settings.
    Unavailable,
}

impl UninstallMethod {
    pub fn label(&self) -> &'static str {
        match self {
            Self::RegisteredUninstaller(_) => "The application's own uninstaller",
            Self::Winget(_) => "Windows Package Manager (winget)",
            Self::Unavailable => "No uninstaller is available",
        }
    }
}

/// Chooses how to uninstall, preferring the vendor's own mechanism.
///
/// Returns [`UninstallMethod::Unavailable`] rather than inventing a fallback
/// that deletes files — there is deliberately no path here that removes a
/// program directory.
pub fn choose_method(app: &InstalledApp, winget_available: bool) -> UninstallMethod {
    if let Some(command) = &app.uninstall_command {
        let trimmed = command.trim();
        if !trimmed.is_empty() {
            return UninstallMethod::RegisteredUninstaller(trimmed.to_string());
        }
    }

    // The uninstall registry key name is the package id for most winget
    // sources, so it is the best identifier available without a live query.
    if winget_available {
        if let Some(id) = app.id.split_once(':').map(|(_, key)| key) {
            if !id.is_empty() {
                return UninstallMethod::Winget(id.to_string());
            }
        }
    }

    UninstallMethod::Unavailable
}

/// The outcome of trying to create a System Restore point.
///
/// Reported verbatim to the user. Stora must never imply a restore point
/// exists when it does not: System Restore is switched off by default on most
/// Windows 11 installs, needs elevation, and refuses more than one point in a
/// 24-hour window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePointOutcome {
    pub created: bool,
    /// Shown in the confirmation dialog exactly as written.
    pub message: String,
}

impl RestorePointOutcome {
    pub fn created() -> Self {
        Self {
            created: true,
            message: "A System Restore point was created before this uninstall.".into(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            created: false,
            message: "No restore point was created: System Restore is turned off for this \
                      drive. Windows disables it by default on many installations."
                .into(),
        }
    }

    pub fn needs_elevation() -> Self {
        Self {
            created: false,
            message: "No restore point was created: creating one needs administrator \
                      permission, and Stora does not run elevated."
                .into(),
        }
    }

    pub fn rate_limited() -> Self {
        Self {
            created: false,
            message: "No restore point was created: Windows allows only one per 24 hours, \
                      and a recent one already exists."
                .into(),
        }
    }

    pub fn failed(detail: &str) -> Self {
        Self {
            created: false,
            message: format!("No restore point was created: {detail}"),
        }
    }
}

/// A file or registry key left behind after an uninstaller finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    pub path: String,
    pub relationship: String,
    pub bytes: u64,
    /// False for registry keys, which Stora reports but never removes.
    pub removable: bool,
    pub reason: String,
}

/// Compares the footprint captured before an uninstall with what survives.
///
/// Only locations that still exist are returned, and their sizes are the
/// *current* ones — the pre-uninstall figures are stale by definition.
pub fn diff_footprint(
    before: &[(String, String, u64)],
    still_present: impl Fn(&str) -> Option<u64>,
) -> Vec<Leftover> {
    let mut leftovers = Vec::new();

    for (path, relationship, _) in before {
        let Some(bytes) = still_present(path) else {
            continue;
        };
        if bytes == 0 {
            continue;
        }

        leftovers.push(Leftover {
            path: path.clone(),
            relationship: relationship.clone(),
            bytes,
            removable: true,
            reason: "This folder was part of the application's footprint and survived its \
                     uninstaller."
                .into(),
        });
    }

    leftovers.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    leftovers
}

/// Wraps an orphaned registry key as a reported, non-removable leftover.
///
/// Registry leftovers are kilobytes. Removing them has no measurable benefit
/// and a real chance of breaking something, so Stora shows them and stops
/// there.
pub fn registry_leftover(key_path: &str) -> Leftover {
    Leftover {
        path: key_path.to_string(),
        relationship: "Registry key".into(),
        bytes: 0,
        removable: false,
        reason: "Reported for completeness. Stora does not remove registry keys: they \
                 occupy almost no space and removing them risks breaking other software."
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AppType, Confidence};

    fn app(uninstall: Option<&str>) -> InstalledApp {
        InstalledApp {
            id: "machine:TestApp".into(),
            name: "Test App".into(),
            publisher: "Contoso".into(),
            version: "1.0".into(),
            reported_bytes: None,
            detected_bytes: None,
            install_location: Some("C:\\Program Files\\Test App".into()),
            install_date: None,
            app_type: AppType::DesktopApplication,
            app_type_label: "Desktop application".into(),
            uninstall_command: uninstall.map(str::to_string),
            source: "test".into(),
            confidence: Confidence::High,
            confidence_label: "High".into(),
            suggestable: true,
        }
    }

    #[test]
    fn the_registered_uninstaller_is_always_preferred() {
        let method = choose_method(&app(Some("\"C:\\App\\unins.exe\" /S")), true);
        assert!(matches!(method, UninstallMethod::RegisteredUninstaller(_)));
    }

    #[test]
    fn winget_is_used_only_when_no_uninstaller_is_registered() {
        let method = choose_method(&app(None), true);
        match method {
            UninstallMethod::Winget(id) => assert_eq!(id, "TestApp"),
            other => panic!("expected winget, got {other:?}"),
        }
    }

    #[test]
    fn a_blank_uninstall_string_falls_through_to_winget() {
        let method = choose_method(&app(Some("   ")), true);
        assert!(matches!(method, UninstallMethod::Winget(_)));
    }

    #[test]
    fn without_winget_the_user_is_pointed_at_windows_settings() {
        // The important property: there is no fallback that deletes a folder.
        let method = choose_method(&app(None), false);
        assert_eq!(method, UninstallMethod::Unavailable);
    }

    #[test]
    fn no_method_ever_describes_deleting_a_directory() {
        for method in [
            choose_method(&app(Some("unins.exe")), true),
            choose_method(&app(None), true),
            choose_method(&app(None), false),
        ] {
            let label = method.label().to_lowercase();
            assert!(!label.contains("delete"), "got: {label}");
            assert!(!label.contains("remove folder"), "got: {label}");
        }
    }

    #[test]
    fn every_restore_point_failure_says_so_plainly() {
        for outcome in [
            RestorePointOutcome::disabled(),
            RestorePointOutcome::needs_elevation(),
            RestorePointOutcome::rate_limited(),
            RestorePointOutcome::failed("the service did not respond"),
        ] {
            assert!(!outcome.created);
            assert!(
                outcome.message.starts_with("No restore point was created"),
                "a failure must never read as a success: {}",
                outcome.message
            );
        }
    }

    #[test]
    fn a_successful_restore_point_is_stated_without_hedging() {
        let outcome = RestorePointOutcome::created();
        assert!(outcome.created);
        assert!(outcome.message.contains("was created"));
    }

    #[test]
    fn leftovers_are_only_the_locations_that_survived() {
        let before = vec![
            (
                "C:\\Program Files\\App".to_string(),
                "Application files".to_string(),
                5_000,
            ),
            (
                "C:\\Users\\Test\\AppData\\Local\\App".to_string(),
                "User data".to_string(),
                2_000,
            ),
        ];

        // The uninstaller removed the program directory but left user data.
        let leftovers = diff_footprint(&before, |path| {
            if path.contains("AppData") {
                Some(1_500)
            } else {
                None
            }
        });

        assert_eq!(leftovers.len(), 1);
        assert!(leftovers[0].path.contains("AppData"));
        assert_eq!(
            leftovers[0].bytes, 1_500,
            "the current size is reported, not the pre-uninstall one"
        );
    }

    #[test]
    fn a_clean_uninstall_leaves_nothing_to_report() {
        let before = vec![(
            "C:\\Program Files\\App".to_string(),
            "Application files".to_string(),
            5_000,
        )];
        assert!(diff_footprint(&before, |_| None).is_empty());
    }

    #[test]
    fn an_emptied_folder_is_not_reported_as_a_leftover() {
        let before = vec![(
            "C:\\Program Files\\App".to_string(),
            "Application files".to_string(),
            5_000,
        )];
        assert!(
            diff_footprint(&before, |_| Some(0)).is_empty(),
            "a zero-byte remnant is not worth offering"
        );
    }

    #[test]
    fn leftovers_are_ordered_largest_first() {
        let before = vec![
            ("C:\\A".to_string(), "Cache".to_string(), 1),
            ("C:\\B".to_string(), "User data".to_string(), 1),
        ];

        let leftovers = diff_footprint(&before, |path| {
            if path == "C:\\A" {
                Some(100)
            } else {
                Some(9_000)
            }
        });

        assert_eq!(leftovers[0].path, "C:\\B");
    }

    #[test]
    fn registry_leftovers_are_reported_but_never_removable() {
        let leftover = registry_leftover(
            "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\TestApp",
        );

        assert!(!leftover.removable);
        assert_eq!(leftover.bytes, 0);
        assert!(leftover.reason.contains("does not remove registry keys"));
    }

    #[test]
    fn every_leftover_explains_why_it_is_listed() {
        let before = vec![("C:\\A".to_string(), "Cache".to_string(), 1)];
        for leftover in diff_footprint(&before, |_| Some(500)) {
            assert!(leftover.reason.len() > 30);
        }
    }
}
