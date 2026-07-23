//! Curated, offline explanations of what writes to well-known locations.
//!
//! Every entry is hand-written and cites primary documentation. There is no
//! language model here, no web request, and no confidence score: a location
//! either has a curated entry or it does not, and "no information available"
//! is a perfectly good answer.
//!
//! This exists because the honest answer to "is it safe to delete this?" is
//! usually a specific, checkable fact — *Windows Installer needs this folder
//! to uninstall MSI software* — not a probability.

use serde::{Deserialize, Serialize};

/// The checked-in knowledge file, embedded so a deployed binary needs nothing
/// beside it.
const LOCATIONS: &str = include_str!("../data/locations.json");

#[derive(Debug, Clone, Deserialize)]
struct KnowledgeFile {
    entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    /// A `%VAR%` path, an absolute path, or a `**\name` suffix match.
    pub pattern: String,
    pub title: String,
    /// What puts data here.
    pub written_by: String,
    /// What actually happens if it goes.
    pub if_removed: String,
    /// Whether removing it is a supported thing to do at all.
    pub removable: bool,
    pub source_title: String,
    pub source_url: String,
}

/// The result of a lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Explanation {
    pub path: String,
    /// `None` when nothing is known. The interface says so rather than
    /// guessing.
    pub entry: Option<Entry>,
}

impl Explanation {
    pub fn unknown(path: &str) -> Self {
        Self {
            path: path.to_string(),
            entry: None,
        }
    }
}

/// Loads the curated entries.
pub fn entries() -> Vec<Entry> {
    serde_json::from_str::<KnowledgeFile>(LOCATIONS)
        .map(|file| file.entries)
        .unwrap_or_default()
}

/// Expands a pattern's environment variables, when it has any.
fn expand(pattern: &str) -> Option<String> {
    if !pattern.contains('%') {
        return Some(pattern.to_string());
    }
    stora_winapi::expand_environment(pattern)
}

/// Decides whether `path` is covered by `pattern`.
///
/// Three shapes are supported, in order of specificity:
///
/// * `**\name` — the last component matches, anywhere on disk.
/// * `%VAR%\...` — expanded, then treated as a prefix.
/// * an absolute path — treated as a prefix.
///
/// Prefix matching compares whole components, so `C:\Windows\Temp` never
/// matches `C:\Windows\Temporary`.
pub fn matches(path: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("**\\") {
        return stora_security::file_name_of(path).eq_ignore_ascii_case(suffix);
    }

    let Some(expanded) = expand(pattern) else {
        return false;
    };

    stora_security::is_within(path, &expanded)
}

