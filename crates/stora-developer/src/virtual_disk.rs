use serde::{Deserialize, Serialize};

use stora_core::{Result, TaskControl};

/// Virtual disk extensions Stora recognizes.
const DISK_EXTENSIONS: &[&str] = &["vhdx", "vhd", "vmdk", "vdi", "qcow2"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualDiskKind {
    Wsl,
    DockerDesktop,
    HyperV,
    VmWare,
    VirtualBox,
    Unknown,
}

impl VirtualDiskKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Wsl => "WSL distribution",
            Self::DockerDesktop => "Docker Desktop",
            Self::HyperV => "Hyper-V",
            Self::VmWare => "VMware",
            Self::VirtualBox => "VirtualBox",
            Self::Unknown => "Virtual disk",
        }
    }

    /// The supported way to reclaim space, stated as guidance.
    ///
    /// Stora never deletes a virtual disk. A large `.vhdx` is usually a
    /// working machine, and its size on disk does not shrink automatically
    /// even after data is deleted inside it.
    pub fn guidance(&self) -> &'static str {
        match self {
            Self::Wsl => {
                "A WSL disk does not shrink when files inside it are deleted. Shut WSL down \
                 with `wsl --shutdown`, then compact the disk with \
                 `Optimize-VHD -Path <disk> -Mode Full` in an elevated PowerShell."
            }
            Self::DockerDesktop => {
                "Reclaim space from inside Docker first with `docker system prune`, then use \
                 Docker Desktop's \"Clean / Purge data\" option. Deleting the disk file \
                 removes every image, container, and volume."
            }
            Self::HyperV => {
                "Shut the virtual machine down, then compact the disk with \
                 `Optimize-VHD -Path <disk> -Mode Full` in an elevated PowerShell."
            }
            Self::VmWare => {
                "Use the virtual machine's own settings to compact the disk. Deleting the \
                 file destroys the machine."
            }
            Self::VirtualBox => {
                "Compact with `VBoxManage modifymedium disk <disk> --compact`. Deleting the \
                 file destroys the machine."
            }
            Self::Unknown => {
                "This appears to be a virtual disk. Use its own tooling to compact it rather \
                 than deleting the file."
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDisk {
    pub path: String,
    pub name: String,
    /// Name of the distribution or machine, when it can be inferred.
    pub owner: String,
    pub kind: String,
    pub kind_label: String,
    /// Size the file occupies on disk.
    pub bytes: u64,
    pub last_modified: Option<i64>,
    pub guidance: String,
    /// Always false. Stora does not delete virtual disks.
    pub removable: bool,
}

/// Locations where virtual disks commonly live.
const SEARCH_PATTERNS: &[(&str, VirtualDiskKind)] = &[
    ("%LOCALAPPDATA%\\Packages", VirtualDiskKind::Wsl),
    ("%LOCALAPPDATA%\\wsl", VirtualDiskKind::Wsl),
    (
        "%LOCALAPPDATA%\\Docker\\wsl",
        VirtualDiskKind::DockerDesktop,
    ),
    (
        "%USERPROFILE%\\AppData\\Local\\Docker\\wsl",
        VirtualDiskKind::DockerDesktop,
    ),
    ("%USERPROFILE%\\.docker", VirtualDiskKind::DockerDesktop),
    ("%USERPROFILE%\\VirtualBox VMs", VirtualDiskKind::VirtualBox),
    (
        "%USERPROFILE%\\Documents\\Virtual Machines",
        VirtualDiskKind::VmWare,
    ),
    ("%PUBLIC%\\Documents\\Hyper-V", VirtualDiskKind::HyperV),
];

/// How deep to look inside each known location.
const MAX_DEPTH: usize = 4;

/// Finds virtual disks in the usual locations.
///
/// This is intentionally a targeted search rather than a whole-drive sweep:
/// scanning every volume for multi-gigabyte files would be slow and would
/// surface disks belonging to software Stora knows nothing about.
pub fn detect(control: &TaskControl) -> Result<Vec<VirtualDisk>> {
    let mut found: Vec<VirtualDisk> = Vec::new();

    for (pattern, kind) in SEARCH_PATTERNS {
        control.checkpoint()?;

        let Some(expanded) = stora_winapi::expand_environment(pattern) else {
            continue;
        };
        let Ok(root) = stora_security::normalize(&expanded) else {
            continue;
        };
        if !std::path::Path::new(&root).exists() {
            continue;
        }

        collect(&root, *kind, 0, control, &mut found)?;
    }

    // The same disk can sit under two overlapping patterns.
    found.sort_by(|a, b| {
        a.path
            .to_ascii_lowercase()
            .cmp(&b.path.to_ascii_lowercase())
    });
    found.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));

    found.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    Ok(found)
}

