use serde::{Deserialize, Serialize};

use stora_core::{Result, TaskControl};
use stora_security::ExclusionSet;

use crate::artifact::{self, SafetyLabel};
use crate::project::{self, ProjectKind};

/// How deep the project search descends below the chosen root.
const MAX_DEPTH: usize = 8;

/// Artifacts smaller than this are not worth showing.
const MIN_ARTIFACT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedArtifact {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub file_count: u64,
    pub label: String,
    pub label_text: String,
    pub explanation: String,
    pub cleanup_command: Option<String>,
    /// False for project source and user data, which are never removable.
    pub removable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProject {
    pub path: String,
    pub name: String,
    pub kinds: Vec<String>,
    pub kind_labels: Vec<String>,
    pub artifacts: Vec<DetectedArtifact>,
    /// Total of the artifacts that may be removed.
    pub reclaimable_bytes: u64,
    pub last_modified: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeveloperSummary {
    pub projects: Vec<DetectedProject>,
    /// Reclaimable bytes grouped by artifact name, largest first.
    pub totals_by_artifact: Vec<(String, u64)>,
    pub total_reclaimable: u64,
    pub projects_scanned: u64,
}

/// Finds development projects under `root` and the artifacts they generate.
///
/// A directory only counts as a project when a marker file proves it. Folders
/// that merely happen to be named `build` or `target` are ignored.
pub fn scan_projects(
    root: &str,
    exclusions: &ExclusionSet,
    control: &TaskControl,
) -> Result<DeveloperSummary> {
    let mut projects = Vec::new();
    let mut scanned = 0u64;

    walk(root, 0, exclusions, control, &mut projects, &mut scanned)?;

    // Largest opportunity first — that is the order a person reads in.
    projects.sort_by(|a, b| b.reclaimable_bytes.cmp(&a.reclaimable_bytes));

    let mut totals: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for project in &projects {
        for artifact in &project.artifacts {
            if artifact.removable {
                *totals.entry(artifact.name.clone()).or_default() += artifact.bytes;
            }
        }
    }

    let mut totals_by_artifact: Vec<(String, u64)> = totals.into_iter().collect();
    totals_by_artifact.sort_by(|a, b| b.1.cmp(&a.1));

    let total_reclaimable = projects.iter().map(|p| p.reclaimable_bytes).sum();

    Ok(DeveloperSummary {
        projects,
        totals_by_artifact,
        total_reclaimable,
        projects_scanned: scanned,
    })
}

fn walk(
    path: &str,
    depth: usize,
    exclusions: &ExclusionSet,
    control: &TaskControl,
    projects: &mut Vec<DetectedProject>,
    scanned: &mut u64,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    control.checkpoint()?;

    if exclusions.excludes_directory(path) {
        return Ok(());
    }

    let extended = stora_security::to_extended_length(path);
    let Ok(read) = std::fs::read_dir(&extended) else {
        return Ok(());
    };

    let mut file_names = Vec::new();
    let mut directory_names = Vec::new();

    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        match entry.file_type() {
            Ok(kind) if kind.is_symlink() => {}
            Ok(kind) if kind.is_dir() => directory_names.push(name),
            Ok(_) => file_names.push(name),
            Err(_) => {}
        }
    }

    let kinds = project::kinds_from_entries(&file_names, &directory_names);

    if !kinds.is_empty() {
        *scanned += 1;
        let project = describe_project(path, &kinds, &directory_names, control)?;

        // Descend past a project only to find nested ones, skipping the
        // artifact folders we already accounted for.
        let artifact_names: Vec<String> = project
            .artifacts
            .iter()
            .map(|a| a.name.to_ascii_lowercase())
            .collect();

        projects.push(project);

        for name in &directory_names {
            if artifact_names.contains(&name.to_ascii_lowercase()) {
                continue;
            }
            // Never walk into a dependency tree looking for more projects;
            // `node_modules` contains thousands of nested `package.json`
            // files that are not the user's projects.
            if is_never_descended(name) {
                continue;
            }
            walk(
                &join(path, name),
                depth + 1,
                exclusions,
                control,
                projects,
                scanned,
            )?;
        }
        return Ok(());
    }

    for name in &directory_names {
        if is_never_descended(name) {
            continue;
        }
        walk(
            &join(path, name),
            depth + 1,
            exclusions,
            control,
            projects,
            scanned,
        )?;
    }

    Ok(())
}

/// Directories that are never worth searching for projects.
fn is_never_descended(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "node_modules" | ".git" | "target" | ".venv" | "__pycache__" | "$recycle.bin"
    )
}

