//! Path validation, protected-location rules, and cleanup authorization.
//!
//! Every deletion in Stora passes through this crate. The frontend is treated
//! as untrusted input: it selects from a backend-generated plan and never
//! supplies a path to delete.

pub mod authorize;
pub mod exclusions;
pub mod path;
pub mod protected;

pub use authorize::{authorize_selection, revalidate};
pub use exclusions::ExclusionSet;
pub use path::{extension_of, file_name_of, is_within, normalize, parent_of, to_extended_length};
pub use protected::{ensure_deletable, is_protected, is_sensitive, ProtectionVerdict};
