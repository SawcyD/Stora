//! Tauri command surface.
//!
//! Every command is small and typed. There is deliberately no general-purpose
//! "delete this path" or "run this command" entry point: deletion is only
//! reachable by approving indices into a plan the backend generated.

pub mod advanced;
pub mod apps;
pub mod cleanup;
pub mod developer;
pub mod knowledge;
pub mod scan;
pub mod settings;
pub mod storage;
