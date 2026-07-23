use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::{Result, StoraError};

const RUN: u8 = 0;
const PAUSE: u8 = 1;
const CANCEL: u8 = 2;

/// Cooperative control handle shared between the UI thread and a worker.
///
/// Workers poll this at directory and file granularity, which keeps
/// cancellation latency well under a second without per-byte checks.
#[derive(Debug)]
pub struct TaskControl {
    state: AtomicU8,
    finished: AtomicBool,
}

impl Default for TaskControl {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskControl {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(RUN),
            finished: AtomicBool::new(false),
        }
    }

    pub fn pause(&self) {
        // Never move a cancelling task back into a running-ish state.
        let _ = self
            .state
            .compare_exchange(RUN, PAUSE, Ordering::SeqCst, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        let _ = self
            .state
            .compare_exchange(PAUSE, RUN, Ordering::SeqCst, Ordering::SeqCst);
    }

    pub fn cancel(&self) {
        self.state.store(CANCEL, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::SeqCst) == CANCEL
    }

    pub fn is_paused(&self) -> bool {
        self.state.load(Ordering::SeqCst) == PAUSE
    }

    pub fn mark_finished(&self) {
        self.finished.store(true, Ordering::SeqCst);
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    /// Blocks while paused. Returns `Err(ScanCancelled)` if cancelled, so
    /// worker loops can simply use `?`.
    pub fn checkpoint(&self) -> Result<()> {
        loop {
            match self.state.load(Ordering::SeqCst) {
                CANCEL => return Err(StoraError::ScanCancelled),
                PAUSE => std::thread::sleep(std::time::Duration::from_millis(80)),
                _ => return Ok(()),
            }
        }
    }
}

/// Registry of live background tasks, keyed by task id.
///
/// Enforces one task per kind so overlapping scans or cleanups cannot corrupt
/// each other's state.
#[derive(Debug, Default)]
pub struct TaskRegistry {
    inner: Mutex<HashMap<String, TaskHandle>>,
}

#[derive(Debug, Clone)]
struct TaskHandle {
    kind: String,
    control: Arc<TaskControl>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new task of `kind`, failing if one is already active.
    pub fn register(&self, kind: &str, task_id: &str) -> Result<Arc<TaskControl>> {
        let mut guard = self.inner.lock().expect("task registry poisoned");
        guard.retain(|_, handle| !handle.control.is_finished());

        if guard.values().any(|handle| handle.kind == kind) {
            return Err(StoraError::TaskAlreadyRunning { kind: kind.into() });
        }

        let control = Arc::new(TaskControl::new());
        guard.insert(
            task_id.to_string(),
            TaskHandle {
                kind: kind.to_string(),
                control: Arc::clone(&control),
            },
        );
        Ok(control)
    }

    pub fn get(&self, task_id: &str) -> Result<Arc<TaskControl>> {
        let guard = self.inner.lock().expect("task registry poisoned");
        guard
            .get(task_id)
            .map(|handle| Arc::clone(&handle.control))
            .ok_or_else(|| StoraError::TaskNotFound {
                task_id: task_id.into(),
            })
    }

    /// Returns the live task of the given kind, if any.
    pub fn active_of_kind(&self, kind: &str) -> Option<(String, Arc<TaskControl>)> {
        let guard = self.inner.lock().expect("task registry poisoned");
        guard.iter().find_map(|(id, handle)| {
            (handle.kind == kind && !handle.control.is_finished())
                .then(|| (id.clone(), Arc::clone(&handle.control)))
        })
    }

    pub fn finish(&self, task_id: &str) {
        let mut guard = self.inner.lock().expect("task registry poisoned");
        if let Some(handle) = guard.remove(task_id) {
            handle.control.mark_finished();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_overlapping_tasks_of_same_kind() {
        let registry = TaskRegistry::new();
        registry
            .register("scan", "a")
            .expect("first scan registers");
        let err = registry
            .register("scan", "b")
            .expect_err("second scan rejected");
        assert_eq!(err.code(), "TaskAlreadyRunning");
    }

    #[test]
    fn allows_new_task_after_previous_finished() {
        let registry = TaskRegistry::new();
        registry.register("scan", "a").unwrap();
        registry.finish("a");
        registry.register("scan", "b").expect("scan slot freed");
    }

    #[test]
    fn different_kinds_run_concurrently() {
        let registry = TaskRegistry::new();
        registry.register("scan", "a").unwrap();
        registry
            .register("cleanup", "b")
            .expect("cleanup is a separate slot");
    }

    #[test]
    fn checkpoint_reports_cancellation() {
        let control = TaskControl::new();
        control.cancel();
        assert!(control.checkpoint().is_err());
        assert!(control.is_cancelled());
    }

    #[test]
    fn cancel_wins_over_pause() {
        let control = TaskControl::new();
        control.cancel();
        control.resume();
        assert!(control.is_cancelled(), "resume must not undo a cancel");
    }
}
