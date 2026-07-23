use stora_core::Result;

use crate::classify;
use crate::model::{Confidence, InstalledApp};

/// Raw values read from one uninstall registry entry.
///
/// Kept separate from [`InstalledApp`] so the parsing and interpretation
/// logic can be tested without a registry.
#[derive(Debug, Default, Clone)]
pub struct RawEntry {
    pub key_name: String,
    pub display_name: String,
    pub publisher: String,
    pub display_version: String,
    pub install_location: String,
    pub uninstall_string: String,
    pub install_date: String,
    /// `EstimatedSize` in kilobytes, as Windows stores it.
    pub estimated_size_kb: Option<u32>,
    pub system_component: bool,
    pub is_wow64: bool,
    pub is_per_user: bool,
    /// Present on entries that are updates or patches of a parent product.
    pub parent_key_name: String,
    pub release_type: String,
}

/// Decides whether a registry entry represents something worth listing.
///
/// Windows keeps a great deal in the uninstall keys that is not an
/// application a person installed: updates, patches, and entries explicitly
/// flagged as system components.
pub fn is_listable(entry: &RawEntry) -> bool {
    if entry.display_name.trim().is_empty() {
        return false;
    }
    // Microsoft's own convention: `SystemComponent = 1` means "do not show
    // this in Programs and Features".
    if entry.system_component {
        return false;
    }
    // Updates and patches belong to a parent product, not on their own row.
    if !entry.parent_key_name.trim().is_empty() {
        return false;
    }
    let release = entry.release_type.to_ascii_lowercase();
    if matches!(release.as_str(), "update" | "hotfix" | "securityupdate") {
        return false;
    }
    true
}

/// Parses Windows' `InstallDate`, stored as `YYYYMMDD`.
pub fn parse_install_date(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.len() != 8 || !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let year: i32 = trimmed[0..4].parse().ok()?;
    let month: u32 = trimmed[4..6].parse().ok()?;
    let day: u32 = trimmed[6..8].parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    // Days since the Unix epoch via a civil-date conversion, so no date
    // library is needed for a value this simple.
    let days = days_from_civil(year, month, day);
    Some(days * 86_400)
}

/// Howard Hinnant's `days_from_civil`.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = (year - era * 400) as i64;
    let month = month as i64;
    let day = day as i64;

    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era as i64 * 146_097 + day_of_era - 719_468
}

/// Turns a raw registry entry into a listed application.
pub fn to_installed_app(entry: &RawEntry) -> InstalledApp {
    let install_location = {
        let trimmed = entry.install_location.trim();
        if trimmed.is_empty() {
            None
        } else {
            stora_security::normalize(trimmed).ok()
        }
    };

    let app_type = classify::infer_type(
        &entry.display_name,
        &entry.publisher,
        install_location.as_deref(),
    );

    // A reported size with no install location cannot be verified, so the
    // entry is only medium confidence.
    let confidence = match (&install_location, entry.estimated_size_kb) {
        (Some(_), _) => Confidence::High,
        (None, Some(_)) => Confidence::Medium,
        (None, None) => Confidence::Low,
    };

    let uninstall = {
        let trimmed = entry.uninstall_string.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    };

    InstalledApp {
        id: format!(
            "{}{}",
            if entry.is_per_user {
                "user:"
            } else {
                "machine:"
            },
            entry.key_name
        ),
        name: entry.display_name.trim().to_string(),
        publisher: entry.publisher.trim().to_string(),
        version: entry.display_version.trim().to_string(),
        reported_bytes: entry.estimated_size_kb.map(|kb| kb as u64 * 1024),
        detected_bytes: None,
        install_location,
        install_date: parse_install_date(&entry.install_date),
        app_type_label: app_type.label().to_string(),
        suggestable: app_type.is_suggestable(),
        app_type,
        uninstall_command: uninstall,
        source: if entry.is_wow64 {
            "Uninstall registry (32-bit)".into()
        } else if entry.is_per_user {
            "Uninstall registry (per user)".into()
        } else {
            "Uninstall registry".into()
        },
        confidence_label: confidence.label().to_string(),
        confidence,
    }
}

