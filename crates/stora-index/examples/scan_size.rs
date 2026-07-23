//! Measures how much database a real scan produces.
//!
//! Run with: `cargo run -p stora-index --example scan_size -- <path>`
//!
//! Kept in the repo because the per-file storage floor is a deliberate size
//! trade-off, and this is how it gets re-checked when the schema changes.

use stora_core::model::{FileEntry, FolderAggregate, ScanOptions, ScanState, StorageCategory};
use stora_core::TaskControl;
use stora_index::Index;
use stora_scanner::{ScanSink, ScanTotals, Walker};

struct Sink {
    index: Index,
    scan_id: i64,
    stored_entries: usize,
}

impl ScanSink for Sink {
    fn entries(&mut self, entries: &[(FileEntry, StorageCategory)]) -> stora_core::Result<()> {
        self.stored_entries += entries.len();
        self.index.insert_entries(self.scan_id, entries)
    }

    fn aggregates(&mut self, aggregates: &[FolderAggregate]) -> stora_core::Result<()> {
        self.index.insert_aggregates(self.scan_id, aggregates)
    }

    fn categories(&mut self, totals: &[(StorageCategory, u64, u64)]) -> stora_core::Result<()> {
        self.index.insert_category_totals(self.scan_id, totals)
    }

    fn error(&mut self, _path: &str, _error: &stora_core::StoraError) {}

    fn progress(
        &mut self,
        _state: ScanState,
        _totals: ScanTotals,
        _current: &str,
        _elapsed_ms: u64,
    ) {
    }
}

fn main() {
    let root = std::env::args().nth(1).expect("usage: scan_size <path>");
    let temp = tempfile::tempdir().expect("temp dir");
    let db_path = temp.path().join("measure.db");

    let index = Index::open(&db_path).expect("open index");
    let scan_id = index.begin_scan(&root, 0).expect("begin scan");

    let mut sink = Sink {
        index,
        scan_id,
        stored_entries: 0,
    };

    // Pass `allocated` as a second argument to include the per-file
    // size-on-disk query, which is what the app does by default.
    let use_allocated_size = std::env::args().nth(2).as_deref() == Some("allocated");

    let options = ScanOptions {
        root: root.clone(),
        use_allocated_size,
        scan_system: true,
        ..Default::default()
    };

    let control = TaskControl::new();
    let exclusions = stora_security::ExclusionSet::default();
    let totals = {
        let mut walker = Walker::new(options, &exclusions, &control, &mut sink);
        let totals = walker.run().unwrap_or_else(|_| walker.totals());
        walker.flush().expect("flush");
        totals
    };

    // Checkpoint the write-ahead log so the size on disk is the real total.
    sink.index
        .with(|c| {
            c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(|e| stora_core::StoraError::Database(e.to_string()))
        })
        .ok();

    let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    println!("root:            {root}");
    println!("files seen:      {}", totals.files);
    println!("folders seen:    {}", totals.folders);
    println!(
        "data analyzed:   {}",
        stora_core::format_bytes(totals.bytes)
    );
    println!("rows stored:     {}", sink.stored_entries);
    println!("database size:   {}", stora_core::format_bytes(db_bytes));

    if totals.files > 0 {
        let percent = (sink.stored_entries as f64 / totals.files as f64) * 100.0;
        println!("stored / seen:   {percent:.1}%");
        println!(
            "bytes per file:  {:.1}",
            db_bytes as f64 / totals.files as f64
        );
    }
}
