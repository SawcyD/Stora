use stora_core::{Result, TaskControl};

use crate::model::{AppFootprint, Confidence, FootprintLocation, InstalledApp};

/// Candidate roots that may hold data belonging to an application.
const DATA_ROOTS: &[(&str, &str)] = &[
    ("%LOCALAPPDATA%", "User data"),
    ("%APPDATA%", "User data"),
    ("%PROGRAMDATA%", "Shared application data"),
];

/// Folder names inside an application's data directory and what they hold.
const SUBFOLDER_ROLES: &[(&str, &str)] = &[
    ("cache", "Cache"),
    ("code cache", "Cache"),
    ("gpucache", "Cache"),
    ("logs", "Logs"),
    ("crashpad", "Crash reports"),
    ("crashdumps", "Crash reports"),
    ("extensions", "Extensions"),
    ("user data", "User data"),
];

/// Builds an application's storage footprint from observable evidence.
///
/// Every location carries the reason it was attributed. A folder is only
/// linked to an application when its name matches distinctly enough to be
/// meaningful — a shared directory is never assigned to one application on a
/// weak name match alone.
pub fn build(app: &InstalledApp, control: &TaskControl) -> Result<AppFootprint> {
    let mut locations = Vec::new();

    // The install directory is the strongest evidence there is: the registry
    // entry names it directly.
    if let Some(install) = &app.install_location {
        control.checkpoint()?;
        if std::path::Path::new(install).exists() {
            let (bytes, _) = stora_developer::directory_size(install, control)?;
            locations.push(FootprintLocation {
                path: install.clone(),
                relationship: "Application files".into(),
                bytes,
                confidence: Confidence::High,
                confidence_label: Confidence::High.label().to_string(),
                reason: "The application's own uninstall entry names this folder as its \
                         install location."
                    .into(),
            });
        }
    }

    for (pattern, role) in DATA_ROOTS {
        control.checkpoint()?;

        let Some(expanded) = stora_winapi::expand_environment(pattern) else {
            continue;
        };
        let Ok(root) = stora_security::normalize(&expanded) else {
            continue;
        };

        for candidate in match_data_folders(&root, app)? {
            control.checkpoint()?;
            let (bytes, _) = stora_developer::directory_size(&candidate.path, control)?;
            if bytes == 0 {
                continue;
            }
            locations.push(FootprintLocation {
                path: candidate.path,
                relationship: candidate.role.unwrap_or_else(|| (*role).to_string()),
                bytes,
                confidence: candidate.confidence,
                confidence_label: candidate.confidence.label().to_string(),
                reason: candidate.reason,
            });
        }
    }

    locations.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let total_bytes = locations.iter().map(|l| l.bytes).sum();

    Ok(AppFootprint {
        app_id: app.id.clone(),
        locations,
        total_bytes,
    })
}

struct Candidate {
    path: String,
    role: Option<String>,
    confidence: Confidence,
    reason: String,
}

/// Finds folders under a data root that plausibly belong to `app`.
fn match_data_folders(root: &str, app: &InstalledApp) -> Result<Vec<Candidate>> {
    let extended = stora_security::to_extended_length(root);
    let Ok(read) = std::fs::read_dir(&extended) else {
        return Ok(Vec::new());
    };

    let mut found = Vec::new();

    for entry in read.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }

        let folder = entry.file_name().to_string_lossy().to_string();
        let path = entry.path().to_string_lossy().replace('/', "\\");

        let Some((confidence, reason)) = attribution(&folder, app) else {
            continue;
        };

        found.push(Candidate {
            path,
            role: None,
            confidence,
            reason,
        });
    }

    Ok(found)
}

/// Decides whether a data folder belongs to an application, and how sure we
/// are.
///
/// Returns `None` rather than guessing. A generic folder name that merely
/// contains a common word is not evidence.
pub fn attribution(folder_name: &str, app: &InstalledApp) -> Option<(Confidence, String)> {
    let folder = folder_name.trim();
    if folder.is_empty() {
        return None;
    }

    let folder_lower = folder.to_ascii_lowercase();
    let name_lower = app.name.trim().to_ascii_lowercase();
    let publisher_lower = app.publisher.trim().to_ascii_lowercase();

    if name_lower.is_empty() {
        return None;
    }

    // An exact match on the application name is strong evidence.
    if folder_lower == name_lower {
        return Some((
            Confidence::High,
            format!("The folder name matches the application name exactly ({folder})."),
        ));
    }

    // Publisher folder holding the application, e.g. `Contoso`.
    if !publisher_lower.is_empty() && folder_lower == publisher_lower {
        return Some((
            Confidence::Medium,
            format!(
                "The folder is named after the publisher ({folder}). It may hold data for \
                 more than one of their applications."
            ),
        ));
    }

    // A compacted form, e.g. "Visual Studio Code" -> "VisualStudioCode".
    let compact_name: String = name_lower.chars().filter(|c| !c.is_whitespace()).collect();
    let compact_folder: String = folder_lower
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if compact_folder == compact_name {
        return Some((
            Confidence::High,
            format!("The folder name matches the application name ({folder})."),
        ));
    }

    // Require a distinctive name before accepting a partial match. Short or
    // common names produce false links — a folder called "Code" could belong
    // to anything.
    if compact_name.len() >= 8 && compact_folder.contains(&compact_name) {
        return Some((
            Confidence::Medium,
            format!("The folder name contains the application name ({folder})."),
        ));
    }

    None
}

