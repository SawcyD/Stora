use serde::{Deserialize, Serialize};

/// A kind of development project Stora can recognize.
///
/// Recognition is always driven by a marker file that the toolchain itself
/// creates — never by the presence of a conventionally named output folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectKind {
    Rust,
    Node,
    Python,
    DotNet,
    Java,
    Go,
    Unity,
    Unreal,
    Roblox,
    Unknown,
}

impl ProjectKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Node => "node",
            Self::Python => "python",
            Self::DotNet => "dotnet",
            Self::Java => "java",
            Self::Go => "go",
            Self::Unity => "unity",
            Self::Unreal => "unreal",
            Self::Roblox => "roblox",
            Self::Unknown => "unknown",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Node => "Node.js",
            Self::Python => "Python",
            Self::DotNet => ".NET",
            Self::Java => "Java",
            Self::Go => "Go",
            Self::Unity => "Unity",
            Self::Unreal => "Unreal Engine",
            Self::Roblox => "Roblox",
            Self::Unknown => "Unknown",
        }
    }
}

/// A file whose presence proves a directory is a project of a given kind.
struct Marker {
    /// Exact file name, compared case-insensitively.
    file_name: Option<&'static str>,
    /// Or an extension, for markers like `*.sln` and `*.uproject`.
    extension: Option<&'static str>,
    /// Or a directory name, for engine projects identified by structure.
    directory: Option<&'static str>,
    kind: ProjectKind,
}

const MARKERS: &[Marker] = &[
    Marker {
        file_name: Some("cargo.toml"),
        extension: None,
        directory: None,
        kind: ProjectKind::Rust,
    },
    Marker {
        file_name: Some("package.json"),
        extension: None,
        directory: None,
        kind: ProjectKind::Node,
    },
    Marker {
        file_name: Some("pyproject.toml"),
        extension: None,
        directory: None,
        kind: ProjectKind::Python,
    },
    Marker {
        file_name: Some("requirements.txt"),
        extension: None,
        directory: None,
        kind: ProjectKind::Python,
    },
    Marker {
        file_name: Some("setup.py"),
        extension: None,
        directory: None,
        kind: ProjectKind::Python,
    },
    Marker {
        file_name: Some("go.mod"),
        extension: None,
        directory: None,
        kind: ProjectKind::Go,
    },
    Marker {
        file_name: Some("build.gradle"),
        extension: None,
        directory: None,
        kind: ProjectKind::Java,
    },
    Marker {
        file_name: Some("pom.xml"),
        extension: None,
        directory: None,
        kind: ProjectKind::Java,
    },
    Marker {
        file_name: Some("default.project.json"),
        extension: None,
        directory: None,
        kind: ProjectKind::Roblox,
    },
    Marker {
        file_name: None,
        extension: Some("sln"),
        directory: None,
        kind: ProjectKind::DotNet,
    },
    Marker {
        file_name: None,
        extension: Some("csproj"),
        directory: None,
        kind: ProjectKind::DotNet,
    },
    Marker {
        file_name: None,
        extension: Some("uproject"),
        directory: None,
        kind: ProjectKind::Unreal,
    },
    Marker {
        file_name: None,
        extension: None,
        directory: Some("projectsettings"),
        kind: ProjectKind::Unity,
    },
];

/// Determines the project kinds a directory's own contents prove.
///
/// Takes the directory's immediate entry names so it can be unit tested
/// without touching the filesystem. Returns every kind matched, because a
/// single directory legitimately can be more than one (a Node front end beside
/// a Rust backend, for example).
pub fn kinds_from_entries(file_names: &[String], directory_names: &[String]) -> Vec<ProjectKind> {
    let mut kinds = Vec::new();

    for marker in MARKERS {
        let matched = if let Some(name) = marker.file_name {
            file_names
                .iter()
                .any(|entry| entry.eq_ignore_ascii_case(name))
        } else if let Some(extension) = marker.extension {
            file_names.iter().any(|entry| {
                stora_security::extension_of(entry)
                    .is_some_and(|found| found.eq_ignore_ascii_case(extension))
            })
        } else if let Some(directory) = marker.directory {
            directory_names
                .iter()
                .any(|entry| entry.eq_ignore_ascii_case(directory))
        } else {
            false
        };

        if matched && !kinds.contains(&marker.kind) {
            kinds.push(marker.kind);
        }
    }

    kinds
}

