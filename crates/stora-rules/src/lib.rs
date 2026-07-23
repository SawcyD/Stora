//! Automation rules, folder growth history, and local alerts.
//!
//! Two properties hold throughout. A rule is disabled when created and only
//! runs after the user turns it on. And an automated action may only remove
//! categories on [`rule::SAFE_CATEGORIES`] — regeneratable data that the
//! owning program recreates — so automation can never delete something a
//! person made.

pub mod growth;
pub mod rule;

pub use growth::{
    change_over, growth_alert, low_space_alert, Alert, GrowthEntry, Snapshot, TimeRange,
};
pub use rule::{
    permitted_categories, should_run, Action, Conditions, Rule, Skip, Trigger, ERROR_LIMIT,
    SAFE_CATEGORIES,
};
