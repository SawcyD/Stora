use std::path::{Component, Path, PathBuf};

use stora_core::{Result, StoraError};

/// Windows' documented limit for a non-extended path.
const MAX_PATH: usize = 260;

/// Normalizes a path received from the frontend into a canonical, absolute,
/// backslash-separated Windows path.
///
/// This rejects traversal rather than silently resolving it, because a `..`
/// arriving from the UI always indicates either a bug or an attack — a
/// legitimate selection is produced from a backend-generated plan.
pub fn normalize(input: &str) -> Result<String> {
    if input.trim().is_empty() {
        return Err(StoraError::InvalidPath {
            reason: "path is empty".into(),
        });
    }

    // Interior NULs would truncate the path once handed to a Win32 call.
    if input.contains('\0') {
        return Err(StoraError::InvalidPath {
            reason: "path contains a null character".into(),
        });
    }

    let unified = input.replace('/', "\\");
    let stripped = strip_extended_prefix(&unified);

    if stripped.starts_with("\\\\") {
        return Err(StoraError::InvalidPath {
            reason: "UNC and network paths are not supported".into(),
        });
    }

    let path = Path::new(stripped);
    let mut normalized = PathBuf::new();
    let mut has_root = false;

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                normalized.push(prefix.as_os_str());
                has_root = true;
            }
            Component::RootDir => normalized.push("\\"),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(StoraError::InvalidPath {
                    reason: "path traversal is not permitted".into(),
                })
            }
            Component::Normal(part) => {
                let text = part.to_string_lossy();
                // Trailing dots and spaces are stripped by Win32, which would
                // make the stored path and the deleted path disagree.
                if text.ends_with('.') || text.ends_with(' ') {
                    return Err(StoraError::InvalidPath {
                        reason: "path component ends with a dot or space".into(),
                    });
                }
                normalized.push(part);
            }
        }
    }

    // On non-Windows hosts (tests, CI) `Component::Prefix` never appears, so
    // fall back to a syntactic drive-letter check.
    if !has_root && !looks_like_drive_path(stripped) {
        return Err(StoraError::InvalidPath {
            reason: "path must be absolute and include a drive letter".into(),
        });
    }

    let result = normalized.to_string_lossy().replace('/', "\\");
    let result = if result.is_empty() {
        stripped.to_string()
    } else {
        result
    };

    Ok(canonical_case(&result))
}

fn strip_extended_prefix(path: &str) -> &str {
    path.strip_prefix("\\\\?\\UNC\\")
        .map(|_| path)
        .unwrap_or_else(|| path.strip_prefix("\\\\?\\").unwrap_or(path))
}

fn looks_like_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\')
}

/// Uppercases the drive letter only. File name casing is preserved because
/// NTFS stores it, even though lookups are case-insensitive.
fn canonical_case(path: &str) -> String {
    let mut chars: Vec<char> = path.chars().collect();
    if chars.len() >= 2 && chars[1] == ':' {
        chars[0] = chars[0].to_ascii_uppercase();
    }
    chars.into_iter().collect()
}

/// Adds the `\\?\` prefix when a path would otherwise exceed `MAX_PATH`.
///
/// Applied at the last moment before a filesystem call; stored paths stay in
/// their readable form.
pub fn to_extended_length(path: &str) -> String {
    if path.len() < MAX_PATH || path.starts_with("\\\\?\\") {
        path.to_string()
    } else {
        format!("\\\\?\\{path}")
    }
}

/// Case-insensitive containment test used for exclusions and authorization.
///
/// Compares whole path components so `C:\Temp2` is not treated as living
/// inside `C:\Temp`.
pub fn is_within(candidate: &str, ancestor: &str) -> bool {
    let candidate = candidate.trim_end_matches('\\').to_ascii_lowercase();
    let ancestor = ancestor.trim_end_matches('\\').to_ascii_lowercase();

    if candidate == ancestor {
        return true;
    }
    candidate.starts_with(&format!("{ancestor}\\"))
}

/// Returns the parent directory of a normalized path, or `None` at the root.
pub fn parent_of(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('\\');
    let idx = trimmed.rfind('\\')?;
    if idx <= 2 {
        // `C:\Users` -> `C:\`
        return Some(format!("{}\\", &trimmed[..idx]));
    }
    Some(trimmed[..idx].to_string())
}

