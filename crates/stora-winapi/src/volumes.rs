use stora_core::model::{DriveInfo, DriveType};
use stora_core::{Result, StoraError};

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows::Win32::System::WindowsProgramming::{
        DRIVE_CDROM, DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOTE, DRIVE_REMOVABLE,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn from_wide(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    fn drive_type_of(root: &str) -> DriveType {
        let wide_root = wide(root);
        // SAFETY: `wide_root` is a NUL-terminated UTF-16 buffer that outlives
        // the call, which is all GetDriveTypeW requires.
        match unsafe { GetDriveTypeW(PCWSTR(wide_root.as_ptr())) } {
            DRIVE_FIXED => DriveType::Fixed,
            DRIVE_REMOVABLE => DriveType::Removable,
            DRIVE_REMOTE => DriveType::Network,
            DRIVE_CDROM => DriveType::CdRom,
            DRIVE_RAMDISK => DriveType::RamDisk,
            _ => DriveType::Unknown,
        }
    }

    fn volume_details(root: &str) -> Option<(String, String)> {
        let wide_root = wide(root);
        let mut label = [0u16; 261];
        let mut filesystem = [0u16; 261];

        // SAFETY: both buffers are sized per the Win32 contract (MAX_PATH + 1)
        // and are passed with their true lengths.
        let ok = unsafe {
            GetVolumeInformationW(
                PCWSTR(wide_root.as_ptr()),
                Some(&mut label),
                None,
                None,
                None,
                Some(&mut filesystem),
            )
        };

        if ok.is_err() {
            return None;
        }
        Some((from_wide(&label), from_wide(&filesystem)))
    }

    fn capacity(root: &str) -> Option<(u64, u64)> {
        let wide_root = wide(root);
        let mut free_to_caller = 0u64;
        let mut total = 0u64;
        let mut total_free = 0u64;

        // SAFETY: all three out-params are valid, initialized u64 locals.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(wide_root.as_ptr()),
                Some(&mut free_to_caller),
                Some(&mut total),
                Some(&mut total_free),
            )
        };

        if ok.is_err() {
            return None;
        }
        // Report the quota-aware figure: it is what the user can actually use.
        Some((total, free_to_caller))
    }

    pub fn enumerate() -> Result<Vec<DriveInfo>> {
        // SAFETY: no arguments, no pointers; returns a bitmask of drive letters.
        let mask = unsafe { GetLogicalDrives() };
        if mask == 0 {
            return Err(StoraError::Windows("GetLogicalDrives failed".into()));
        }

        let mut drives = Vec::new();
        for index in 0..26u32 {
            if mask & (1 << index) == 0 {
                continue;
            }
            let letter = (b'A' + index as u8) as char;
            let root = format!("{letter}:\\");

            let drive_type = drive_type_of(&root);
            // Skip optical and network volumes: neither is meaningful to
            // analyze for local storage cleanup.
            if matches!(drive_type, DriveType::CdRom | DriveType::Network) {
                continue;
            }

            // A removable slot with no media returns no capacity; skip quietly
            // rather than surfacing an error for an empty card reader.
            let Some((total_bytes, free_bytes)) = capacity(&root) else {
                continue;
            };

            let (label, filesystem) =
                volume_details(&root).unwrap_or_else(|| (String::new(), String::new()));

            let label = if label.is_empty() {
                match drive_type {
                    DriveType::Removable => "Removable Disk".to_string(),
                    _ => "Local Disk".to_string(),
                }
            } else {
                label
            };

            drives.push(DriveInfo {
                root,
                label,
                filesystem,
                total_bytes,
                free_bytes,
                is_removable: matches!(drive_type, DriveType::Removable),
                drive_type,
            });
        }

        Ok(drives)
    }

    pub fn drive_for(path: &str) -> Result<DriveInfo> {
        let root = path
            .get(..3)
            .filter(|prefix| prefix.as_bytes()[1] == b':')
            .ok_or_else(|| StoraError::InvalidPath {
                reason: "path has no drive letter".into(),
            })?
            .to_ascii_uppercase();

        enumerate()?
            .into_iter()
            .find(|drive| drive.root.eq_ignore_ascii_case(&root))
            .ok_or(StoraError::VolumeUnavailable { volume: root })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn enumerate() -> Result<Vec<DriveInfo>> {
        Err(StoraError::UnsupportedFilesystem {
            filesystem: "non-windows host".into(),
        })
    }

    pub fn drive_for(_path: &str) -> Result<DriveInfo> {
        Err(StoraError::UnsupportedFilesystem {
            filesystem: "non-windows host".into(),
        })
    }
}

/// Enumerates local fixed and removable volumes.
///
/// Optical and network drives are omitted: Stora analyzes local storage, and
/// walking a network share would be slow and misleading.
pub fn enumerate_drives() -> Result<Vec<DriveInfo>> {
    imp::enumerate()
}

/// Resolves the volume that contains `path`.
pub fn drive_for_path(path: &str) -> Result<DriveInfo> {
    imp::drive_for(path)
}
