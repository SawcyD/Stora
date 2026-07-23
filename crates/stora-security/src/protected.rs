use stora_core::{Result, StoraError};

use crate::path::{self, is_within};

/// Directories Stora will never delete from, regardless of what any plan,
/// rule, or frontend request says.
///
/// These are matched as ancestors: anything beneath them is protected too,
/// with narrowly scoped exceptions listed in [`ALLOWED_WITHIN_PROTECTED`].
const PROTECTED_ROOTS: &[&str] = &[
    "C:\\Windows",
    "C:\\Program Files",
    "C:\\Program Files (x86)",
    "C:\\ProgramData\\Microsoft\\Windows",
    "C:\\System Volume Information",
    "C:\\$Recycle.Bin",
    "C:\\Recovery",
    "C:\\Boot",
    "C:\\EFI",
    "C:\\PerfLogs",
];

/// Specific regeneratable caches that live inside an otherwise protected root.
///
/// Each entry is a genuine Windows-managed cache whose removal is supported.
const ALLOWED_WITHIN_PROTECTED: &[&str] = &[
    "C:\\Windows\\Temp",
    "C:\\Windows\\Prefetch",
    "C:\\Windows\\SoftwareDistribution\\Download",
];

/// File names that must never be removed even when they appear in a scan.
const PROTECTED_FILE_NAMES: &[&str] = &[
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "bootmgr",
    "ntldr",
    "boot.ini",
    "ntuser.dat",
];

/// Path fragments that suggest credentials or keys. Excluded from quarantine
/// and from any bulk operation so secrets are not copied around.
const SENSITIVE_FRAGMENTS: &[&str] = &[
    "\\.ssh\\",
    "\\.gnupg\\",
    "\\.aws\\credentials",
    "\\.kube\\config",
    "\\microsoft\\crypto\\",
    "\\microsoft\\protect\\",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionVerdict {
    Allowed,
    /// A Windows system location.
    ProtectedSystemPath,
    /// A volume root — deleting one is never a cleanup action.
    VolumeRoot,
    /// A file Windows itself manages.
    ProtectedSystemFile,
}

/// Classifies a normalized path. Callers must reject anything that is not
/// [`ProtectionVerdict::Allowed`].
pub fn classify(normalized_path: &str) -> ProtectionVerdict {
    let trimmed = normalized_path.trim_end_matches('\\');

    // `C:` or `C:\` — a drive root.
    if trimmed.len() <= 2 {
        return ProtectionVerdict::VolumeRoot;
    }

    let name = path::file_name_of(normalized_path).to_ascii_lowercase();
    if PROTECTED_FILE_NAMES.contains(&name.as_str()) {
        return ProtectionVerdict::ProtectedSystemFile;
    }

    // An explicit allowance beats the surrounding protected root, but only for
    // paths strictly *inside* it — never the cache directory itself.
    for allowed in ALLOWED_WITHIN_PROTECTED {
        if is_within(normalized_path, allowed) && !path_equals(normalized_path, allowed) {
            return ProtectionVerdict::Allowed;
        }
    }

    for root in PROTECTED_ROOTS {
        if is_within(normalized_path, root) {
            return ProtectionVerdict::ProtectedSystemPath;
        }
    }

    ProtectionVerdict::Allowed
}

pub fn is_protected(normalized_path: &str) -> bool {
    classify(normalized_path) != ProtectionVerdict::Allowed
}

/// Returns an error unless the path may be deleted.
pub fn ensure_deletable(normalized_path: &str) -> Result<()> {
    match classify(normalized_path) {
        ProtectionVerdict::Allowed => Ok(()),
        _ => Err(StoraError::ProtectedPath {
            path: normalized_path.to_string(),
        }),
    }
}

/// True when a path likely holds credentials or key material.
pub fn is_sensitive(normalized_path: &str) -> bool {
    let lowered = normalized_path.to_ascii_lowercase();
    SENSITIVE_FRAGMENTS
        .iter()
        .any(|fragment| lowered.contains(fragment))
}

fn path_equals(a: &str, b: &str) -> bool {
    a.trim_end_matches('\\')
        .eq_ignore_ascii_case(b.trim_end_matches('\\'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_windows_directory() {
        assert_eq!(
            classify("C:\\Windows\\System32\\kernel32.dll"),
            ProtectionVerdict::ProtectedSystemPath
        );
        assert!(ensure_deletable("C:\\Windows\\System32").is_err());
    }

    #[test]
    fn protects_program_files() {
        assert!(is_protected("C:\\Program Files\\App\\app.exe"));
        assert!(is_protected("C:\\Program Files (x86)\\App"));
    }

    #[test]
    fn protects_volume_roots() {
        assert_eq!(classify("C:\\"), ProtectionVerdict::VolumeRoot);
        assert_eq!(classify("D:"), ProtectionVerdict::VolumeRoot);
    }

    #[test]
    fn protects_system_managed_files_anywhere() {
        assert_eq!(
            classify("C:\\pagefile.sys"),
            ProtectionVerdict::ProtectedSystemFile
        );
        assert_eq!(
            classify("D:\\hiberfil.sys"),
            ProtectionVerdict::ProtectedSystemFile
        );
    }

    #[test]
    fn allows_windows_temp_contents() {
        assert_eq!(
            classify("C:\\Windows\\Temp\\build.log"),
            ProtectionVerdict::Allowed
        );
    }

    #[test]
    fn still_protects_the_windows_temp_directory_itself() {
        assert_eq!(
            classify("C:\\Windows\\Temp"),
            ProtectionVerdict::ProtectedSystemPath,
            "we may clear the cache but never remove the folder"
        );
    }

    #[test]
    fn does_not_protect_lookalike_siblings() {
        assert_eq!(
            classify("C:\\Windows Old Backup"),
            ProtectionVerdict::Allowed
        );
        assert_eq!(
            classify("C:\\Program Files Custom"),
            ProtectionVerdict::Allowed
        );
    }

    #[test]
    fn allows_ordinary_user_paths() {
        assert!(!is_protected("C:\\Users\\Test\\Downloads\\file.zip"));
        assert!(!is_protected("D:\\Development\\project\\node_modules"));
    }

    #[test]
    fn protection_is_case_insensitive() {
        assert!(is_protected("c:\\windows\\system32"));
        assert!(is_protected("C:\\PROGRAM FILES\\app"));
    }

    #[test]
    fn detects_sensitive_locations() {
        assert!(is_sensitive("C:\\Users\\Test\\.ssh\\id_rsa"));
        assert!(is_sensitive("C:\\Users\\Test\\.aws\\credentials"));
        assert!(!is_sensitive("C:\\Users\\Test\\Downloads\\notes.txt"));
    }
}