/// Finds the most specific curated entry covering `path`.
///
/// Specificity is the length of the expanded pattern, so an entry for
/// `C:\Windows\Temp` wins over one for `C:\Windows`. A suffix rule is only
/// used when no path-based rule applies, because a folder named `target`
/// inside a documented Windows location is the Windows location's business.
pub fn explain(path: &str) -> Explanation {
    let Ok(normalized) = stora_security::normalize(path) else {
        return Explanation::unknown(path);
    };

    let all = entries();

    let mut best: Option<(usize, Entry)> = None;

    for entry in &all {
        if entry.pattern.starts_with("**\\") {
            continue;
        }
        if !matches(&normalized, &entry.pattern) {
            continue;
        }
        let specificity = expand(&entry.pattern).map(|p| p.len()).unwrap_or(0);
        if best
            .as_ref()
            .is_none_or(|(best_len, _)| specificity > *best_len)
        {
            best = Some((specificity, entry.clone()));
        }
    }

    if let Some((_, entry)) = best {
        return Explanation {
            path: normalized,
            entry: Some(entry),
        };
    }

    for entry in &all {
        if entry.pattern.starts_with("**\\") && matches(&normalized, &entry.pattern) {
            return Explanation {
                path: normalized,
                entry: Some(entry.clone()),
            };
        }
    }

    Explanation::unknown(&normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_knowledge_file_parses() {
        let all = entries();
        assert!(!all.is_empty(), "the embedded knowledge file must load");
    }

    #[test]
    fn every_entry_is_complete_and_cited() {
        for entry in entries() {
            assert!(!entry.id.is_empty());
            assert!(!entry.title.is_empty());
            assert!(
                entry.written_by.len() > 15,
                "{} needs to say what writes there",
                entry.id
            );
            assert!(
                entry.if_removed.len() > 25,
                "{} needs to say what happens if removed",
                entry.id
            );
            assert!(
                entry.source_url.starts_with("https://"),
                "{} needs a real citation",
                entry.id
            );
            assert!(
                !entry.source_title.is_empty(),
                "{} needs a source title",
                entry.id
            );
        }
    }

    #[test]
    fn entry_ids_are_unique() {
        let all = entries();
        let mut ids: Vec<String> = all.iter().map(|e| e.id.clone()).collect();
        ids.sort();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate entry id");
    }

    #[test]
    fn critical_windows_locations_are_marked_unremovable() {
        // These are the entries that matter most: getting one of them wrong
        // would be an instruction to break Windows.
        let all = entries();
        for id in [
            "winsxs",
            "system32",
            "installer",
            "pagefile",
            "hiberfil",
            "systemVolumeInformation",
        ] {
            let entry = all
                .iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("missing entry: {id}"));
            assert!(
                !entry.removable,
                "{id} must never be described as removable"
            );
        }
    }

    #[test]
    fn no_entry_contains_a_confidence_score() {
        // The knowledge base states facts and cites them. A numeric score
        // would imply a calibration that does not exist.
        let raw = LOCATIONS.to_lowercase();
        assert!(!raw.contains("\"confidence\""));
        assert!(!raw.contains("\"safe\":"));
    }

    #[test]
    fn suffix_patterns_match_the_last_component_anywhere() {
        assert!(matches(
            "D:\\Dev\\project\\node_modules",
            "**\\node_modules"
        ));
        assert!(matches("C:\\a\\b\\c\\target", "**\\target"));
        assert!(!matches("D:\\Dev\\node_modules\\react", "**\\node_modules"));
    }

    #[test]
    fn absolute_patterns_respect_component_boundaries() {
        assert!(matches("C:\\Windows\\Temp\\a.tmp", "C:\\Windows\\Temp"));
        assert!(
            !matches("C:\\Windows\\Temporary", "C:\\Windows\\Temp"),
            "a prefix must not match a longer sibling name"
        );
    }

    #[test]
    fn the_most_specific_entry_wins() {
        // Both `C:\Windows\Temp` and (via System32) other Windows rules could
        // apply; the narrower one must be chosen.
        let explanation = explain("C:\\Windows\\Temp\\install.log");
        assert_eq!(explanation.entry.map(|e| e.id), Some("windowsTemp".into()));
    }

    #[test]
    fn a_critical_location_is_explained_as_such() {
        let explanation = explain("C:\\Windows\\System32\\kernel32.dll");
        let entry = explanation.entry.expect("System32 is documented");
        assert_eq!(entry.id, "system32");
        assert!(!entry.removable);
    }

    #[test]
    fn winsxs_explains_why_its_size_is_misleading() {
        let entry = explain("C:\\Windows\\WinSxS\\Manifests")
            .entry
            .expect("WinSxS is documented");
        assert!(
            entry.if_removed.contains("hard links"),
            "the size illusion is the single most misunderstood thing about WinSxS"
        );
    }

    #[test]
    fn an_unknown_path_returns_no_entry_rather_than_a_guess() {
        let explanation = explain("D:\\Some\\Unremarkable\\Folder");
        assert!(
            explanation.entry.is_none(),
            "an unknown location must say nothing, not invent an answer"
        );
    }

    #[test]
    fn an_invalid_path_is_handled_without_panicking() {
        assert!(explain("").entry.is_none());
        assert!(explain("..\\..\\escape").entry.is_none());
        assert!(explain("relative\\path").entry.is_none());
    }

    #[test]
    fn lookup_normalizes_the_path_first() {
        // Separators are unified and the drive letter is capitalized. Folder
        // casing is left alone on purpose — NTFS stores it, and rewriting it
        // would make Stora display a path that differs from the real one.
        let explanation = explain("c:/windows/temp/thing.log");
        assert_eq!(explanation.path, "C:\\windows\\temp\\thing.log");
        assert!(
            explanation.entry.is_some(),
            "matching is case-insensitive even though display casing is preserved"
        );
    }

    #[test]
    fn a_path_rule_beats_a_suffix_rule() {
        // `C:\Windows\Installer` is documented as a path; nothing should let a
        // generic suffix rule override a specific Windows location.
        let entry = explain("C:\\Windows\\Installer\\abc.msi")
            .entry
            .expect("found");
        assert_eq!(entry.id, "installer");
    }

    #[test]
    fn developer_suffix_rules_still_work_outside_windows_folders() {
        let entry = explain("D:\\Dev\\my-app\\node_modules")
            .entry
            .expect("found");
        assert_eq!(entry.id, "nodeModules");
        assert!(entry.removable);
    }
}
