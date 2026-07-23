//! Filesystem traversal with progress reporting, cancellation, and folder
//! aggregation.
//!
//! The walker is synchronous and single-threaded by design: Stora prioritizes
//! leaving the system responsive over maximum throughput, and a single ordered
//! pass makes folder totals exact without a second aggregation phase.

pub mod category;
pub mod walker;

pub use category::classify;
pub use walker::{ScanSink, ScanTotals, Walker};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use stora_core::model::{
        Exclusion, ExclusionKind, ExclusionReason, FileEntry, FolderAggregate, ScanOptions,
        ScanState, StorageCategory,
    };
    use stora_core::{StoraError, TaskControl};
    use stora_security::ExclusionSet;

    #[derive(Default)]
    struct RecordingSink {
        entries: Vec<FileEntry>,
        aggregates: Vec<FolderAggregate>,
        errors: Vec<(String, String)>,
        category_totals: Vec<(StorageCategory, u64, u64)>,
        progress_calls: usize,
        /// (state, files so far, current path, elapsed) for each report.
        progress_log: Vec<(ScanState, u64, String, u64)>,
        /// Test hook: pause inside each report so elapsed time is measurable.
        stall_each_report_ms: u64,
    }

    impl ScanSink for RecordingSink {
        fn entries(&mut self, entries: &[(FileEntry, StorageCategory)]) -> stora_core::Result<()> {
            self.entries
                .extend(entries.iter().map(|(entry, _)| entry.clone()));
            Ok(())
        }

        fn aggregates(&mut self, aggregates: &[FolderAggregate]) -> stora_core::Result<()> {
            self.aggregates.extend_from_slice(aggregates);
            Ok(())
        }

        fn categories(&mut self, totals: &[(StorageCategory, u64, u64)]) -> stora_core::Result<()> {
            self.category_totals = totals.to_vec();
            Ok(())
        }

        fn error(&mut self, path: &str, error: &StoraError) {
            self.errors
                .push((path.to_string(), error.code().to_string()));
        }

        fn progress(
            &mut self,
            state: ScanState,
            totals: ScanTotals,
            current_path: &str,
            elapsed_ms: u64,
        ) {
            self.progress_calls += 1;
            // Lets a test guarantee that measurable time passes mid-scan.
            if self.stall_each_report_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(self.stall_each_report_ms));
            }
            self.progress_log
                .push((state, totals.files, current_path.to_string(), elapsed_ms));
        }
    }

    fn options_for(root: &Path) -> ScanOptions {
        ScanOptions {
            root: root.to_string_lossy().replace('/', "\\"),
            scan_hidden: true,
            scan_system: true,
            // Per-file allocation queries are a Windows call; logical sizes
            // keep these tests deterministic across hosts.
            use_allocated_size: false,
            ..Default::default()
        }
    }

    fn run(root: &Path, exclusions: ExclusionSet) -> (RecordingSink, ScanTotals) {
        let control = TaskControl::new();
        let mut sink = RecordingSink::default();
        let totals = {
            let mut walker = Walker::new(options_for(root), &exclusions, &control, &mut sink);
            let totals = walker.run().expect("scan completes");
            walker.flush().unwrap();
            totals
        };
        (sink, totals)
    }

    fn aggregate_for<'a>(sink: &'a RecordingSink, path: &Path) -> &'a FolderAggregate {
        let wanted = path.to_string_lossy().replace('/', "\\");
        sink.aggregates
            .iter()
            .find(|aggregate| aggregate.path.eq_ignore_ascii_case(&wanted))
            .unwrap_or_else(|| panic!("no aggregate for {wanted}"))
    }

    /// Comfortably above the walker's per-file storage floor.
    const LARGE: usize = 2 * 1024 * 1024;

    #[test]
    fn counts_files_and_sums_sizes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.bin"), vec![0u8; 1000]).unwrap();
        fs::write(dir.path().join("b.bin"), vec![0u8; 2000]).unwrap();

        let (_, totals) = run(dir.path(), ExclusionSet::default());

        assert_eq!(totals.files, 2);
        assert_eq!(totals.bytes, 3000);
    }

    #[test]
    fn reports_progress_before_reading_the_first_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.bin"), b"data").unwrap();

        let (sink, _) = run(dir.path(), ExclusionSet::default());

        // Enumerating a large volume root takes time. Without an immediate
        // first report the interface shows an all-zero readout that is
        // indistinguishable from a hang.
        let first = sink.progress_log.first().expect("an opening report");
        assert_eq!(first.0, ScanState::Scanning);
        assert_eq!(first.1, 0, "nothing has been counted yet");
        assert_eq!(
            first.2,
            dir.path().to_string_lossy().replace('/', "\\"),
            "the opening report names the scan root"
        );
    }

    #[test]
    fn the_final_report_carries_a_real_elapsed_time() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            fs::write(dir.path().join(format!("f{i}.bin")), vec![0u8; 2048]).unwrap();
        }

        let control = TaskControl::new();
        let exclusions = ExclusionSet::default();
        let mut sink = RecordingSink {
            // Guarantees the clock advances between the opening and closing
            // reports, without depending on how fast the filesystem is.
            stall_each_report_ms: 6,
            ..Default::default()
        };
        {
            let mut walker = Walker::new(options_for(dir.path()), &exclusions, &control, &mut sink);
            walker.run().expect("scan completes");
        }

        let last = sink.progress_log.last().expect("a closing report");
        assert_eq!(last.0, ScanState::Completed);
        assert_eq!(last.1, 5);

        // A hardcoded zero here previously froze the on-screen timer at 00:00,
        // leaving no way to tell a working scan from a stuck one.
        assert!(
            last.3 > 0,
            "elapsed time must be read from the clock, got {}",
            last.3
        );
    }

    #[test]
    fn only_files_above_the_size_floor_get_individual_records() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("small.bin"), vec![0u8; 4096]).unwrap();
        fs::write(dir.path().join("large.bin"), vec![0u8; LARGE]).unwrap();

        let (sink, totals) = run(dir.path(), ExclusionSet::default());

        // Both files count toward the totals...
        assert_eq!(totals.files, 2);
        assert_eq!(totals.bytes, 4096 + LARGE as u64);

        // ...but only the large one is worth a row. Storing every small file
        // added hundreds of megabytes per scan for rows nothing reads.
        assert_eq!(sink.entries.len(), 1);
        assert_eq!(sink.entries[0].name, "large.bin");
    }

    #[test]
    fn category_totals_include_files_below_the_storage_floor() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tiny.bin"), vec![0u8; 512]).unwrap();
        fs::write(dir.path().join("also-tiny.bin"), vec![0u8; 256]).unwrap();

        let (sink, _) = run(dir.path(), ExclusionSet::default());

        let counted: u64 = sink.category_totals.iter().map(|(_, _, count)| count).sum();
        let bytes: u64 = sink.category_totals.iter().map(|(_, bytes, _)| bytes).sum();

        assert_eq!(counted, 2, "small files must still be counted");
        assert_eq!(bytes, 768);
        assert!(
            sink.entries.is_empty(),
            "neither file is large enough to store individually"
        );
    }

    #[test]
    fn rolls_child_totals_up_into_ancestors() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        let deepest = inner.join("deepest");
        fs::create_dir_all(&deepest).unwrap();

        fs::write(dir.path().join("root.bin"), vec![0u8; 100]).unwrap();
        fs::write(inner.join("inner.bin"), vec![0u8; 200]).unwrap();
        fs::write(deepest.join("deep.bin"), vec![0u8; 400]).unwrap();

        let (sink, totals) = run(dir.path(), ExclusionSet::default());

        assert_eq!(totals.files, 3);
        assert_eq!(totals.bytes, 700);

        let root = aggregate_for(&sink, dir.path());
        assert_eq!(root.logical_size, 700, "root must include all descendants");
        assert_eq!(root.file_count, 3);
        assert_eq!(root.folder_count, 2, "inner and deepest");

        let inner_aggregate = aggregate_for(&sink, &inner);
        assert_eq!(inner_aggregate.logical_size, 600);
        assert_eq!(inner_aggregate.file_count, 2);

        let deepest_aggregate = aggregate_for(&sink, &deepest);
        assert_eq!(deepest_aggregate.logical_size, 400);
        assert_eq!(deepest_aggregate.folder_count, 0);
    }

    #[test]
    fn records_parent_links_for_the_tree_view() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        fs::create_dir(&inner).unwrap();
        fs::write(inner.join("f.bin"), b"x").unwrap();

        let (sink, _) = run(dir.path(), ExclusionSet::default());

        let root = aggregate_for(&sink, dir.path());
        assert!(root.parent_path.is_none(), "the scan root has no parent");

        let inner_aggregate = aggregate_for(&sink, &inner);
        assert_eq!(
            inner_aggregate.parent_path.as_deref(),
            Some(dir.path().to_string_lossy().replace('/', "\\").as_str())
        );
    }

    #[test]
    fn empty_directory_produces_a_zeroed_aggregate() {
        let dir = tempfile::tempdir().unwrap();
        let (sink, totals) = run(dir.path(), ExclusionSet::default());

        assert_eq!(totals.files, 0);
        assert_eq!(totals.bytes, 0);
        assert_eq!(totals.folders, 1, "the root itself still counts");

        let root = aggregate_for(&sink, dir.path());
        assert_eq!(root.logical_size, 0);
        assert!(!root.has_children);
    }

    #[test]
    fn excluded_folders_and_their_contents_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir(&vault).unwrap();
        // Large enough that it would definitely be recorded if not excluded,
        // so the assertion below proves the exclusion rather than the floor.
        fs::write(vault.join("secret.bin"), vec![0u8; LARGE]).unwrap();
        fs::write(dir.path().join("keep.bin"), vec![0u8; 100]).unwrap();

        let exclusions = ExclusionSet::from_rules(&[Exclusion {
            id: 0,
            pattern: vault.to_string_lossy().replace('/', "\\"),
            kind: ExclusionKind::Folder,
            reason: ExclusionReason::UserExclusion,
            created_at: 0,
        }]);

        let (sink, totals) = run(dir.path(), exclusions);

        assert_eq!(totals.files, 1);
        assert_eq!(totals.bytes, 100);
        assert!(
            !sink.entries.iter().any(|e| e.name == "secret.bin"),
            "excluded content must never be recorded"
        );
    }

    #[test]
    fn excluded_extensions_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("keep.bin"), vec![0u8; 100]).unwrap();
        fs::write(dir.path().join("skip.psd"), vec![0u8; 900]).unwrap();

        let exclusions = ExclusionSet::from_rules(&[Exclusion {
            id: 0,
            pattern: "psd".into(),
            kind: ExclusionKind::Extension,
            reason: ExclusionReason::UserExclusion,
            created_at: 0,
        }]);

        let (_, totals) = run(dir.path(), exclusions);
        assert_eq!(totals.files, 1);
        assert_eq!(totals.bytes, 100);
    }

    #[test]
    fn cancellation_stops_the_walk_and_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..50 {
            fs::write(dir.path().join(format!("f{i}.bin")), b"data").unwrap();
        }

        let control = TaskControl::new();
        control.cancel();
        let exclusions = ExclusionSet::default();

        let mut sink = RecordingSink::default();
        let mut walker = Walker::new(options_for(dir.path()), &exclusions, &control, &mut sink);

        let err = walker.run().expect_err("a cancelled scan returns an error");
        assert_eq!(err.code(), "ScanCancelled");

        // Partial results must still be persistable.
        walker.flush().expect("flush after cancellation");
    }

    #[test]
    fn unreadable_root_is_reported_rather_than_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");

        let control = TaskControl::new();
        let exclusions = ExclusionSet::default();
        let mut sink = RecordingSink::default();
        let mut walker = Walker::new(options_for(&missing), &exclusions, &control, &mut sink);

        let err = walker.run().expect_err("missing root fails");
        assert_eq!(err.code(), "PathNotFound");
        assert_eq!(sink.errors.len(), 1);
    }

    #[test]
    fn handles_unicode_names() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("日本語");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("café.txt"), vec![0u8; LARGE]).unwrap();

        let (sink, totals) = run(dir.path(), ExclusionSet::default());

        assert_eq!(totals.files, 1);
        assert_eq!(totals.bytes, LARGE as u64);
        assert!(sink.entries.iter().any(|entry| entry.name == "café.txt"));
    }

    #[test]
    fn handles_deeply_nested_trees_without_overflowing() {
        let dir = tempfile::tempdir().unwrap();
        let mut path = dir.path().to_path_buf();
        for i in 0..80 {
            path = path.join(format!("level{i}"));
        }
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("deep.bin"), vec![0u8; 64]).unwrap();

        let (_, totals) = run(dir.path(), ExclusionSet::default());
        assert_eq!(totals.files, 1);
        assert_eq!(totals.bytes, 64);
        assert_eq!(totals.folders, 81, "80 levels plus the root");
    }

    #[test]
    fn classifies_entries_while_walking() {
        assert_eq!(
            classify("C:\\Users\\Test\\Documents\\a.docx"),
            StorageCategory::Documents
        );
    }
}