/// Labels a subfolder inside an application's data directory.
pub fn subfolder_role(folder_name: &str) -> Option<&'static str> {
    let lowered = folder_name.to_ascii_lowercase();
    SUBFOLDER_ROLES
        .iter()
        .find(|(name, _)| *name == lowered)
        .map(|(_, role)| *role)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AppType;

    fn app(name: &str, publisher: &str) -> InstalledApp {
        InstalledApp {
            id: "machine:test".into(),
            name: name.into(),
            publisher: publisher.into(),
            version: "1.0".into(),
            reported_bytes: None,
            detected_bytes: None,
            install_location: None,
            install_date: None,
            app_type: AppType::DesktopApplication,
            app_type_label: "Desktop application".into(),
            uninstall_command: None,
            source: "test".into(),
            confidence: Confidence::High,
            confidence_label: "High".into(),
            suggestable: true,
        }
    }

    #[test]
    fn an_exact_name_match_is_high_confidence() {
        let (confidence, reason) =
            attribution("Obsidian", &app("Obsidian", "Dynalist")).expect("matched");
        assert_eq!(confidence, Confidence::High);
        assert!(reason.contains("exactly"));
    }

    #[test]
    fn a_whitespace_insensitive_match_is_high_confidence() {
        let (confidence, _) =
            attribution("VisualStudioCode", &app("Visual Studio Code", "Microsoft"))
                .expect("matched");
        assert_eq!(confidence, Confidence::High);
    }

    #[test]
    fn a_publisher_folder_is_only_medium_confidence() {
        let (confidence, reason) =
            attribution("JetBrains", &app("IntelliJ IDEA", "JetBrains")).expect("matched");
        assert_eq!(confidence, Confidence::Medium);
        assert!(
            reason.contains("more than one"),
            "a shared publisher folder must say so"
        );
    }

    #[test]
    fn short_names_do_not_produce_partial_matches() {
        // "Code" is far too common to attribute a folder on.
        assert!(attribution("VSCodeBackups", &app("Code", "Microsoft")).is_none());
        assert!(attribution("MyGitRepos", &app("Git", "Git")).is_none());
    }

    #[test]
    fn a_distinctive_partial_match_is_medium_confidence() {
        let (confidence, _) =
            attribution("ObsidianVaultCache", &app("Obsidian", "Dynalist")).expect("matched");
        assert_eq!(confidence, Confidence::Medium);
    }

    #[test]
    fn unrelated_folders_are_not_attributed() {
        assert!(attribution("Temp", &app("Obsidian", "Dynalist")).is_none());
        assert!(attribution("Microsoft", &app("Obsidian", "Dynalist")).is_none());
        assert!(attribution("", &app("Obsidian", "Dynalist")).is_none());
    }

    #[test]
    fn an_app_without_a_name_matches_nothing() {
        assert!(attribution("Anything", &app("", "")).is_none());
    }

    #[test]
    fn matching_is_case_insensitive() {
        let (confidence, _) =
            attribution("OBSIDIAN", &app("obsidian", "Dynalist")).expect("matched");
        assert_eq!(confidence, Confidence::High);
    }

    #[test]
    fn subfolder_roles_are_recognized() {
        assert_eq!(subfolder_role("Cache"), Some("Cache"));
        assert_eq!(subfolder_role("logs"), Some("Logs"));
        assert_eq!(subfolder_role("Crashpad"), Some("Crash reports"));
        assert_eq!(subfolder_role("Extensions"), Some("Extensions"));
        assert_eq!(subfolder_role("something-else"), None);
    }

    #[test]
    fn the_install_location_is_the_strongest_evidence() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("app.exe"), vec![0u8; 500]).unwrap();

        let mut installed = app("TestApp", "Contoso");
        installed.install_location = Some(dir.path().to_string_lossy().replace('/', "\\"));

        let footprint = build(&installed, &TaskControl::new()).unwrap();
        let install = footprint
            .locations
            .iter()
            .find(|l| l.relationship == "Application files")
            .expect("install location listed");

        assert_eq!(install.confidence, Confidence::High);
        assert_eq!(install.bytes, 500);
        assert!(install.reason.contains("uninstall entry"));
    }

    #[test]
    fn a_missing_install_location_is_simply_omitted() {
        let mut installed = app("TestApp", "Contoso");
        installed.install_location = Some("C:\\definitely\\missing".into());

        let footprint = build(&installed, &TaskControl::new()).unwrap();
        assert!(footprint
            .locations
            .iter()
            .all(|l| l.relationship != "Application files"));
    }

    #[test]
    fn footprint_building_is_cancellable() {
        let control = TaskControl::new();
        control.cancel();

        let mut installed = app("TestApp", "Contoso");
        installed.install_location = Some("C:\\Program Files".into());

        assert!(build(&installed, &control).is_err());
    }

    #[test]
    fn every_location_states_its_reason() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("app.exe"), b"x").unwrap();

        let mut installed = app("TestApp", "Contoso");
        installed.install_location = Some(dir.path().to_string_lossy().replace('/', "\\"));

        let footprint = build(&installed, &TaskControl::new()).unwrap();
        for location in &footprint.locations {
            assert!(
                location.reason.len() > 20,
                "every attribution must be explainable"
            );
        }
    }
}
