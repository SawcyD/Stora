//! Lightweight local scheduler for rules that the user explicitly enabled.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::{commands::advanced, state::AppState};

pub const AUTOMATION_RUN_EVENT: &str = "stora://automation-run";

/// Starts one bounded background check every five minutes. Rule-level guards
/// still decide whether anything is due, so waking up does not imply a run.
pub fn start(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("stora-automation".into())
        .spawn(move || loop {
            let messages = match advanced::run_automation_cycle(&app.state::<AppState>()) {
                Ok(messages) => messages,
                Err(error) => {
                    tracing::warn!(?error, "could not evaluate automation rules");
                    Vec::new()
                }
            };
            if !messages.is_empty() {
                let _ = app.emit(AUTOMATION_RUN_EVENT, messages);
            }
            std::thread::sleep(Duration::from_secs(5 * 60));
        });
}
