//! Installed application discovery, footprint attribution, and the
//! "potentially unused" view.
//!
//! Two rules govern this crate. Runtimes, redistributables, and drivers are
//! never suggested for removal, because nothing launches them directly and so
//! they always look idle. And no usage figure is ever presented as a plain
//! "last opened" — the source and confidence are always stated, because
//! Windows has no single reliable value.

pub mod classify;
pub mod discovery;
pub mod footprint;
pub mod model;
pub mod uninstall;
pub mod unused;

pub use classify::infer_type;
pub use discovery::discover;
pub use footprint::build as build_footprint;
pub use model::{
    ActivitySource, AppActivity, AppFootprint, AppType, Confidence, FootprintLocation, InstalledApp,
};
pub use uninstall::{
    choose_method, diff_footprint, registry_leftover, Leftover, RestorePointOutcome,
    UninstallMethod,
};
pub use unused::{describe, matches_filter, UnusedFilter};
