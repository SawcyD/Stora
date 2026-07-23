use stora_core::model::{Exclusion, ExclusionKind};

use crate::path::{extension_of, is_within};

/// Compiled exclusion set. Built once per scan or cleanup so matching stays
/// cheap inside the walker's hot loop.
#[derive(Debug, Default, Clone)]
pub struct ExclusionSet {
    folders: Vec<String>,
    files: Vec<String>,
    extensions: Vec<String>,
    volumes: Vec<String>,
    categories: Vec<String>,
}

impl ExclusionSet {
    pub fn from_rules(rules: &[Exclusion]) -> Self {
        let mut set = Self::default();
        for rule in rules {
            let pattern = rule.pattern.to_ascii_lowercase();
            match rule.kind {
                ExclusionKind::Folder => set.folders.push(pattern),
                ExclusionKind::File => set.files.push(pattern),
                ExclusionKind::Extension => set
                    .extensions
                    .push(pattern.trim_start_matches('.').to_string()),
                ExclusionKind::Volume => set.volumes.push(pattern),
                ExclusionKind::Category => set.categories.push(pattern),
            }
        }
        set
    }

    pub fn is_empty(&self) -> bool {
        self.folders.is_empty()
            && self.files.is_empty()
            && self.extensions.is_empty()
            && self.volumes.is_empty()
    }

    /// True when a path should be skipped entirely, including its children.
    pub fn excludes_directory(&self, path: &str) -> bool {
        self.matches_volume(path) || self.folders.iter().any(|folder| is_within(path, folder))
    }

    /// True when an individual file should be skipped.
    pub fn excludes_file(&self, path: &str) -> bool {
        if self.matches_volume(path) {
            return true;
        }
        let lowered = path.to_ascii_lowercase();
        if self.files.contains(&lowered) {
            return true;
        }
        if self.folders.iter().any(|folder| is_within(path, folder)) {
            return true;
        }
        match extension_of(path) {
            Some(ext) => self.extensions.contains(&ext),
            None => false,
        }
    }

    pub fn excludes_category(&self, category_id: &str) -> bool {
        let lowered = category_id.to_ascii_lowercase();
        self.categories.contains(&lowered)
    }

    fn matches_volume(&self, path: &str) -> bool {
        if self.volumes.is_empty() {
            return false;
        }
        let lowered = path.to_ascii_lowercase();
        self.volumes
            .iter()
            .any(|volume| lowered.starts_with(&volume.to_ascii_lowercase()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stora_core::model::ExclusionReason;

    fn rule(pattern: &str, kind: ExclusionKind) -> Exclusion {
        Exclusion {
            id: 0,
            pattern: pattern.into(),
            kind,
            reason: ExclusionReason::UserExclusion,
            created_at: 0,
        }
    }

    #[test]
    fn folder_exclusion_covers_descendants() {
        let set =
            ExclusionSet::from_rules(&[rule("C:\\Users\\Test\\Vault", ExclusionKind::Folder)]);
        assert!(set.excludes_directory("C:\\Users\\Test\\Vault"));
        assert!(set.excludes_directory("C:\\Users\\Test\\Vault\\Inner"));
        assert!(set.excludes_file("C:\\Users\\Test\\Vault\\secret.txt"));
        assert!(!set.excludes_directory("C:\\Users\\Test\\Other"));
    }

    #[test]
    fn folder_exclusion_does_not_match_sibling_prefixes() {
        let set = ExclusionSet::from_rules(&[rule("C:\\Data", ExclusionKind::Folder)]);
        assert!(!set.excludes_directory("C:\\DataBackup"));
    }

    #[test]
    fn extension_exclusion_ignores_leading_dot() {
        let set = ExclusionSet::from_rules(&[rule(".psd", ExclusionKind::Extension)]);
        assert!(set.excludes_file("C:\\Art\\poster.psd"));
        assert!(set.excludes_file("C:\\Art\\POSTER.PSD"));
        assert!(!set.excludes_file("C:\\Art\\poster.png"));
    }

    #[test]
    fn file_exclusion_matches_exact_path_only() {
        let set = ExclusionSet::from_rules(&[rule("C:\\Temp\\keep.bin", ExclusionKind::File)]);
        assert!(set.excludes_file("c:\\temp\\keep.bin"));
        assert!(!set.excludes_file("C:\\Temp\\other.bin"));
    }

    #[test]
    fn volume_exclusion_skips_the_whole_drive() {
        let set = ExclusionSet::from_rules(&[rule("E:\\", ExclusionKind::Volume)]);
        assert!(set.excludes_directory("E:\\Anything"));
        assert!(!set.excludes_directory("C:\\Anything"));
    }

    #[test]
    fn empty_set_excludes_nothing() {
        let set = ExclusionSet::default();
        assert!(set.is_empty());
        assert!(!set.excludes_file("C:\\Users\\Test\\file.txt"));
        assert!(!set.excludes_directory("C:\\Users\\Test"));
    }
}
