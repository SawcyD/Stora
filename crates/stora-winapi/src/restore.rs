//! System Restore point creation.
//!
//! This is best-effort by nature. System Restore is switched off by default on
//! most Windows 11 installations, creating a point requires elevation, and
//! Windows refuses more than one per 24 hours unless that limit is changed.
//! Every one of those is a normal outcome, so the result distinguishes them
//! rather than collapsing everything into "failed".

/// Why a restore point could not be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreFailure {
    /// System Restore is turned off for the volume.
    Disabled,
    /// Creating a point needs administrator rights.
    NeedsElevation,
    /// One was already created within the last 24 hours.
    RateLimited,
    /// Something else went wrong.
    Other,
}

/// Attempts to create a System Restore point named `description`.
///
/// Uses the documented WMI provider through PowerShell rather than the
/// deprecated `SRSetRestorePoint` C API, which is what Windows itself
/// recommends for this operation.
pub fn create_restore_point(description: &str) -> std::result::Result<(), RestoreFailure> {
    #[cfg(windows)]
    {
        use std::process::Command;

        // The description is passed as a separate argument, never interpolated
        // into a command string, so it cannot alter the command.
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "param($d) Checkpoint-Computer -Description $d \
                 -RestorePointType APPLICATION_UNINSTALL",
                "-d",
                description,
            ])
            .output()
            .map_err(|_| RestoreFailure::Other)?;

        if output.status.success() {
            return Ok(());
        }

        let message = String::from_utf8_lossy(&output.stderr).to_lowercase();

        // Windows reports each of these differently, and the user deserves to
        // know which one they hit.
        if message.contains("disabled") || message.contains("not enabled") {
            return Err(RestoreFailure::Disabled);
        }
        if message.contains("access is denied")
            || message.contains("administrator")
            || message.contains("elevat")
        {
            return Err(RestoreFailure::NeedsElevation);
        }
        if message.contains("frequency")
            || message.contains("24 hours")
            || message.contains("already been created")
        {
            return Err(RestoreFailure::RateLimited);
        }

        Err(RestoreFailure::Other)
    }

    #[cfg(not(windows))]
    {
        let _ = description;
        Err(RestoreFailure::Other)
    }
}

/// True when `winget` is present and runnable.
pub fn winget_available() -> bool {
    #[cfg(windows)]
    {
        use std::process::Command;

        Command::new("winget")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_winget_probe_is_stable_within_a_run() {
        // Either answer is valid on a given machine, but the uninstall flow
        // resolves the method twice — at preflight and at execution — so an
        // unstable probe would offer winget and then fail to use it.
        let first = winget_available();
        let second = winget_available();
        assert_eq!(first, second);
    }

    #[test]
    #[cfg(not(windows))]
    fn restore_points_are_unavailable_off_windows() {
        assert_eq!(create_restore_point("test"), Err(RestoreFailure::Other));
    }

    #[test]
    fn failure_reasons_are_distinct() {
        // Each maps to different wording for the user, so they must not be
        // collapsed together.
        let all = [
            RestoreFailure::Disabled,
            RestoreFailure::NeedsElevation,
            RestoreFailure::RateLimited,
            RestoreFailure::Other,
        ];
        for (index, first) in all.iter().enumerate() {
            for second in all.iter().skip(index + 1) {
                assert_ne!(first, second);
            }
        }
    }
}