/// Reads a directory and reports which project kinds it proves.
pub fn detect(path: &str) -> Vec<ProjectKind> {
    let extended = stora_security::to_extended_length(path);
    let Ok(read) = std::fs::read_dir(&extended) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    let mut directories = Vec::new();

    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => directories.push(name),
            Ok(_) => files.push(name),
            Err(_) => {}
        }
    }

    kinds_from_entries(&files, &directories)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn recognizes_a_rust_project_by_its_manifest() {
        let kinds = kinds_from_entries(&names(&["Cargo.toml", "Cargo.lock"]), &names(&["src"]));
        assert_eq!(kinds, vec![ProjectKind::Rust]);
    }

    #[test]
    fn recognizes_a_node_project() {
        let kinds = kinds_from_entries(&names(&["package.json"]), &names(&["node_modules"]));
        assert_eq!(kinds, vec![ProjectKind::Node]);
    }

    #[test]
    fn recognizes_projects_identified_by_extension() {
        assert_eq!(
            kinds_from_entries(&names(&["Game.uproject"]), &[]),
            vec![ProjectKind::Unreal]
        );
        assert_eq!(
            kinds_from_entries(&names(&["App.sln"]), &[]),
            vec![ProjectKind::DotNet]
        );
    }

    #[test]
    fn recognizes_unity_by_directory_structure() {
        let kinds = kinds_from_entries(&[], &names(&["ProjectSettings", "Assets", "Library"]));
        assert_eq!(kinds, vec![ProjectKind::Unity]);
    }

    #[test]
    fn recognizes_roblox_projects() {
        assert_eq!(
            kinds_from_entries(&names(&["default.project.json"]), &[]),
            vec![ProjectKind::Roblox]
        );
    }

    #[test]
    fn a_build_folder_alone_proves_nothing() {
        // The central safety rule: never treat a conventionally named output
        // folder as evidence on its own.
        for folder in ["target", "build", "dist", "bin", "obj", "node_modules"] {
            let kinds = kinds_from_entries(&names(&["notes.txt"]), &names(&[folder]));
            assert!(
                kinds.is_empty(),
                "'{folder}' must not identify a project by itself"
            );
        }
    }

    #[test]
    fn detects_multiple_kinds_in_one_directory() {
        let kinds = kinds_from_entries(&names(&["Cargo.toml", "package.json"]), &[]);
        assert!(kinds.contains(&ProjectKind::Rust));
        assert!(kinds.contains(&ProjectKind::Node));
        assert_eq!(kinds.len(), 2);
    }

    #[test]
    fn marker_matching_is_case_insensitive() {
        assert_eq!(
            kinds_from_entries(&names(&["CARGO.TOML"]), &[]),
            vec![ProjectKind::Rust]
        );
        assert_eq!(
            kinds_from_entries(&[], &names(&["projectsettings"])),
            vec![ProjectKind::Unity]
        );
    }

    #[test]
    fn duplicate_markers_report_the_kind_once() {
        // Both `pyproject.toml` and `setup.py` mark Python.
        let kinds = kinds_from_entries(&names(&["pyproject.toml", "setup.py"]), &[]);
        assert_eq!(kinds, vec![ProjectKind::Python]);
    }

    #[test]
    fn detect_reads_a_real_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir(dir.path().join("target")).unwrap();

        let kinds = detect(&dir.path().to_string_lossy().replace('/', "\\"));
        assert_eq!(kinds, vec![ProjectKind::Rust]);
    }

    #[test]
    fn detect_returns_nothing_for_an_unreadable_path() {
        assert!(detect("C:\\definitely\\not\\here").is_empty());
    }
}
