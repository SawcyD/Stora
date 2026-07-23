//! UserAssist: Windows' own record of programs launched through Explorer.
//!
//! # What this can and cannot tell us
//!
//! UserAssist lives under `HKCU`, so reading it needs no elevation. It records
//! a run count and a last-executed timestamp for programs started through the
//! shell — Start menu, desktop, taskbar, File Explorer.
//!
//! It does **not** see programs started from a terminal, a script, a launcher
//! such as Steam, or by another process. An application missing from
//! UserAssist has therefore not been shown to be unused; it has only not been
//! seen *by this mechanism*. Every caller must treat absence as unknown.
//!
//! Because it is Windows' own bookkeeping rather than something Stora
//! witnessed, results are reported as a *Windows activity estimate* at medium
//! confidence, never as a launch Stora observed.

use serde::{Deserialize, Serialize};

/// One decoded UserAssist entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAssistEntry {
    /// Decoded program path or shell identifier.
    pub name: String,
    pub run_count: u32,
    /// Unix seconds, or `None` when the record carries no usable timestamp.
    pub last_executed: Option<i64>,
}

/// Decodes the ROT13 obfuscation Windows applies to UserAssist value names.
///
/// This is not encryption and was never meant as such — it exists only to stop
/// the names showing up in a naive string search of the registry.
pub fn rot13(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            'a'..='z' => (((c as u8 - b'a' + 13) % 26) + b'a') as char,
            'A'..='Z' => (((c as u8 - b'A' + 13) % 26) + b'A') as char,
            other => other,
        })
        .collect()
}

/// Converts a Windows `FILETIME` (100 ns ticks since 1601) to Unix seconds.
///
/// Returns `None` for zero or nonsensical values rather than emitting a
/// timestamp from the seventeenth century.
pub fn filetime_to_unix(filetime: u64) -> Option<i64> {
    if filetime == 0 {
        return None;
    }

    // Ticks between 1601-01-01 and 1970-01-01.
    const EPOCH_DIFFERENCE: u64 = 116_444_736_000_000_000;
    if filetime < EPOCH_DIFFERENCE {
        return None;
    }

    let unix = (filetime - EPOCH_DIFFERENCE) / 10_000_000;

    // Anything beyond a century from the epoch is corrupt, not a real date.
    if unix > 4_102_444_800 {
        return None;
    }

    Some(unix as i64)
}

/// Parses the binary value stored against a UserAssist entry.
///
/// The Windows 7 and later layout is 72 bytes: a session id, a run count at
/// offset 4, focus statistics, then a `FILETIME` at offset 60. The older
/// Windows XP layout is 16 bytes with the count at offset 4 and the timestamp
/// at offset 8; it is still handled because a long-lived profile can carry
/// entries forward.
pub fn parse_value(data: &[u8]) -> Option<(u32, Option<i64>)> {
    if data.len() >= 68 {
        let run_count = u32::from_le_bytes(data[4..8].try_into().ok()?);
        let filetime = u64::from_le_bytes(data[60..68].try_into().ok()?);
        return Some((run_count, filetime_to_unix(filetime)));
    }

    if data.len() >= 16 {
        // The XP-era counter starts at 5; normalize it so a single run reads
        // as one rather than six.
        let raw = u32::from_le_bytes(data[4..8].try_into().ok()?);
        let run_count = raw.saturating_sub(5);
        let filetime = u64::from_le_bytes(data[8..16].try_into().ok()?);
        return Some((run_count, filetime_to_unix(filetime)));
    }

    None
}

/// True when a decoded name refers to a real executable rather than a shell
/// pseudo-entry.
///
/// UserAssist also records `UEME_CTLSESSION`, GUID-prefixed shell folders, and
/// control panel applets. Only paths to executables are useful for attributing
/// activity to an installed application.
pub fn is_executable_entry(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();

    if lowered.starts_with("ueme_") {
        return false;
    }
    if !lowered.ends_with(".exe") {
        return false;
    }
    // Entries are sometimes recorded against a known-folder GUID rather than
    // a real path; those cannot be matched to an install location.
    if lowered.starts_with('{') {
        return false;
    }

    true
}

