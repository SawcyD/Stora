//! Allocated-size and file-lock queries.

use stora_core::Result;

/// Returns the space a file actually occupies on disk, which differs from its
/// logical size for sparse, compressed, and very small (resident) files.
///
/// Falls back to the logical size when the query is unavailable, so callers
/// always get a usable number.
pub fn allocated_size(path: &str, logical_size: u64) -> u64 {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetCompressedFileSizeW;

        let extended = stora_security::to_extended_length(path);
        let wide: Vec<u16> = extended.encode_utf16().chain(std::iter::once(0)).collect();
        let mut high: u32 = 0;

        // SAFETY: `wide` is NUL-terminated and `high` is a valid out-param.
        let low = unsafe { GetCompressedFileSizeW(PCWSTR(wide.as_ptr()), Some(&mut high)) };

        // INVALID_FILE_SIZE with a real error means the query failed.
        if low == u32::MAX {
            return logical_size;
        }
        ((high as u64) << 32) | low as u64
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        logical_size
    }
}

/// Rounds a logical size up to the volume's cluster size.
///
/// Used as an estimate when a per-file query is too expensive during a bulk
/// scan. Marked as an estimate wherever it reaches the UI.
pub fn estimate_allocated(logical_size: u64, cluster_size: u64) -> u64 {
    if cluster_size == 0 {
        return logical_size;
    }
    logical_size.div_ceil(cluster_size) * cluster_size
}

/// Names of processes currently holding a handle to `path`.
///
/// Uses the Restart Manager, the mechanism Windows Installer itself relies on.
/// An empty vector means either nothing holds the file or the query was
/// unavailable — this is advisory information, never proof.
pub fn processes_locking(path: &str) -> Result<Vec<String>> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
        use windows::Win32::System::RestartManager::{
            RmEndSession, RmGetList, RmRegisterResources, RmStartSession, RM_PROCESS_INFO,
        };

        let mut session_handle: u32 = 0;
        let mut session_key = [0u16; 33]; // CCH_RM_SESSION_KEY + 1

        // SAFETY: both out-params are valid locals; the key buffer is the
        // size the Restart Manager documents.
        let started = unsafe {
            RmStartSession(
                &mut session_handle,
                0,
                windows::core::PWSTR(session_key.as_mut_ptr()),
            )
        };
        if started != ERROR_SUCCESS {
            return Ok(Vec::new());
        }

        // Ensure the session is closed on every path out of this block.
        struct SessionGuard(u32);
        impl Drop for SessionGuard {
            fn drop(&mut self) {
                // SAFETY: handle came from a successful RmStartSession.
                let _ = unsafe { RmEndSession(self.0) };
            }
        }
        let _guard = SessionGuard(session_handle);

        let extended = stora_security::to_extended_length(path);
        let wide: Vec<u16> = extended.encode_utf16().chain(std::iter::once(0)).collect();
        let resources = [PCWSTR(wide.as_ptr())];

        // SAFETY: `resources` outlives the call; the other resource kinds are
        // unused and correctly passed as empty.
        let registered =
            unsafe { RmRegisterResources(session_handle, Some(&resources), None, None) };
        if registered != ERROR_SUCCESS {
            return Ok(Vec::new());
        }

        let mut needed: u32 = 0;
        let mut count: u32 = 0;
        let mut reason: u32 = 0;

        // First call reports how many entries exist.
        // SAFETY: passing a null array with a zero count is the documented
        // way to size the buffer.
        let probe =
            unsafe { RmGetList(session_handle, &mut needed, &mut count, None, &mut reason) };

        if probe == ERROR_SUCCESS || needed == 0 {
            return Ok(Vec::new());
        }
        if probe != ERROR_MORE_DATA {
            return Ok(Vec::new());
        }

        let mut infos = vec![RM_PROCESS_INFO::default(); needed as usize];
        count = needed;

        // SAFETY: `infos` has exactly `count` elements.
        let listed = unsafe {
            RmGetList(
                session_handle,
                &mut needed,
                &mut count,
                Some(infos.as_mut_ptr()),
                &mut reason,
            )
        };
        if listed != ERROR_SUCCESS {
            return Ok(Vec::new());
        }

        let mut names: Vec<String> = infos
            .iter()
            .take(count as usize)
            .map(|info| {
                let raw = &info.strAppName;
                let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                String::from_utf16_lossy(&raw[..end])
            })
            .filter(|name| !name.is_empty())
            .collect();

        names.sort();
        names.dedup();
        Ok(names)
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_rounds_up_to_the_cluster_size() {
        assert_eq!(estimate_allocated(1, 4096), 4096);
        assert_eq!(estimate_allocated(4096, 4096), 4096);
        assert_eq!(estimate_allocated(4097, 4096), 8192);
    }

    #[test]
    fn estimate_handles_empty_files_and_unknown_clusters() {
        assert_eq!(estimate_allocated(0, 4096), 0);
        assert_eq!(estimate_allocated(1234, 0), 1234);
    }

    #[test]
    fn allocated_size_falls_back_for_a_missing_file() {
        let reported = allocated_size("C:\\definitely\\missing\\file.bin", 512);
        assert_eq!(reported, 512);
    }

    #[test]
    fn allocated_size_is_reported_for_a_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sample.bin");
        std::fs::write(&file, vec![7u8; 8192]).unwrap();

        let size = allocated_size(&file.to_string_lossy(), 8192);
        // Allocation is cluster-rounded, so it is never below the logical size
        // for an ordinary uncompressed file.
        assert!(
            size >= 8192,
            "expected at least the logical size, got {size}"
        );
    }
}
