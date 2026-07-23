//! Local observation of application launches.
//!
//! Everything here stays on the device. Nothing is uploaded, and observation
//! only runs when the user turns it on in Settings.
//!
//! # Why snapshots rather than an event subscription
//!
//! The ideal mechanism is an event-driven one (a WMI `__InstanceCreationEvent`
//! subscription, or ETW). This implementation instead diffs periodic process
//! snapshots, which has a real and stated limitation: an application that
//! starts and exits entirely between two snapshots is never seen. That is an
//! honest under-count — Stora reports "no reliable activity data" rather than
//! inventing a launch. The interval is deliberately coarse so the cost stays
//! near zero; this is not a per-second poll of every process.

pub mod observer;
pub mod snapshot;
pub mod userassist;

pub use observer::{LaunchObserver, ObservedLaunch};
pub use snapshot::{running_processes, ProcessInfo};
pub use userassist::{newest_within, read_entries, UserAssistEntry};