#[cfg(windows)]
mod registry {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    };

    const USERASSIST_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist";

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open(path: &str) -> Option<HKEY> {
        let wide_path = wide(path);
        let mut key = HKEY::default();

        // SAFETY: `wide_path` is NUL-terminated; `key` receives the handle.
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(wide_path.as_ptr()),
                0,
                KEY_READ,
                &mut key,
            )
        };

        (opened == ERROR_SUCCESS).then_some(key)
    }

    /// Enumerates the `Count` subkey of one UserAssist GUID.
    fn read_count_key(guid: &str, entries: &mut Vec<UserAssistEntry>) {
        let Some(key) = open(&format!("{USERASSIST_PATH}\\{guid}\\Count")) else {
            return;
        };

        let mut index = 0u32;
        loop {
            let mut name = [0u16; MAX_PATH as usize * 2];
            let mut name_length = name.len() as u32;
            let mut data = [0u8; 512];
            let mut data_length = data.len() as u32;

            // SAFETY: both buffers are passed with their true lengths, which
            // the API updates in place.
            let result = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    windows::core::PWSTR(name.as_mut_ptr()),
                    &mut name_length,
                    None,
                    None,
                    Some(data.as_mut_ptr()),
                    Some(&mut data_length),
                )
            };

            if result != ERROR_SUCCESS {
                break;
            }
            index += 1;

            let encoded = String::from_utf16_lossy(&name[..name_length as usize]);
            let decoded = rot13(&encoded);

            if !is_executable_entry(&decoded) {
                continue;
            }

            let Some((run_count, last_executed)) = parse_value(&data[..data_length as usize])
            else {
                continue;
            };

            entries.push(UserAssistEntry {
                name: decoded.replace('/', "\\"),
                run_count,
                last_executed,
            });
        }

        // SAFETY: `key` came from a successful open.
        unsafe {
            let _ = RegCloseKey(key);
        }
    }

    pub fn read_all() -> Vec<UserAssistEntry> {
        let mut entries = Vec::new();

        let Some(root) = open(USERASSIST_PATH) else {
            return entries;
        };

        // The GUID subkeys vary between Windows versions, so enumerate them
        // rather than hardcoding the well-known ones.
        let mut index = 0u32;
        loop {
            let mut name = [0u16; 64];
            let mut length = name.len() as u32;

            // SAFETY: `name` is `length` units long, as declared.
            let result = unsafe {
                RegEnumKeyExW(
                    root,
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

            let guid = String::from_utf16_lossy(&name[..length as usize]);
            read_count_key(&guid, &mut entries);
        }

        // SAFETY: `root` came from a successful open.
        unsafe {
            let _ = RegCloseKey(root);
        }

        // The same executable can appear under more than one GUID; keep the
        // most recent sighting.
        entries.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then(b.last_executed.cmp(&a.last_executed))
        });
        entries.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));

        entries
    }
}

#[cfg(not(windows))]
mod registry {
    use super::UserAssistEntry;

    pub fn read_all() -> Vec<UserAssistEntry> {
        Vec::new()
    }
}

/// Reads every usable UserAssist entry for the current user.
pub fn read_entries() -> Vec<UserAssistEntry> {
    registry::read_all()
}

