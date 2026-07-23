use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use stora_core::{Result, TaskControl};

use crate::hash::{self, FileIdentity};

/// Files below this size are never worth reporting.
pub const DEFAULT_MINIMUM_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified: Option<i64>,
    /// True when this path is a hard link to another entry in the same group.
    pub is_hard_link: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub hash: String,
    pub size: u64,
    pub files: Vec<DuplicateFile>,
    /// Space freed by keeping exactly one copy. Hard links contribute nothing.
    pub reclaimable_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateReport {
    pub groups: Vec<DuplicateGroup>,
    pub total_reclaimable: u64,
    pub files_compared: u64,
    pub files_fully_hashed: u64,
}

/// A candidate file supplied by the caller.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: String,
    pub size: u64,
    pub modified: Option<i64>,
}

/// Finds exact duplicates using a staged pipeline.
///
/// 1. Group by size — files of different lengths cannot be identical.
/// 2. Drop groups with one member, and anything below the minimum size.
/// 3. Hash a sample from each end to eliminate near-misses cheaply.
/// 4. Fully hash the survivors with SHA-256 and verify.
/// 5. Detect hard links so the same file under two names is not reported as a
///    duplicate that could free space.
///
/// The staging matters: full-hashing every same-size file on a large drive
/// would read hundreds of gigabytes for a result that is mostly negative.
pub fn find(
    candidates: &[Candidate],
    minimum_bytes: u64,
    control: &TaskControl,
) -> Result<DuplicateReport> {
    let mut by_size: HashMap<u64, Vec<&Candidate>> = HashMap::new();
    for candidate in candidates {
        if candidate.size < minimum_bytes || candidate.size == 0 {
            continue;
        }
        by_size.entry(candidate.size).or_default().push(candidate);
    }

    let mut groups = Vec::new();
    let mut files_compared = 0u64;
    let mut files_fully_hashed = 0u64;

    for (size, same_size) in by_size {
        control.checkpoint()?;

        if same_size.len() < 2 {
            continue;
        }
        files_compared += same_size.len() as u64;

        // Stage 3: cheap sampling.
        let mut by_sample: HashMap<u64, Vec<&Candidate>> = HashMap::new();
        for candidate in same_size {
            control.checkpoint()?;
            // An unreadable file is skipped rather than failing the run.
            let Ok(sample) = hash::sample_hash(&candidate.path, size) else {
                continue;
            };
            by_sample.entry(sample).or_default().push(candidate);
        }

        // Stage 4: full verification.
        for (_, sampled) in by_sample {
            control.checkpoint()?;
            if sampled.len() < 2 {
                continue;
            }

            let mut by_hash: HashMap<String, Vec<&Candidate>> = HashMap::new();
            for candidate in sampled {
                control.checkpoint()?;
                let Ok(digest) = hash::full_hash(&candidate.path) else {
                    continue;
                };
                files_fully_hashed += 1;
                by_hash.entry(digest).or_default().push(candidate);
            }

            for (digest, verified) in by_hash {
                if verified.len() < 2 {
                    continue;
                }
                groups.push(build_group(digest, size, &verified));
            }
        }
    }

    // Largest opportunity first.
    groups.sort_by(|a, b| b.reclaimable_bytes.cmp(&a.reclaimable_bytes));
    let total_reclaimable = groups.iter().map(|g| g.reclaimable_bytes).sum();

    Ok(DuplicateReport {
        groups,
        total_reclaimable,
        files_compared,
        files_fully_hashed,
    })
}

/// Builds a group, marking hard links and computing what can really be freed.
fn build_group(hash: String, size: u64, verified: &[&Candidate]) -> DuplicateGroup {
    let mut seen_identities: Vec<FileIdentity> = Vec::new();
    let mut files = Vec::with_capacity(verified.len());
    // Distinct physical copies. Only these can free space.
    let mut distinct_copies = 0u64;

    for candidate in verified {
        let identity = hash::file_identity(&candidate.path);

        let is_hard_link = match identity {
            Some(id) => {
                if seen_identities.contains(&id) {
                    true
                } else {
                    seen_identities.push(id);
                    distinct_copies += 1;
                    false
                }
            }
            None => {
                // Without an identity, assume it is a distinct copy — the
                // conservative choice is to report the file rather than hide
                // it, but the user still decides.
                distinct_copies += 1;
                false
            }
        };

        files.push(DuplicateFile {
            path: candidate.path.clone(),
            name: stora_security::file_name_of(&candidate.path),
            size,
            modified: candidate.modified,
            is_hard_link,
        });
    }

    // Keeping one copy frees the rest. Hard links free nothing at all.
    let reclaimable_bytes = size * distinct_copies.saturating_sub(1);

    // Shortest path first: it is usually the original.
    files.sort_by(|a, b| a.path.len().cmp(&b.path.len()));

    DuplicateGroup {
        hash,
        size,
        files,
        reclaimable_bytes,
    }
}