pub fn file_name_of(path: &str) -> String {
    let trimmed = path.trim_end_matches('\\');
    match trimmed.rfind('\\') {
        Some(idx) => trimmed[idx + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

pub fn extension_of(path: &str) -> Option<String> {
    let name = file_name_of(path);
    let idx = name.rfind('.')?;
    if idx == 0 || idx + 1 >= name.len() {
        return None;
    }
    Some(name[idx + 1..].to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_separators_and_drive_case() {
        assert_eq!(normalize("c:/Users/Test").unwrap(), "C:\\Users\\Test");
        assert_eq!(normalize("C:\\Users\\Test").unwrap(), "C:\\Users\\Test");
    }

    #[test]
    fn removes_redundant_current_directory_segments() {
        assert_eq!(normalize("C:\\Users\\.\\Test").unwrap(), "C:\\Users\\Test");
    }

    #[test]
    fn rejects_traversal() {
        let err = normalize("C:\\Users\\..\\Windows").unwrap_err();
        assert_eq!(err.code(), "InvalidPath");
    }

    #[test]
    fn rejects_traversal_disguised_by_forward_slashes() {
        assert!(normalize("C:/Users/../../Windows").is_err());
    }

    #[test]
    fn rejects_relative_and_empty_paths() {
        assert!(normalize("Users\\Test").is_err());
        assert!(normalize("").is_err());
        assert!(normalize("   ").is_err());
    }

    #[test]
    fn rejects_null_bytes() {
        assert!(normalize("C:\\Users\\Test\0.txt").is_err());
    }

    #[test]
    fn rejects_unc_paths() {
        assert!(normalize("\\\\server\\share\\file.txt").is_err());
    }

    #[test]
    fn rejects_trailing_dot_or_space_components() {
        assert!(normalize("C:\\Users\\Test.").is_err());
        assert!(normalize("C:\\Users\\Test ").is_err());
    }

    #[test]
    fn accepts_extended_length_prefix() {
        assert_eq!(
            normalize("\\\\?\\C:\\Users\\Test").unwrap(),
            "C:\\Users\\Test"
        );
    }

    #[test]
    fn preserves_unicode_components() {
        let path = normalize("C:\\Users\\日本語\\Проект\\café.txt").unwrap();
        assert_eq!(path, "C:\\Users\\日本語\\Проект\\café.txt");
    }

    #[test]
    fn adds_extended_prefix_only_for_long_paths() {
        let short = "C:\\Users\\Test";
        assert_eq!(to_extended_length(short), short);

        let long = format!("C:\\{}", "a".repeat(300));
        assert!(to_extended_length(&long).starts_with("\\\\?\\"));
    }

    #[test]
    fn does_not_double_prefix_long_paths() {
        let long = format!("\\\\?\\C:\\{}", "a".repeat(300));
        assert_eq!(to_extended_length(&long), long);
    }

    #[test]
    fn containment_respects_component_boundaries() {
        assert!(is_within("C:\\Temp\\a.txt", "C:\\Temp"));
        assert!(is_within("C:\\Temp", "C:\\Temp"));
        assert!(!is_within("C:\\Temp2\\a.txt", "C:\\Temp"));
        assert!(!is_within("C:\\Other", "C:\\Temp"));
    }

    #[test]
    fn containment_is_case_insensitive() {
        assert!(is_within("c:\\temp\\A.TXT", "C:\\Temp"));
    }

    #[test]
    fn parent_walks_up_to_the_volume_root() {
        assert_eq!(parent_of("C:\\Users\\Test"), Some("C:\\Users".into()));
        assert_eq!(parent_of("C:\\Users"), Some("C:\\".into()));
        assert_eq!(parent_of("C:\\"), None);
    }

    #[test]
    fn extracts_names_and_extensions() {
        assert_eq!(file_name_of("C:\\Temp\\cache.bin"), "cache.bin");
        assert_eq!(extension_of("C:\\Temp\\cache.bin"), Some("bin".into()));
        assert_eq!(extension_of("C:\\Temp\\archive.TAR.GZ"), Some("gz".into()));
        assert_eq!(extension_of("C:\\Temp\\.gitignore"), None);
        assert_eq!(extension_of("C:\\Temp\\noext"), None);
    }
}