/// Finds the most recent UserAssist entry for an executable inside
/// `install_location`.
///
/// Returns `None` when nothing matches — which means *not seen by this
/// mechanism*, not *unused*.
pub fn newest_within<'a>(
    entries: &'a [UserAssistEntry],
    install_location: &str,
) -> Option<&'a UserAssistEntry> {
    entries
        .iter()
        .filter(|entry| stora_security::is_within(&entry.name, install_location))
        .max_by_key(|entry| entry.last_executed.unwrap_or(i64::MIN))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rot13_round_trips() {
        let original = "C:\\Program Files\\App\\app.exe";
        assert_eq!(rot13(&rot13(original)), original);
    }

    #[test]
    fn rot13_leaves_non_letters_alone() {
        // Drive letters, separators, and digits must survive untouched.
        assert_eq!(rot13("C:\\Windows\\2024"), "P:\\Jvaqbjf\\2024");
        assert_eq!(rot13("P:\\Jvaqbjf\\2024"), "C:\\Windows\\2024");
    }

    #[test]
    fn rot13_handles_both_cases() {
        assert_eq!(rot13("abcXYZ"), "nopKLM");
    }

    #[test]
    fn filetime_converts_to_unix_seconds() {
        // 1970-01-01 in FILETIME ticks.
        assert_eq!(filetime_to_unix(116_444_736_000_000_000), Some(0));
        // Exactly one day later.
        assert_eq!(
            filetime_to_unix(116_444_736_000_000_000 + 864_000_000_000),
            Some(86_400)
        );
    }

    #[test]
    fn an_empty_or_impossible_filetime_yields_nothing() {
        // These must not become a date in 1601 or a date centuries away.
        assert_eq!(filetime_to_unix(0), None);
        assert_eq!(filetime_to_unix(1_000), None);
        assert_eq!(filetime_to_unix(u64::MAX), None);
    }

    /// Builds a Windows 7+ style 72-byte value.
    fn modern_value(run_count: u32, filetime: u64) -> Vec<u8> {
        let mut data = vec![0u8; 72];
        data[4..8].copy_from_slice(&run_count.to_le_bytes());
        data[60..68].copy_from_slice(&filetime.to_le_bytes());
        data
    }

    #[test]
    fn parses_the_modern_value_layout() {
        let filetime = 116_444_736_000_000_000 + 864_000_000_000;
        let (count, last) = parse_value(&modern_value(7, filetime)).expect("parsed");

        assert_eq!(count, 7);
        assert_eq!(last, Some(86_400));
    }

    #[test]
    fn parses_the_legacy_value_layout() {
        // XP-era: 16 bytes, count offset by five.
        let mut data = vec![0u8; 16];
        data[4..8].copy_from_slice(&11u32.to_le_bytes());
        data[8..16].copy_from_slice(&116_444_736_000_000_000u64.to_le_bytes());

        let (count, last) = parse_value(&data).expect("parsed");
        assert_eq!(count, 6, "the legacy counter starts at five");
        assert_eq!(last, Some(0));
    }

    #[test]
    fn a_truncated_value_is_rejected_rather_than_guessed() {
        assert!(parse_value(&[]).is_none());
        assert!(parse_value(&[0u8; 8]).is_none());
    }

    #[test]
    fn a_zero_timestamp_gives_a_count_but_no_date() {
        let (count, last) = parse_value(&modern_value(3, 0)).expect("parsed");
        assert_eq!(count, 3);
        assert_eq!(last, None, "a run count without a date must not invent one");
    }

    #[test]
    fn shell_pseudo_entries_are_ignored() {
        assert!(!is_executable_entry("UEME_CTLSESSION"));
        assert!(!is_executable_entry("UEME_RUNPATH"));
        assert!(!is_executable_entry(
            "{6D809377-6AF0-444B-8957-A3773F02200E}\\App\\app.exe"
        ));
        assert!(!is_executable_entry("Microsoft.Windows.Explorer"));
    }

    #[test]
    fn real_executables_are_accepted() {
        assert!(is_executable_entry("C:\\Program Files\\App\\app.exe"));
        assert!(is_executable_entry("D:\\Games\\Game.EXE"));
    }

    fn entry(name: &str, last: Option<i64>) -> UserAssistEntry {
        UserAssistEntry {
            name: name.into(),
            run_count: 1,
            last_executed: last,
        }
    }

    #[test]
    fn matches_an_executable_inside_the_install_folder() {
        let entries = vec![entry("C:\\Program Files\\App\\bin\\app.exe", Some(500))];
        let found = newest_within(&entries, "C:\\Program Files\\App").expect("matched");
        assert_eq!(found.last_executed, Some(500));
    }

    #[test]
    fn a_sibling_folder_with_a_shared_prefix_does_not_match() {
        let entries = vec![entry("C:\\Program Files\\App2\\app.exe", Some(500))];
        assert!(newest_within(&entries, "C:\\Program Files\\App").is_none());
    }

    #[test]
    fn the_most_recent_entry_wins() {
        let entries = vec![
            entry("C:\\App\\old.exe", Some(100)),
            entry("C:\\App\\new.exe", Some(900)),
        ];
        let found = newest_within(&entries, "C:\\App").unwrap();
        assert_eq!(found.name, "C:\\App\\new.exe");
    }

    #[test]
    fn no_match_means_unknown_not_unused() {
        // The single most important property of this module: silence from
        // UserAssist is not evidence of disuse. A program launched only from
        // a terminal or a game launcher never appears here at all.
        let entries = vec![entry("C:\\Other\\thing.exe", Some(100))];
        assert!(newest_within(&entries, "C:\\Program Files\\NeverShellLaunched").is_none());
    }

    #[test]
    fn reading_entries_never_panics_on_this_machine() {
        // Exercises the real registry path; an empty result is valid.
        let entries = read_entries();
        for found in &entries {
            assert!(!found.name.is_empty());
            assert!(is_executable_entry(&found.name));
        }
    }
}
