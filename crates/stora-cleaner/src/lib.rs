//! Cleanup category detection, plan building, preview, and execution.
//!
//! The plan is authoritative: the frontend selects indices into a plan the
//! backend generated, and every item is re-checked against the protected-path
//! rules and revalidated on disk immediately before removal.

pub mod categories;
pub mod execute;
pub mod plan;

pub use categories::{all as all_categories, find as find_category};
pub use execute::{execute, CleanupReporter, ExecutionOutcome};
pub use plan::{build as build_plan, default_selection, PlanRequest};
