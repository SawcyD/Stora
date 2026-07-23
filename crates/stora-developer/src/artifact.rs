use serde::{Deserialize, Serialize};

use crate::project::ProjectKind;

/// How safe a detected artifact is to remove.
///
/// The distinction between `Regeneratable` and `UsuallyRegeneratable` is not
/// cosmetic: the first can always be rebuilt from the project itself, while
/// the second may require network access or a toolchain that is no longer
/// installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SafetyLabel {
    /// Rebuilt automatically from files already on disk.
    Regeneratable,
    /// Usually rebuilt, but may need a download or a specific toolchain.
    UsuallyRegeneratable,
    /// Compiler or bundler output.
    BuildOutput,
    /// Downloaded dependencies.
    DependencyCache,
    /// Files the developer wrote. Never offered for removal.
    ProjectSource,
    /// Content a person created. Never offered for removal.
    UserCreatedData,
    Unknown,
}

impl SafetyLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Regeneratable => "regeneratable",
            Self::UsuallyRegeneratable => "usuallyRegeneratable",
            Self::BuildOutput => "buildOutput",
            Self::DependencyCache => "dependencyCache",
            Self::ProjectSource => "projectSource",
            Self::UserCreatedData => "userCreatedData",
            Self::Unknown => "unknown",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Regeneratable => "Regeneratable",
            Self::UsuallyRegeneratable => "Usually regeneratable",
            Self::BuildOutput => "Build output",
            Self::DependencyCache => "Dependency cache",
            Self::ProjectSource => "Project source",
            Self::UserCreatedData => "User-created data",
            Self::Unknown => "Unknown",
        }
    }

    /// Whether Stora may ever suggest removing this.
    ///
    /// Source and user data are excluded unconditionally — no setting turns
    /// this on.
    pub fn is_removable(&self) -> bool {
        matches!(
            self,
            Self::Regeneratable
                | Self::UsuallyRegeneratable
                | Self::BuildOutput
                | Self::DependencyCache
        )
    }
}

/// A directory that a given project kind is known to generate.
pub struct ArtifactRule {
    /// Directory name, compared case-insensitively.
    pub directory: &'static str,
    /// Which project kinds legitimately produce it.
    pub kinds: &'static [ProjectKind],
    pub label: SafetyLabel,
    /// Shown to the user as the reason this can be removed.
    pub explanation: &'static str,
    /// The official command that regenerates or clears it, when one exists.
    pub cleanup_command: Option<&'static str>,
}