fn describe_project(
    path: &str,
    kinds: &[ProjectKind],
    directory_names: &[String],
    control: &TaskControl,
) -> Result<DetectedProject> {
    let mut artifacts = Vec::new();

    for name in directory_names {
        control.checkpoint()?;

        let Some(rule) = artifact::classify(name, kinds) else {
            continue;
        };

        let artifact_path = join(path, name);
        let (bytes, file_count) = directory_size(&artifact_path, control)?;

        if bytes < MIN_ARTIFACT_BYTES {
            continue;
        }

        artifacts.push(DetectedArtifact {
            path: artifact_path,
            name: name.clone(),
            bytes,
            file_count,
            label: rule.label.as_str().to_string(),
            label_text: rule.label.label().to_string(),
            explanation: rule.explanation.to_string(),
            cleanup_command: rule.cleanup_command.map(str::to_string),
            removable: rule.label.is_removable(),
        });
    }

    artifacts.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    let reclaimable_bytes = artifacts
        .iter()
        .filter(|a| a.removable)
        .map(|a| a.bytes)
        .sum();

    let last_modified = std::fs::metadata(stora_security::to_extended_length(path))
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    Ok(DetectedProject {
        path: path.to_string(),
        name: stora_security::file_name_of(path),
        kinds: kinds.iter().map(|k| k.as_str().to_string()).collect(),
        kind_labels: kinds.iter().map(|k| k.label().to_string()).collect(),
        artifacts,
        reclaimable_bytes,
        last_modified,
    })
}

/// Recursively totals a directory. Returns (bytes, file count).
pub fn directory_size(path: &str, control: &TaskControl) -> Result<(u64, u64)> {
    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut stack = vec![path.to_string()];

    while let Some(current) = stack.pop() {
        control.checkpoint()?;

        let extended = stora_security::to_extended_length(&current);
        let Ok(read) = std::fs::read_dir(&extended) else {
            continue;
        };

        for entry in read.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            // Links point elsewhere; following them would inflate the total.
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push(entry.path().to_string_lossy().replace('/', "\\"));
            } else {
                bytes += metadata.len();
                files += 1;
            }
        }
    }

    Ok((bytes, files))
}

/// Machine-wide package manager caches present on this system.
pub fn detect_package_caches(control: &TaskControl) -> Result<Vec<DetectedArtifact>> {
    let mut found = Vec::new();

    for cache in artifact::PACKAGE_CACHES {
        control.checkpoint()?;

        let Some(expanded) = stora_winapi::expand_environment(cache.pattern) else {
            continue;
        };
        let Ok(path) = stora_security::normalize(&expanded) else {
            continue;
        };
        if !std::path::Path::new(&path).exists() {
            continue;
        }

        let (bytes, file_count) = directory_size(&path, control)?;
        if bytes == 0 {
            continue;
        }

        found.push(DetectedArtifact {
            path,
            name: cache.name.to_string(),
            bytes,
            file_count,
            label: SafetyLabel::DependencyCache.as_str().to_string(),
            label_text: SafetyLabel::DependencyCache.label().to_string(),
            explanation: cache.explanation.to_string(),
            cleanup_command: cache.cleanup_command.map(str::to_string),
            removable: true,
        });
    }

    found.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    Ok(found)
}