#[cfg(windows)]
mod registry {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        REG_SAM_FLAGS,
    };

    const UNINSTALL_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn read_string(key: HKEY, name: &str) -> String {
        let wide_name = wide(name);
        let mut size: u32 = 0;

        // SAFETY: querying with a null buffer returns the required size.
        let probe = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(wide_name.as_ptr()),
                None,
                None,
                None,
                Some(&mut size),
            )
        };
        if probe != ERROR_SUCCESS || size == 0 {
            return String::new();
        }

        let mut buffer = vec![0u8; size as usize];
        // SAFETY: `buffer` is exactly `size` bytes, as just reported.
        let read = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(wide_name.as_ptr()),
                None,
                None,
                Some(buffer.as_mut_ptr()),
                Some(&mut size),
            )
        };
        if read != ERROR_SUCCESS {
            return String::new();
        }

        let units: Vec<u16> = buffer
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
        String::from_utf16_lossy(&units[..end])
    }

    fn read_u32(key: HKEY, name: &str) -> Option<u32> {
        let wide_name = wide(name);
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;

        // SAFETY: `value` is a valid u32 out-param of the stated size.
        let read = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(wide_name.as_ptr()),
                None,
                None,
                Some(&mut value as *mut u32 as *mut u8),
                Some(&mut size),
            )
        };
        (read == ERROR_SUCCESS).then_some(value)
    }

    fn enumerate(
        root: HKEY,
        flags: REG_SAM_FLAGS,
        is_wow64: bool,
        is_per_user: bool,
    ) -> Vec<RawEntry> {
        let mut entries = Vec::new();
        let path = wide(UNINSTALL_PATH);
        let mut base = HKEY::default();

        // SAFETY: `path` is NUL-terminated; `base` receives the opened key.
        let opened = unsafe {
            RegOpenKeyExW(
                root,
                PCWSTR(path.as_ptr()),
                0,
                KEY_READ | KEY_ENUMERATE_SUB_KEYS | flags,
                &mut base,
            )
        };
        if opened != ERROR_SUCCESS {
            return entries;
        }

        let mut index = 0u32;
        loop {
            let mut name = [0u16; MAX_PATH as usize];
            let mut length = name.len() as u32;

            // SAFETY: `name` is `length` units long, as declared.
            let result = unsafe {
                RegEnumKeyExW(
                    base,
                    index,
                    windows::core::PWSTR(name.as_mut_ptr()),
                    &mut length,
                    None,
                    windows::core::PWSTR::null(),
                    None,
                    None,
                )
            };
            if result != ERROR_SUCCESS {
                break;
            }
            index += 1;

            let key_name = String::from_utf16_lossy(&name[..length as usize]);
            let sub_path = wide(&format!("{UNINSTALL_PATH}\\{key_name}"));
            let mut sub = HKEY::default();

            // SAFETY: same contract as the parent open.
            let sub_opened = unsafe {
                RegOpenKeyExW(
                    root,
                    PCWSTR(sub_path.as_ptr()),
                    0,
                    KEY_READ | flags,
                    &mut sub,
                )
            };
            if sub_opened != ERROR_SUCCESS {
                continue;
            }

            let entry = RawEntry {
                display_name: read_string(sub, "DisplayName"),
                publisher: read_string(sub, "Publisher"),
                display_version: read_string(sub, "DisplayVersion"),
                install_location: read_string(sub, "InstallLocation"),
                uninstall_string: read_string(sub, "UninstallString"),
                install_date: read_string(sub, "InstallDate"),
                estimated_size_kb: read_u32(sub, "EstimatedSize"),
                system_component: read_u32(sub, "SystemComponent").unwrap_or(0) == 1,
                parent_key_name: read_string(sub, "ParentKeyName"),
                release_type: read_string(sub, "ReleaseType"),
                key_name,
                is_wow64,
                is_per_user,
            };

            // SAFETY: `sub` came from a successful open.
            let _ = unsafe { RegCloseKey(sub) };

            if is_listable(&entry) {
                entries.push(entry);
            }
        }

        // SAFETY: `base` came from a successful open.
        let _ = unsafe { RegCloseKey(base) };
        entries
    }

    pub fn read_all() -> Vec<RawEntry> {
        let mut entries = enumerate(HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY, false, false);
        entries.extend(enumerate(HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, true, false));
        entries.extend(enumerate(HKEY_CURRENT_USER, REG_SAM_FLAGS(0), false, true));
        entries
    }
}

#[cfg(not(windows))]
mod registry {
    use super::RawEntry;

    pub fn read_all() -> Vec<RawEntry> {
        Vec::new()
    }
}

