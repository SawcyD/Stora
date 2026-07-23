//! Windows-specific integration: volumes, the Recycle Bin, file metadata,
//! lock detection, and system appearance.
//!
//! Every module compiles on non-Windows hosts with a stub so the rest of the
//! workspace stays testable in CI.

pub mod credentials;
pub mod fileinfo;
pub mod known_folders;
pub mod recycle;
pub mod restore;
pub mod system;
pub mod volumes;

pub use credentials::{
    delete_advisor_api_key, has_advisor_api_key, read_advisor_api_key, save_advisor_api_key,
};
pub use fileinfo::{allocated_size, estimate_allocated, processes_locking};
pub use known_folders::redirect as redirect_known_folder;
pub use recycle::move_to_recycle_bin;
pub use restore::{create_restore_point, winget_available, RestoreFailure};
pub use system::{accent_color, expand_environment};
pub use volumes::{drive_for_path, enumerate_drives};