pub const RULES: &[ArtifactRule] = &[
    ArtifactRule {
        directory: "target",
        kinds: &[ProjectKind::Rust],
        label: SafetyLabel::BuildOutput,
        explanation: "Cargo build output. Rebuilt by the next `cargo build`.",
        cleanup_command: Some("cargo clean"),
    },
    ArtifactRule {
        directory: "node_modules",
        kinds: &[ProjectKind::Node],
        label: SafetyLabel::DependencyCache,
        explanation:
            "Installed npm packages. Restored by reinstalling, which needs network access.",
        cleanup_command: Some("npm install"),
    },
    ArtifactRule {
        directory: ".next",
        kinds: &[ProjectKind::Node],
        label: SafetyLabel::Regeneratable,
        explanation: "Next.js build cache. Recreated on the next build.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: ".turbo",
        kinds: &[ProjectKind::Node],
        label: SafetyLabel::Regeneratable,
        explanation: "Turborepo task cache. Recreated on the next run.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: "dist",
        kinds: &[ProjectKind::Node, ProjectKind::Python],
        label: SafetyLabel::BuildOutput,
        explanation: "Bundler output. Recreated by the next build.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: "build",
        kinds: &[ProjectKind::Node, ProjectKind::Python, ProjectKind::Java],
        label: SafetyLabel::BuildOutput,
        explanation: "Build output directory. Recreated by the next build.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: "__pycache__",
        kinds: &[ProjectKind::Python],
        label: SafetyLabel::Regeneratable,
        explanation: "Compiled Python bytecode. Regenerated automatically on import.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: ".venv",
        kinds: &[ProjectKind::Python],
        label: SafetyLabel::DependencyCache,
        explanation:
            "Python virtual environment. Recreating it requires reinstalling dependencies.",
        cleanup_command: Some("python -m venv .venv"),
    },
    ArtifactRule {
        directory: ".pytest_cache",
        kinds: &[ProjectKind::Python],
        label: SafetyLabel::Regeneratable,
        explanation: "pytest run cache. Recreated on the next test run.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: "bin",
        kinds: &[ProjectKind::DotNet],
        label: SafetyLabel::BuildOutput,
        explanation: ".NET compiled output. Recreated by the next build.",
        cleanup_command: Some("dotnet clean"),
    },
    ArtifactRule {
        directory: "obj",
        kinds: &[ProjectKind::DotNet],
        label: SafetyLabel::BuildOutput,
        explanation: ".NET intermediate output. Recreated by the next build.",
        cleanup_command: Some("dotnet clean"),
    },
    ArtifactRule {
        directory: ".gradle",
        kinds: &[ProjectKind::Java],
        label: SafetyLabel::Regeneratable,
        explanation: "Gradle project cache. Recreated on the next build.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: "library",
        kinds: &[ProjectKind::Unity],
        label: SafetyLabel::UsuallyRegeneratable,
        explanation:
            "Unity's imported asset database. Unity rebuilds it on next open, which can take \
             a long time for a large project.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: "deriveddatacache",
        kinds: &[ProjectKind::Unreal],
        label: SafetyLabel::UsuallyRegeneratable,
        explanation:
            "Unreal derived data cache. Rebuilt on demand, which can be slow the first time.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: "intermediate",
        kinds: &[ProjectKind::Unreal],
        label: SafetyLabel::BuildOutput,
        explanation: "Unreal intermediate build files. Recreated by the next build.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: "saved",
        kinds: &[ProjectKind::Unreal],
        label: SafetyLabel::UserCreatedData,
        explanation:
            "Unreal saved data, including autosaves and logs. May contain work that exists \
             nowhere else.",
        cleanup_command: None,
    },
    ArtifactRule {
        directory: "coverage",
        kinds: &[ProjectKind::Node, ProjectKind::Python],
        label: SafetyLabel::Regeneratable,
        explanation: "Test coverage output. Recreated by the next test run.",
        cleanup_command: None,
    },
];

/// A machine-wide package manager cache, independent of any single project.
pub struct PackageCache {
    pub id: &'static str,
    pub name: &'static str,
    /// `%VAR%`-style path, expanded at detection time.
    pub pattern: &'static str,
    pub explanation: &'static str,
    /// The official command, shown to the user before anything runs.
    pub cleanup_command: Option<&'static str>,
}

pub const PACKAGE_CACHES: &[PackageCache] = &[
    PackageCache {
        id: "npmCache",
        name: "npm cache",
        pattern: "%LOCALAPPDATA%\\npm-cache",
        explanation: "Downloaded npm packages. npm refetches anything it needs.",
        cleanup_command: Some("npm cache clean --force"),
    },
    PackageCache {
        id: "pnpmStore",
        name: "pnpm store",
        pattern: "%LOCALAPPDATA%\\pnpm\\store",
        explanation: "pnpm's content-addressable package store, shared across projects.",
        cleanup_command: Some("pnpm store prune"),
    },
    PackageCache {
        id: "cargoRegistry",
        name: "Cargo registry cache",
        pattern: "%USERPROFILE%\\.cargo\\registry\\cache",
        explanation: "Downloaded crate archives. Cargo refetches them when needed.",
        cleanup_command: None,
    },
    PackageCache {
        id: "nugetCache",
        name: "NuGet cache",
        pattern: "%USERPROFILE%\\.nuget\\packages",
        explanation: "Downloaded NuGet packages, shared across .NET projects.",
        cleanup_command: Some("dotnet nuget locals all --clear"),
    },
    PackageCache {
        id: "pipCache",
        name: "pip cache",
        pattern: "%LOCALAPPDATA%\\pip\\Cache",
        explanation: "Downloaded Python wheels. pip refetches them when needed.",
        cleanup_command: Some("pip cache purge"),
    },
    PackageCache {
        id: "gradleCache",
        name: "Gradle cache",
        pattern: "%USERPROFILE%\\.gradle\\caches",
        explanation: "Gradle's dependency and build cache.",
        cleanup_command: None,
    },
];

