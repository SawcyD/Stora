//! Detection and classification of development storage.
//!
//! The governing rule of this crate: a folder is never treated as a build
//! artifact because of its name. A directory only counts as a project when a
//! marker file the toolchain itself creates proves it, and only then are its
//! conventionally named output folders classified.

pub mod artifact;
pub mod project;
pub mod scan;
pub mod virtual_disk;

pub use artifact::{classify, SafetyLabel, PACKAGE_CACHES, RULES};
pub use project::{detect as detect_project, ProjectKind};
pub use scan::{
    detect_package_caches, directory_size, scan_projects, DetectedArtifact, DetectedProject,
    DeveloperSummary,
};
pub use virtual_disk::{detect as detect_virtual_disks, VirtualDisk, VirtualDiskKind};