fn collect(
    path: &str,
    kind: VirtualDiskKind,
    depth: usize,
    control: &TaskControl,
    found: &mut Vec<VirtualDisk>,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    control.checkpoint()?;

    let extended = stora_security::to_extended_length(path);
    let Ok(read) = std::fs::read_dir(&extended) else {
        return Ok(());
    };

    for entry in read.flatten() {
        let entry_path = entry.path().to_string_lossy().replace('/', "\\");
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }

        if metadata.is_dir() {
            collect(&entry_path, kind, depth + 1, control, found)?;
            continue;
        }

        let Some(extension) = stora_security::extension_of(&entry_path) else {
            continue;
        };
        if !DISK_EXTENSIONS.contains(&extension.as_str()) {
            continue;
        }

        found.push(VirtualDisk {
            name: stora_security::file_name_of(&entry_path),
            owner: infer_owner(&entry_path, kind),
            kind: format!("{kind:?}").to_lowercase(),
            kind_label: kind.label().to_string(),
            bytes: metadata.len(),
            last_modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            guidance: kind.guidance().to_string(),
            // Never removable. This is the whole point of the feature.
            removable: false,
            path: entry_path,
        });
    }

    Ok(())
}

/// Best-effort name for the distribution or machine that owns a disk.
///
/// WSL stores its disks under a package folder whose name encodes the
/// distribution, e.g. `CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc`.
pub fn infer_owner(path: &str, kind: VirtualDiskKind) -> String {
    let parent = stora_security::parent_of(path).unwrap_or_default();
    let folder = stora_security::file_name_of(&parent);

    match kind {
        VirtualDiskKind::Wsl => {
            // Strip the publisher prefix and the package hash suffix.
            let without_hash = folder
                .rsplit_once('_')
                .map(|(head, _)| head)
                .unwrap_or(&folder);
            // Split at the *first* dot: everything before it is the
            // publisher, everything after is the distribution — which itself
            // contains dots, as in `Ubuntu24.04LTS`.
            let distribution = without_hash
                .split_once('.')
                .map(|(_, tail)| tail)
                .unwrap_or(without_hash);

            if distribution.is_empty() {
                folder
            } else {
                distribution.to_string()
            }
        }
        VirtualDiskKind::DockerDesktop => "Docker Desktop".to_string(),
        _ => {
            if folder.is_empty() {
                stora_security::file_name_of(path)
            } else {
                folder
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_disks_are_never_removable() {
        // The defining safety property of this feature.
        let dir = tempfile::tempdir().unwrap();
        let disk = dir.path().join("ext4.vhdx");
        std::fs::write(&disk, vec![0u8; 4096]).unwrap();

        let mut found = Vec::new();
        collect(
            &dir.path().to_string_lossy().replace('/', "\\"),
            VirtualDiskKind::Wsl,
            0,
            &TaskControl::new(),
            &mut found,
        )
        .unwrap();

        assert_eq!(found.len(), 1);
        assert!(!found[0].removable);
        assert_eq!(found[0].bytes, 4096);
    }

    #[test]
    fn recognizes_every_supported_disk_extension() {
        let dir = tempfile::tempdir().unwrap();
        for extension in DISK_EXTENSIONS {
            std::fs::write(dir.path().join(format!("machine.{extension}")), b"x").unwrap();
        }
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();

        let mut found = Vec::new();
        collect(
            &dir.path().to_string_lossy().replace('/', "\\"),
            VirtualDiskKind::Unknown,
            0,
            &TaskControl::new(),
            &mut found,
        )
        .unwrap();

        assert_eq!(found.len(), DISK_EXTENSIONS.len());
        assert!(!found.iter().any(|d| d.name.ends_with(".txt")));
    }

    #[test]
    fn infers_a_wsl_distribution_name() {
        let path = "C:\\Users\\Test\\AppData\\Local\\Packages\\\
                    CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc\\ext4.vhdx";
        assert_eq!(infer_owner(path, VirtualDiskKind::Wsl), "Ubuntu24.04LTS");
    }

    #[test]
    fn owner_inference_survives_an_unexpected_folder_shape() {
        let path = "C:\\Disks\\ext4.vhdx";
        // No publisher prefix and no hash suffix — fall back to the folder.
        assert_eq!(infer_owner(path, VirtualDiskKind::Wsl), "Disks");
    }

    #[test]
    fn every_kind_offers_supported_guidance_not_deletion() {
        for kind in [
            VirtualDiskKind::Wsl,
            VirtualDiskKind::DockerDesktop,
            VirtualDiskKind::HyperV,
            VirtualDiskKind::VmWare,
            VirtualDiskKind::VirtualBox,
            VirtualDiskKind::Unknown,
        ] {
            let guidance = kind.guidance();
            assert!(guidance.len() > 40, "{kind:?} needs real guidance");
            assert!(
                !guidance.to_lowercase().contains("delete the file to"),
                "{kind:?} must not recommend deletion"
            );
        }
    }

    #[test]
    fn wsl_guidance_explains_why_the_disk_stays_large() {
        let guidance = VirtualDiskKind::Wsl.guidance();
        assert!(
            guidance.contains("does not shrink"),
            "users need to know why a WSL disk stays big"
        );
    }

    #[test]
    fn detection_is_cancellable() {
        let control = TaskControl::new();
        control.cancel();
        assert!(detect(&control).is_err());
    }
}