/// Classifies a directory found inside a project of the given kinds.
///
/// Returns `None` when no rule applies — the caller must then leave the folder
/// alone. A directory named `build` inside a project that has no rule for it
/// is not an artifact, it is just a folder someone named `build`.
pub fn classify<'a>(
    directory_name: &str,
    project_kinds: &'a [ProjectKind],
) -> Option<&'a ArtifactRule>
where
    'static: 'a,
{
    let lowered = directory_name.to_ascii_lowercase();

    RULES.iter().find(|rule| {
        rule.directory == lowered && rule.kinds.iter().any(|kind| project_kinds.contains(kind))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_a_cargo_target_folder() {
        let rule = classify("target", &[ProjectKind::Rust]).expect("matched");
        assert_eq!(rule.label, SafetyLabel::BuildOutput);
        assert_eq!(rule.cleanup_command, Some("cargo clean"));
    }

    #[test]
    fn refuses_to_classify_target_outside_a_rust_project() {
        // The core safety rule: the folder name alone means nothing.
        assert!(classify("target", &[ProjectKind::Node]).is_none());
        assert!(classify("target", &[]).is_none());
    }

    #[test]
    fn refuses_to_classify_build_without_a_matching_project() {
        assert!(classify("build", &[ProjectKind::Rust]).is_none());
        assert!(classify("dist", &[ProjectKind::Unity]).is_none());
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert!(classify("NODE_MODULES", &[ProjectKind::Node]).is_some());
        assert!(classify("Library", &[ProjectKind::Unity]).is_some());
    }

    #[test]
    fn unity_library_is_only_usually_regeneratable() {
        let rule = classify("Library", &[ProjectKind::Unity]).unwrap();
        assert_eq!(rule.label, SafetyLabel::UsuallyRegeneratable);
        assert!(
            rule.explanation.contains("long time"),
            "the cost of rebuilding must be stated"
        );
    }

    #[test]
    fn unreal_saved_data_is_never_removable() {
        let rule = classify("Saved", &[ProjectKind::Unreal]).unwrap();
        assert_eq!(rule.label, SafetyLabel::UserCreatedData);
        assert!(
            !rule.label.is_removable(),
            "autosaves must never be offered for removal"
        );
    }

    #[test]
    fn source_and_user_data_are_never_removable() {
        assert!(!SafetyLabel::ProjectSource.is_removable());
        assert!(!SafetyLabel::UserCreatedData.is_removable());
        assert!(!SafetyLabel::Unknown.is_removable());
    }

    #[test]
    fn build_and_cache_labels_are_removable() {
        assert!(SafetyLabel::Regeneratable.is_removable());
        assert!(SafetyLabel::UsuallyRegeneratable.is_removable());
        assert!(SafetyLabel::BuildOutput.is_removable());
        assert!(SafetyLabel::DependencyCache.is_removable());
    }

    #[test]
    fn shared_directory_names_resolve_per_project_kind() {
        // `build` is claimed by several ecosystems but not by Rust.
        assert!(classify("build", &[ProjectKind::Node]).is_some());
        assert!(classify("build", &[ProjectKind::Java]).is_some());
        assert!(classify("build", &[ProjectKind::Rust]).is_none());
    }

    #[test]
    fn every_rule_has_a_usable_explanation() {
        for rule in RULES {
            assert!(
                rule.explanation.len() > 20,
                "rule '{}' needs a real explanation",
                rule.directory
            );
            assert_eq!(
                rule.directory.to_ascii_lowercase(),
                rule.directory,
                "rule '{}' must be stored lowercase for matching",
                rule.directory
            );
            assert!(
                !rule.kinds.is_empty(),
                "rule '{}' must name the project kinds that produce it",
                rule.directory
            );
        }
    }

    #[test]
    fn package_caches_are_well_formed() {
        for cache in PACKAGE_CACHES {
            assert!(cache.pattern.contains('%'), "{} needs a variable", cache.id);
            assert!(cache.explanation.len() > 20, "{} needs a reason", cache.id);
        }
    }
}
