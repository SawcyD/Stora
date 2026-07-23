//! Exact duplicate detection using a staged pipeline.
//!
//! Cost discipline is the whole design: grouping by size costs nothing,
//! sampling reads 16 KB per file, and only the survivors are read in full.
//! Nothing is presented as a duplicate until two files agree on a full
//! SHA-256.
//!
//! Hard links are detected and never offered for removal — two names for one
//! file are not two copies, and deleting one frees no space.

pub mod finder;
pub mod hash;

pub use finder::{
    find, selection_for, Candidate, DuplicateFile, DuplicateGroup, DuplicateReport, KeepStrategy,
    DEFAULT_MINIMUM_BYTES,
};
pub use hash::{file_identity, full_hash, sample_hash, FileIdentity};