/// Discovers installed applications from the Windows uninstall registry.
///
/// Reads the 64-bit, 32-bit, and per-user views, de-duplicating entries that
/// appear in more than one.
pub fn discover() -> Result<Vec<InstalledApp>> {
    let mut apps: Vec<InstalledApp> = registry::read_all().iter().map(to_installed_app).collect();

    // The same product often registers in both the 64-bit and 32-bit views.
    apps.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then(b.confidence.cmp(&a.confidence))
    });
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name) && a.version == b.version);

    apps.sort_by(|a, b| {
        b.reported_bytes
            .unwrap_or(0)
            .cmp(&a.reported_bytes.unwrap_or(0))
    });
    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AppType;

    fn entry(name: &str) -> RawEntry {
        RawEntry {
            key_name: name.to_string(),
            display_name: name.to_string(),
            publisher: "Contoso".into(),
            display_version: "1.0".into(),
            ..Default::default()
        }
    }

    #[test]
    fn entries_without_a_display_name_are_skipped() {
        let mut raw = entry("Thing");
        raw.display_name = "   ".into();
        assert!(!is_listable(&raw));
    }

    #[test]
    fn system_component_entries_are_hidden() {
        let mut raw = entry("Some Component");
        raw.system_component = true;
        assert!(
            !is_listable(&raw),
            "Windows marks these as not-for-display for a reason"
        );
    }

    #[test]
    fn updates_and_patches_are_not_listed_separately() {
        let mut raw = entry("Update for Thing");
        raw.parent_key_name = "ParentProduct".into();
        assert!(!is_listable(&raw));

        let mut hotfix = entry("Hotfix");
        hotfix.release_type = "Hotfix".into();
        assert!(!is_listable(&hotfix));

        let mut security = entry("Patch");
        security.release_type = "SecurityUpdate".into();
        assert!(!is_listable(&security));
    }

    #[test]
    fn ordinary_applications_are_listed() {
        assert!(is_listable(&entry("Visual Studio Code")));
    }

    #[test]
    fn install_dates_are_parsed_from_the_windows_format() {
        // 1970-01-01 is the epoch itself.
        assert_eq!(parse_install_date("19700101"), Some(0));
        // 2024-01-01 = 19723 days after the epoch.
        assert_eq!(parse_install_date("20240101"), Some(19723 * 86_400));
    }

    #[test]
    fn malformed_install_dates_yield_nothing_rather_than_a_guess() {
        assert_eq!(parse_install_date(""), None);
        assert_eq!(parse_install_date("2024"), None);
        assert_eq!(parse_install_date("notadate"), None);
        assert_eq!(parse_install_date("20241301"), None, "month 13");
        assert_eq!(parse_install_date("20240132"), None, "day 32");
    }

    #[test]
    fn estimated_size_is_converted_from_kilobytes() {
        let mut raw = entry("App");
        raw.estimated_size_kb = Some(2048);
        let app = to_installed_app(&raw);
        assert_eq!(app.reported_bytes, Some(2048 * 1024));
    }

    #[test]
    fn a_missing_size_is_none_rather_than_zero() {
        let app = to_installed_app(&entry("App"));
        assert_eq!(
            app.reported_bytes, None,
            "an absent size must not be reported as zero bytes"
        );
        assert_eq!(app.detected_bytes, None);
    }

    #[test]
    fn confidence_reflects_how_much_is_actually_known() {
        let mut with_location = entry("App");
        with_location.install_location = "C:\\Program Files\\App".into();
        assert_eq!(
            to_installed_app(&with_location).confidence,
            Confidence::High
        );

        let mut size_only = entry("App");
        size_only.estimated_size_kb = Some(100);
        assert_eq!(to_installed_app(&size_only).confidence, Confidence::Medium);

        assert_eq!(to_installed_app(&entry("App")).confidence, Confidence::Low);
    }

    #[test]
    fn per_user_and_machine_entries_get_distinct_ids() {
        let mut machine = entry("App");
        machine.is_per_user = false;
        let mut user = entry("App");
        user.is_per_user = true;

        assert_ne!(to_installed_app(&machine).id, to_installed_app(&user).id);
    }

    #[test]
    fn a_runtime_is_marked_unsuggestable() {
        let raw = entry("Microsoft Visual C++ 2022 Redistributable");
        let app = to_installed_app(&raw);
        assert_eq!(app.app_type, AppType::DriverOrSystemComponent);
        assert!(!app.suggestable);
    }

    #[test]
    fn an_uninstall_string_is_preserved_for_removal() {
        let mut raw = entry("App");
        raw.uninstall_string = "\"C:\\Program Files\\App\\uninstall.exe\" /S".into();
        let app = to_installed_app(&raw);
        assert!(app.uninstall_command.is_some());
    }

    #[test]
    fn a_blank_uninstall_string_becomes_none() {
        let mut raw = entry("App");
        raw.uninstall_string = "   ".into();
        assert!(to_installed_app(&raw).uninstall_command.is_none());
    }
}
