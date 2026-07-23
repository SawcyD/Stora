use std::collections::{HashMap, HashSet};
use std::fs::Metadata;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use stora_core::model::{FileEntry, FolderAggregate, ScanOptions, ScanState, StorageCategory};
use stora_core::{Result, StoraError, TaskControl};
use stora_security::ExclusionSet;

use crate::category;

/// Windows `FILE_ATTRIBUTE_*` values we care about.
const ATTR_HIDDEN: u32 = 0x2;
const ATTR_SYSTEM: u32 = 0x4;
const ATTR_REPARSE_POINT: u32 = 0x400;

/// How often the walker reports progress. Tuned to land inside the 4–10
/// updates/second target without flooding the UI thread.
const PROGRESS_INTERVAL_MS: u64 = 150;

/// How many records accumulate before a batched database write.
const BATCH_SIZE: usize = 4_000;

/// Only files at least this large get an individual database row.
///
/// Nothing in the interface lists files below this size: folder totals come
/// from `folder_aggregates` and the breakdown from accumulated category
/// totals. Storing every small file added hundreds of megabytes per scan for
/// rows no view ever read.
const ENTRY_SIZE_FLOOR: u64 = 1024 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub struct ScanTotals {
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
    pub errors: u64,
}

/// Callbacks the host supplies. Keeping these as traits rather than channels
/// lets the walker stay synchronous and easy to test.
pub trait ScanSink {
    /// Called with a batch of file records. Implementations should persist
    /// them and return quickly.
    fn entries(&mut self, entries: &[(FileEntry, StorageCategory)]) -> Result<()>;

    /// Called with folder totals as each directory finishes.
    fn aggregates(&mut self, aggregates: &[FolderAggregate]) -> Result<()>;

    /// Called once at the end with the totals for every storage category.
    ///
    /// These are accumulated across every file seen, including the small ones
    /// that never get an individual row.
    fn categories(&mut self, totals: &[(StorageCategory, u64, u64)]) -> Result<()>;

    /// Called for a path that could not be read. Scanning continues.
    fn error(&mut self, path: &str, error: &StoraError);

    /// Called at most every `PROGRESS_INTERVAL_MS`, plus once immediately at
    /// the start so the interface never sits on an empty readout.
    fn progress(
        &mut self,
        state: ScanState,
        totals: ScanTotals,
        current_path: &str,
        elapsed_ms: u64,
    );
}

/// One directory in flight, holding the totals accumulated from its children.
struct Frame {
    path: String,
    name: String,
    parent_path: Option<String>,
    /// Keep directory enumeration lazy. Collecting every entry before the
    /// walker sees the first one can make a very large folder look frozen and
    /// prevents pause/cancel from being checked during enumeration.
    children: std::fs::ReadDir,
    logical_size: u64,
    allocated_size: u64,
    file_count: u64,
    folder_count: u64,
    modified: Option<i64>,
    /// File identity of this directory, used to detect reparse-point loops.
    identity: Option<u64>,
}

pub struct Walker<'a, S: ScanSink> {
    options: ScanOptions,
    exclusions: &'a ExclusionSet,
    control: &'a TaskControl,
    sink: &'a mut S,

    entry_batch: Vec<(FileEntry, StorageCategory)>,
    aggregate_batch: Vec<FolderAggregate>,
    totals: ScanTotals,
    /// Running per-category totals: (bytes, file count).
    category_totals: HashMap<StorageCategory, (u64, u64)>,

    /// Identities of directories on the current descent path. Only consulted
    /// when the user has opted into following reparse points.
    ancestry: HashSet<u64>,

    started: Instant,
    last_progress: Instant,
}