/// A rule for choosing which copy to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeepStrategy {
    Newest,
    Oldest,
    ShortestPath,
}

/// Returns the indices a strategy would select *for removal*.
///
/// Exactly one copy is always kept, and hard links are never selected: they
/// point at a file another entry already covers, so removing one frees
/// nothing while still losing a name someone may rely on.
pub fn selection_for(group: &DuplicateGroup, strategy: KeepStrategy) -> Vec<usize> {
    let removable: Vec<usize> = group
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| !file.is_hard_link)
        .map(|(index, _)| index)
        .collect();

    if removable.len() < 2 {
        return Vec::new();
    }

    let keep = match strategy {
        KeepStrategy::Newest => removable
            .iter()
            .copied()
            .max_by_key(|&index| group.files[index].modified.unwrap_or(i64::MIN)),
        KeepStrategy::Oldest => removable
            .iter()
            .copied()
            .min_by_key(|&index| group.files[index].modified.unwrap_or(i64::MAX)),
        KeepStrategy::ShortestPath => removable
            .iter()
            .copied()
            .min_by_key(|&index| group.files[index].path.len()),
    };

    let Some(keep) = keep else {
        return Vec::new();
    };

    removable
        .into_iter()
        .filter(|&index| index != keep)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &std::path::Path, name: &str, contents: &[u8]) -> Candidate {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
        Candidate {
            path: path.to_string_lossy().replace('/', "\\"),
            size: contents.len() as u64,
            modified: Some(0),
        }
    }

    fn payload(byte: u8) -> Vec<u8> {
        vec![byte; 2 * 1024 * 1024]
    }

    #[test]
    fn identical_files_are_grouped() {
        let dir = tempfile::tempdir().unwrap();
        let data = payload(1);
        let candidates = vec![
            write(dir.path(), "a.bin", &data),
            write(dir.path(), "b.bin", &data),
        ];

        let report = find(&candidates, DEFAULT_MINIMUM_BYTES, &TaskControl::new()).unwrap();

        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].files.len(), 2);
        assert_eq!(report.groups[0].reclaimable_bytes, data.len() as u64);
        assert_eq!(report.total_reclaimable, data.len() as u64);
    }

    #[test]
    fn files_of_different_sizes_are_never_compared() {
        let dir = tempfile::tempdir().unwrap();
        let candidates = vec![
            write(dir.path(), "a.bin", &payload(1)),
            write(dir.path(), "b.bin", &vec![1u8; 3 * 1024 * 1024]),
        ];

        let report = find(&candidates, DEFAULT_MINIMUM_BYTES, &TaskControl::new()).unwrap();
        assert!(report.groups.is_empty());
        assert_eq!(
            report.files_fully_hashed, 0,
            "different sizes must not reach the hashing stage"
        );
    }

    #[test]
    fn same_size_but_different_content_is_not_a_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let candidates = vec![
            write(dir.path(), "a.bin", &payload(1)),
            write(dir.path(), "b.bin", &payload(2)),
        ];

        let report = find(&candidates, DEFAULT_MINIMUM_BYTES, &TaskControl::new()).unwrap();
        assert!(report.groups.is_empty());
    }

    #[test]
    fn files_below_the_minimum_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let candidates = vec![
            write(dir.path(), "a.bin", b"tiny"),
            write(dir.path(), "b.bin", b"tiny"),
        ];

        let report = find(&candidates, DEFAULT_MINIMUM_BYTES, &TaskControl::new()).unwrap();
        assert!(report.groups.is_empty());
    }

    #[test]
    fn empty_files_are_never_reported() {
        let dir = tempfile::tempdir().unwrap();
        let candidates = vec![
            write(dir.path(), "a.bin", b""),
            write(dir.path(), "b.bin", b""),
        ];

        let report = find(&candidates, 0, &TaskControl::new()).unwrap();
        assert!(
            report.groups.is_empty(),
            "every empty file matches every other; that is not useful"
        );
    }

    #[test]
    fn three_copies_reclaim_two_of_them() {
        let dir = tempfile::tempdir().unwrap();
        let data = payload(9);
        let candidates = vec![
            write(dir.path(), "a.bin", &data),
            write(dir.path(), "sub/b.bin", &data),
            write(dir.path(), "sub/deep/c.bin", &data),
        ];

        let report = find(&candidates, DEFAULT_MINIMUM_BYTES, &TaskControl::new()).unwrap();
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].reclaimable_bytes, 2 * data.len() as u64);
    }

    #[test]
    fn a_lone_file_is_not_a_group() {
        let dir = tempfile::tempdir().unwrap();
        let candidates = vec![write(dir.path(), "a.bin", &payload(1))];
        let report = find(&candidates, DEFAULT_MINIMUM_BYTES, &TaskControl::new()).unwrap();
        assert!(report.groups.is_empty());
    }

    #[test]
    fn detection_is_cancellable() {
        let dir = tempfile::tempdir().unwrap();
        let data = payload(1);
        let candidates = vec![
            write(dir.path(), "a.bin", &data),
            write(dir.path(), "b.bin", &data),
        ];

        let control = TaskControl::new();
        control.cancel();
        assert!(find(&candidates, DEFAULT_MINIMUM_BYTES, &control).is_err());
    }

    #[test]
    fn an_unreadable_candidate_is_skipped_rather_than_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let data = payload(3);
        let mut candidates = vec![
            write(dir.path(), "a.bin", &data),
            write(dir.path(), "b.bin", &data),
        ];
        candidates.push(Candidate {
            path: "C:\\definitely\\missing.bin".into(),
            size: data.len() as u64,
            modified: None,
        });

        let report = find(&candidates, DEFAULT_MINIMUM_BYTES, &TaskControl::new()).unwrap();
        assert_eq!(report.groups.len(), 1, "the readable pair is still found");
        assert_eq!(report.groups[0].files.len(), 2);
    }

    fn group_with(files: Vec<DuplicateFile>) -> DuplicateGroup {
        DuplicateGroup {
            hash: "abc".into(),
            size: 100,
            reclaimable_bytes: 100 * (files.len().saturating_sub(1)) as u64,
            files,
        }
    }

    fn file(path: &str, modified: i64, is_hard_link: bool) -> DuplicateFile {
        DuplicateFile {
            path: path.into(),
            name: stora_security::file_name_of(path),
            size: 100,
            modified: Some(modified),
            is_hard_link,
        }
    }

    #[test]
    fn every_strategy_keeps_exactly_one_copy() {
        let group = group_with(vec![
            file("C:\\a\\one.bin", 100, false),
            file("C:\\bb\\two.bin", 300, false),
            file("C:\\ccc\\three.bin", 200, false),
        ]);

        for strategy in [
            KeepStrategy::Newest,
            KeepStrategy::Oldest,
            KeepStrategy::ShortestPath,
        ] {
            let selection = selection_for(&group, strategy);
            assert_eq!(
                selection.len(),
                2,
                "{strategy:?} must leave exactly one copy"
            );
        }
    }

    #[test]
    fn keep_newest_removes_the_older_copies() {
        let group = group_with(vec![
            file("C:\\a.bin", 100, false),
            file("C:\\b.bin", 900, false),
        ]);
        assert_eq!(selection_for(&group, KeepStrategy::Newest), vec![0]);
    }

    #[test]
    fn keep_oldest_removes_the_newer_copies() {
        let group = group_with(vec![
            file("C:\\a.bin", 100, false),
            file("C:\\b.bin", 900, false),
        ]);
        assert_eq!(selection_for(&group, KeepStrategy::Oldest), vec![1]);
    }

    #[test]
    fn keep_shortest_path_prefers_the_likely_original() {
        let group = group_with(vec![
            file("C:\\Videos\\clip.mp4", 100, false),
            file("D:\\Backups\\Old Laptop\\Videos\\clip.mp4", 200, false),
        ]);
        assert_eq!(selection_for(&group, KeepStrategy::ShortestPath), vec![1]);
    }

    #[test]
    fn hard_links_are_never_selected_for_removal() {
        let group = group_with(vec![
            file("C:\\a.bin", 100, false),
            file("C:\\link.bin", 200, true),
            file("C:\\b.bin", 300, false),
        ]);

        for strategy in [
            KeepStrategy::Newest,
            KeepStrategy::Oldest,
            KeepStrategy::ShortestPath,
        ] {
            let selection = selection_for(&group, strategy);
            assert!(
                !selection.contains(&1),
                "{strategy:?} selected a hard link, which frees nothing"
            );
        }
    }

    #[test]
    fn a_group_that_is_only_hard_links_selects_nothing() {
        let group = group_with(vec![
            file("C:\\a.bin", 100, false),
            file("C:\\link.bin", 200, true),
        ]);
        assert!(
            selection_for(&group, KeepStrategy::Newest).is_empty(),
            "there is only one real copy here"
        );
    }

    #[test]
    fn nothing_is_selected_automatically_by_construction() {
        // `selection_for` is only ever called when a user picks a strategy;
        // the report itself carries no selection at all.
        let dir = tempfile::tempdir().unwrap();
        let data = payload(4);
        let candidates = vec![
            write(dir.path(), "a.bin", &data),
            write(dir.path(), "b.bin", &data),
        ];
        let report = find(&candidates, DEFAULT_MINIMUM_BYTES, &TaskControl::new()).unwrap();

        // A group exposes files and totals — never a preselection.
        assert_eq!(report.groups[0].files.len(), 2);
    }
}
