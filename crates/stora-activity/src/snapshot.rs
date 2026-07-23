use stora_core::Result;

/// One running process, as observed in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub executable_name: String,
    /// Full image path, when it could be read. Some processes are protected.
    pub executable_path: Option<String>,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    fn image_path(pid: u32) -> Option<String> {
        // SAFETY: opening with the most limited rights that still allow an
        // image-path query. Failure is expected for protected processes.
        let handle: HANDLE =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

        let mut buffer = [0u16; MAX_PATH as usize];
        let mut length = buffer.len() as u32;

        // SAFETY: `buffer` is `length` units long, as declared.
        let queried = unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut length,
            )
        };

        // SAFETY: `handle` came from a successful OpenProcess.
        unsafe {
            let _ = CloseHandle(handle);
        }

        queried.ok()?;
        Some(String::from_utf16_lossy(&buffer[..length as usize]))
    }

    pub fn enumerate() -> Result<Vec<ProcessInfo>> {
        // SAFETY: a process-list snapshot takes no input buffers.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(|err| stora_core::StoraError::Windows(format!("snapshot: {err}")))?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut processes = Vec::new();

        // SAFETY: `entry.dwSize` is set as the API requires.
        if unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok() {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);

                if !name.is_empty() {
                    processes.push(ProcessInfo {
                        pid: entry.th32ProcessID,
                        executable_path: image_path(entry.th32ProcessID)
                            .map(|path| path.replace('/', "\\")),
                        executable_name: name,
                    });
                }

                // SAFETY: same contract as Process32FirstW.
                if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                    break;
                }
            }
        }

        // SAFETY: `snapshot` came from a successful CreateToolhelp32Snapshot.
        unsafe {
            let _ = CloseHandle(snapshot);
        }

        Ok(processes)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn enumerate() -> Result<Vec<ProcessInfo>> {
        Ok(Vec::new())
    }
}

/// Takes a snapshot of currently running processes.
pub fn running_processes() -> Result<Vec<ProcessInfo>> {
    imp::enumerate()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn a_snapshot_includes_this_test_process() {
        let processes = running_processes().expect("snapshot succeeds");
        assert!(!processes.is_empty());

        let current = std::process::id();
        assert!(
            processes.iter().any(|p| p.pid == current),
            "the running test process must appear in its own snapshot"
        );
    }

    #[test]
    #[cfg(windows)]
    fn snapshots_report_executable_names() {
        let processes = running_processes().unwrap();
        assert!(
            processes.iter().all(|p| !p.executable_name.is_empty()),
            "every entry needs a name"
        );
    }

    #[test]
    #[cfg(windows)]
    fn image_paths_are_backslash_separated_when_available() {
        let processes = running_processes().unwrap();
        for process in processes.iter().filter_map(|p| p.executable_path.as_ref()) {
            assert!(
                !process.contains('/'),
                "paths must be normalized to backslashes: {process}"
            );
        }
    }
}