fn join(parent: &str, child: &str) -> String {
    format!("{}\\{}", parent.trim_end_matches('\\'), child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const BIG: usize = 2 * 1024 * 1024;

    fn as_path(path: &std::path::Path) -> String {
        path.to_string_lossy().replace('/', "\\")
    }

    #[test]
    fn finds_a_rust_project_and_its_target_folder() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("app.exe"), vec![0u8; BIG]).unwrap();

        let summary = scan_projects(
            &as_path(dir.path()),
            &ExclusionSet::default(),
            &TaskControl::new(),
        )
        .unwrap();

        assert_eq!(summary.projects.len(), 1);
        let project = &summary.projects[0];
        assert_eq!(project.kinds, vec!["rust"]);
        assert_eq!(project.artifacts.len(), 1);
        assert_eq!(project.artifacts[0].name, "target");
        assert!(project.artifacts[0].removable);
        assert_eq!(project.reclaimable_bytes, BIG as u64);
    }

    #[test]
    fn a_target_folder_without_a_manifest_is_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("big.bin"), vec![0u8; BIG]).unwrap();
        fs::write(dir.path().join("notes.txt"), b"not a project").unwrap();

        let summary = scan_projects(
            &as_path(dir.path()),
            &ExclusionSet::default(),
            &TaskControl::new(),
        )
        .unwrap();

        assert!(
            summary.projects.is_empty(),
            "a folder named 'target' is not evidence of a Rust project"
        );
        assert_eq!(summary.total_reclaimable, 0);
    }

    #[test]
    fn unreal_saved_data_is_listed_but_not_reclaimable() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Game.uproject"), "{}").unwrap();

        let saved = dir.path().join("Saved");
        fs::create_dir(&saved).unwrap();
        fs::write(saved.join("autosave.umap"), vec![0u8; BIG]).unwrap();

        let intermediate = dir.path().join("Intermediate");
        fs::create_dir(&intermediate).unwrap();
        fs::write(intermediate.join("build.obj"), vec![0u8; BIG]).unwrap();

        let summary = scan_projects(
            &as_path(dir.path()),
            &ExclusionSet::default(),
            &TaskControl::new(),
        )
        .unwrap();

        let project = &summary.projects[0];
        assert_eq!(project.artifacts.len(), 2);

        let saved_artifact = project
            .artifacts
            .iter()
            .find(|a| a.name == "Saved")
            .expect("Saved is shown");
        assert!(
            !saved_artifact.removable,
            "autosaves must be visible but never reclaimable"
        );

        // Only Intermediate counts toward what can be freed.
        assert_eq!(project.reclaimable_bytes, BIG as u64);
    }

    #[test]
    fn finds_nested_projects_in_a_monorepo() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let api = dir.path().join("services").join("api");
        fs::create_dir_all(&api).unwrap();
        fs::write(api.join("Cargo.toml"), "[package]").unwrap();
        let target = api.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("api.exe"), vec![0u8; BIG]).unwrap();

        let summary = scan_projects(
            &as_path(dir.path()),
            &ExclusionSet::default(),
            &TaskControl::new(),
        )
        .unwrap();

        assert_eq!(summary.projects.len(), 2);
        assert!(summary.projects.iter().any(|p| p.kinds == vec!["rust"]));
        assert!(summary.projects.iter().any(|p| p.kinds == vec!["node"]));
    }

    #[test]
    fn does_not_search_inside_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        // A dependency with its own manifest must not be reported as one of
        // the user's projects.
        let dependency = dir.path().join("node_modules").join("react");
        fs::create_dir_all(&dependency).unwrap();
        fs::write(dependency.join("package.json"), "{}").unwrap();
        fs::write(dependency.join("index.js"), vec![0u8; BIG]).unwrap();

        let summary = scan_projects(
            &as_path(dir.path()),
            &ExclusionSet::default(),
            &TaskControl::new(),
        )
        .unwrap();

        assert_eq!(summary.projects.len(), 1);
        assert_eq!(summary.projects[0].artifacts[0].name, "node_modules");
    }

    #[test]
    fn small_artifacts_are_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("tiny.bin"), b"small").unwrap();

        let summary = scan_projects(
            &as_path(dir.path()),
            &ExclusionSet::default(),
            &TaskControl::new(),
        )
        .unwrap();

        assert!(summary.projects[0].artifacts.is_empty());
    }

    #[test]
    fn totals_are_grouped_by_artifact_name() {
        let dir = tempfile::tempdir().unwrap();

        for name in ["one", "two"] {
            let project = dir.path().join(name);
            fs::create_dir(&project).unwrap();
            fs::write(project.join("Cargo.toml"), "[package]").unwrap();
            let target = project.join("target");
            fs::create_dir(&target).unwrap();
            fs::write(target.join("out.bin"), vec![0u8; BIG]).unwrap();
        }

        let summary = scan_projects(
            &as_path(dir.path()),
            &ExclusionSet::default(),
            &TaskControl::new(),
        )
        .unwrap();

        assert_eq!(summary.projects.len(), 2);
        assert_eq!(summary.totals_by_artifact.len(), 1);
        assert_eq!(summary.totals_by_artifact[0].0, "target");
        assert_eq!(summary.totals_by_artifact[0].1, 2 * BIG as u64);
        assert_eq!(summary.total_reclaimable, 2 * BIG as u64);
    }

    #[test]
    fn respects_exclusions() {
        use stora_core::model::{Exclusion, ExclusionKind, ExclusionReason};

        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("private");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("Cargo.toml"), "[package]").unwrap();
        let target = project.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("out.bin"), vec![0u8; BIG]).unwrap();

        let exclusions = ExclusionSet::from_rules(&[Exclusion {
            id: 0,
            pattern: as_path(&project),
            kind: ExclusionKind::Folder,
            reason: ExclusionReason::UserExclusion,
            created_at: 0,
        }]);

        let summary =
            scan_projects(&as_path(dir.path()), &exclusions, &TaskControl::new()).unwrap();
        assert!(summary.projects.is_empty());
    }

    #[test]
    fn scanning_is_cancellable() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let control = TaskControl::new();
        control.cancel();

        let err =
            scan_projects(&as_path(dir.path()), &ExclusionSet::default(), &control).unwrap_err();
        assert_eq!(err.code(), "ScanCancelled");
    }

    #[test]
    fn directory_size_totals_nested_content() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("a").join("b");
        fs::create_dir_all(&inner).unwrap();
        fs::write(dir.path().join("top.bin"), vec![0u8; 100]).unwrap();
        fs::write(inner.join("deep.bin"), vec![0u8; 250]).unwrap();

        let (bytes, files) = directory_size(&as_path(dir.path()), &TaskControl::new()).unwrap();
        assert_eq!(bytes, 350);
        assert_eq!(files, 2);
    }

    #[test]
    fn directory_size_of_a_missing_path_is_zero() {
        let (bytes, files) =
            directory_size("C:\\definitely\\missing", &TaskControl::new()).unwrap();
        assert_eq!(bytes, 0);
        assert_eq!(files, 0);
    }
}