impl<'a, S: ScanSink> Walker<'a, S> {
    pub fn new(
        options: ScanOptions,
        exclusions: &'a ExclusionSet,
        control: &'a TaskControl,
        sink: &'a mut S,
    ) -> Self {
        Self {
            options,
            exclusions,
            control,
            sink,
            entry_batch: Vec::with_capacity(BATCH_SIZE),
            aggregate_batch: Vec::with_capacity(256),
            totals: ScanTotals::default(),
            category_totals: HashMap::new(),
            ancestry: HashSet::new(),
            started: Instant::now(),
            last_progress: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Walks the configured root to completion.
    ///
    /// Uses an explicit stack rather than recursion so a deeply nested tree
    /// cannot overflow, and so folder totals roll up in a single pass.
    pub fn run(&mut self) -> Result<ScanTotals> {
        let root = self.options.root.clone();
        let mut stack: Vec<Frame> = Vec::new();

        // Report before the first directory is even read. Enumerating a large
        // volume root can take a moment, and without this the interface shows
        // an all-zero readout that looks indistinguishable from a hang.
        self.report(ScanState::Scanning, &root);

        match self.open_frame(&root, None) {
            Ok(Some(frame)) => stack.push(frame),
            Ok(None) => return Ok(self.totals),
            Err(err) => {
                self.sink.error(&root, &err);
                self.totals.errors += 1;
                return Err(err);
            }
        }

        while let Some(frame) = stack.last_mut() {
            // Cancellation and pause are handled per directory entry, which
            // keeps the response well under the one-second target.
            self.control.checkpoint()?;

            let Some(next_entry) = frame.children.next() else {
                // Directory exhausted: finalize it and fold into its parent.
                let finished = stack.pop().expect("frame present");
                self.finish_frame(finished, &mut stack)?;
                continue;
            };

            let dir_entry = match next_entry {
                Ok(entry) => entry,
                Err(err) => {
                    let path = frame.path.clone();
                    let error = StoraError::from_io(&err, &path);
                    self.sink.error(&path, &error);
                    self.totals.errors += 1;
                    self.maybe_report(ScanState::Scanning, &path);
                    continue;
                }
            };

            let child_path = dir_entry.path().to_string_lossy().replace('/', "\\");

            // `symlink_metadata` does not traverse links, so a junction is
            // reported as itself rather than its target.
            let metadata = match dir_entry.metadata() {
                Ok(metadata) => metadata,
                Err(err) => {
                    let error = StoraError::from_io(&err, &child_path);
                    self.sink.error(&child_path, &error);
                    self.totals.errors += 1;
                    continue;
                }
            };

            if self.should_skip(&metadata) {
                continue;
            }

            if metadata.is_dir() {
                if self.exclusions.excludes_directory(&child_path) {
                    continue;
                }
                if is_reparse_point(&metadata) && !self.follows_reparse_points() {
                    // Record the link itself so its existence is visible, but
                    // do not descend or count the target's bytes twice.
                    self.push_entry(&child_path, &metadata, true)?;
                    frame.folder_count += 1;
                    continue;
                }

                match self.open_frame(&child_path, Some(frame.path.clone())) {
                    Ok(Some(child_frame)) => {
                        if let Some(identity) = child_frame.identity {
                            if !self.ancestry.insert(identity) {
                                let error = StoraError::ReparseLoop {
                                    path: child_path.clone(),
                                };
                                self.sink.error(&child_path, &error);
                                self.totals.errors += 1;
                                continue;
                            }
                        }
                        stack.push(child_frame);
                    }
                    Ok(None) => {}
                    Err(err) => {
                        self.sink.error(&child_path, &err);
                        self.totals.errors += 1;
                    }
                }
            } else {
                if self.exclusions.excludes_file(&child_path) {
                    continue;
                }
                let (logical, allocated) = self.sizes_of(&child_path, &metadata);
                self.push_entry(&child_path, &metadata, false)?;

                frame.logical_size += logical;
                frame.allocated_size += allocated;
                frame.file_count += 1;
                self.totals.files += 1;
                self.totals.bytes += allocated;

                self.maybe_report(ScanState::Scanning, &child_path);
            }
        }

        self.flush()?;
        let root = self.options.root.clone();
        self.report(ScanState::Completed, &root);
        Ok(self.totals)
    }

    /// Reads a directory and prepares its frame.
    ///
    /// Returns `Ok(None)` when the directory should be skipped silently.
    fn open_frame(&mut self, path: &str, parent_path: Option<String>) -> Result<Option<Frame>> {
        let extended = stora_security::to_extended_length(path);

        let metadata =
            std::fs::symlink_metadata(&extended).map_err(|err| StoraError::from_io(&err, path))?;

        let read = std::fs::read_dir(&extended).map_err(|err| StoraError::from_io(&err, path))?;

        Ok(Some(Frame {
            name: stora_security::file_name_of(path),
            parent_path,
            children: read,
            logical_size: 0,
            allocated_size: 0,
            file_count: 0,
            folder_count: 0,
            modified: timestamp(metadata.modified().ok()),
            // Resolving the real target costs a syscall per directory, so only
            // pay it when the user has opted into following reparse points —
            // otherwise a loop is impossible by construction.
            identity: self
                .follows_reparse_points()
                .then(|| directory_identity(path))
                .flatten(),
            path: path.to_string(),
        }))
    }

    /// Emits a completed directory's aggregate and folds it into its parent.
    fn finish_frame(&mut self, frame: Frame, stack: &mut [Frame]) -> Result<()> {
        if let Some(identity) = frame.identity {
            self.ancestry.remove(&identity);
        }

        self.totals.folders += 1;

        if let Some(parent) = stack.last_mut() {
            parent.logical_size += frame.logical_size;
            parent.allocated_size += frame.allocated_size;
            parent.file_count += frame.file_count;
            parent.folder_count += frame.folder_count + 1;
        }

        self.aggregate_batch.push(FolderAggregate {
            path: frame.path.clone(),
            parent_path: frame.parent_path,
            name: frame.name,
            logical_size: frame.logical_size,
            allocated_size: frame.allocated_size,
            file_count: frame.file_count,
            folder_count: frame.folder_count,
            modified: frame.modified,
            has_children: frame.folder_count > 0,
        });

        if self.aggregate_batch.len() >= 256 {
            let batch = std::mem::take(&mut self.aggregate_batch);
            self.sink.aggregates(&batch)?;
        }

        self.maybe_report(ScanState::Scanning, &frame.path);
        Ok(())
    }

    fn push_entry(&mut self, path: &str, metadata: &Metadata, is_directory: bool) -> Result<()> {
        let (logical, allocated) = self.sizes_of(path, metadata);
        let category = category::classify(path);

        // Every file counts toward its category, however small.
        if !is_directory {
            let totals = self.category_totals.entry(category).or_insert((0, 0));
            totals.0 += allocated;
            totals.1 += 1;
        }

        // Only files worth listing individually get a row. Reparse points are
        // always recorded so their existence stays visible.
        let is_reparse = is_reparse_point(metadata);
        if !is_directory && !is_reparse && logical < ENTRY_SIZE_FLOOR {
            return Ok(());
        }

        let entry = FileEntry {
            parent_path: stora_security::parent_of(path).unwrap_or_default(),
            name: stora_security::file_name_of(path),
            extension: if is_directory {
                None
            } else {
                stora_security::extension_of(path)
            },
            logical_size: logical,
            allocated_size: allocated,
            created: timestamp(metadata.created().ok()),
            modified: timestamp(metadata.modified().ok()),
            accessed: timestamp(metadata.accessed().ok()),
            attributes: attributes_of(metadata),
            is_directory,
            is_reparse_point: is_reparse,
            path: path.to_string(),
        };

        self.entry_batch.push((entry, category));

        if self.entry_batch.len() >= BATCH_SIZE {
            let batch = std::mem::take(&mut self.entry_batch);
            self.sink.entries(&batch)?;
        }
        Ok(())
    }

    fn sizes_of(&self, path: &str, metadata: &Metadata) -> (u64, u64) {
        let logical = metadata.len();
        let allocated = if self.options.use_allocated_size {
            stora_winapi::allocated_size(path, logical)
        } else {
            logical
        };
        (logical, allocated)
    }

    fn should_skip(&self, metadata: &Metadata) -> bool {
        let attributes = attributes_of(metadata);
        if !self.options.scan_hidden && attributes & ATTR_HIDDEN != 0 {
            return true;
        }
        if !self.options.scan_system && attributes & ATTR_SYSTEM != 0 {
            return true;
        }
        false
    }

    fn follows_reparse_points(&self) -> bool {
        self.options.follow_symlinks || self.options.follow_junctions
    }

    fn maybe_report(&mut self, state: ScanState, current_path: &str) {
        if self.last_progress.elapsed().as_millis() as u64 >= PROGRESS_INTERVAL_MS {
            self.report(state, current_path);
        }
    }

    fn report(&mut self, state: ScanState, current_path: &str) {
        self.last_progress = Instant::now();
        let elapsed = self.elapsed_ms();
        self.sink
            .progress(state, self.totals, current_path, elapsed);
    }

    /// Writes whatever is left in the batches. Always call before finishing,
    /// including on the cancellation path, so partial results are usable.
    pub fn flush(&mut self) -> Result<()> {
        if !self.entry_batch.is_empty() {
            let batch = std::mem::take(&mut self.entry_batch);
            self.sink.entries(&batch)?;
        }
        if !self.aggregate_batch.is_empty() {
            let batch = std::mem::take(&mut self.aggregate_batch);
            self.sink.aggregates(&batch)?;
        }
        if !self.category_totals.is_empty() {
            let totals: Vec<(StorageCategory, u64, u64)> = self
                .category_totals
                .iter()
                .map(|(category, (bytes, count))| (*category, *bytes, *count))
                .collect();
            self.sink.categories(&totals)?;
        }
        Ok(())
    }

    pub fn totals(&self) -> ScanTotals {
        self.totals
    }
}

fn timestamp(time: Option<SystemTime>) -> Option<i64> {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
}

fn attributes_of(metadata: &Metadata) -> u32 {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes()
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        0
    }
}

fn is_reparse_point(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        attributes_of(metadata) & ATTR_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// A stable identifier for a directory, used to detect reparse-point loops.
///
/// `canonicalize` resolves junctions and symlinks to their real target, so two
/// different link paths that point at the same directory hash identically.
/// Returns `None` when the path cannot be resolved, in which case the walker
/// falls back to simply not descending through reparse points.
fn directory_identity(path: &str) -> Option<u64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let resolved = std::fs::canonicalize(stora_security::to_extended_length(path)).ok()?;
    let mut hasher = DefaultHasher::new();
    resolved.hash(&mut hasher);
    Some(hasher.finish())
}
