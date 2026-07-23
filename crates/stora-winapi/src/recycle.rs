use stora_core::{Result, StoraError};

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName, FOFX_EARLYFAILURE,
        FOFX_RECYCLEONDELETE, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// RAII guard so COM is always balanced, including on the error paths.
    struct ComGuard;

    impl ComGuard {
        fn new() -> Self {
            // SAFETY: initializing COM on the calling (worker) thread. A
            // failure here is benign — it usually means COM is already
            // initialized with a compatible model.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            Self
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            // SAFETY: paired with the CoInitializeEx above on this thread.
            unsafe { CoUninitialize() };
        }
    }

    /// Sends a batch of paths to the Recycle Bin in a single shell operation.
    ///
    /// Batching matters: one `IFileOperation` for thousands of files is far
    /// faster than one call per file, and the shell handles the per-item
    /// bookkeeping the Recycle Bin needs for restoration.
    pub fn recycle(paths: &[String]) -> Result<Vec<(String, StoraError)>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let _com = ComGuard::new();

        // SAFETY: standard COM activation of the shell's file-operation object.
        let operation: IFileOperation =
            unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) }
                .map_err(|err| StoraError::Windows(format!("CoCreateInstance: {err}")))?;

        // Recycle rather than delete, stay silent, and fail early so a bad
        // item aborts before anything is queued behind it.
        // SAFETY: `operation` is a live COM interface.
        unsafe {
            operation
                .SetOperationFlags(
                    FOF_NOCONFIRMATION
                        | FOF_NOERRORUI
                        | FOF_SILENT
                        | FOFX_RECYCLEONDELETE
                        | FOFX_EARLYFAILURE,
                )
                .map_err(|err| StoraError::Windows(format!("SetOperationFlags: {err}")))?;
        }

        let mut failures = Vec::new();
        let mut queued = 0usize;

        for path in paths {
            let extended = stora_security::to_extended_length(path);
            let wide_path = wide(&extended);

            // SAFETY: `wide_path` is NUL-terminated and outlives the call.
            let item: std::result::Result<IShellItem, _> =
                unsafe { SHCreateItemFromParsingName(PCWSTR(wide_path.as_ptr()), None) };

            match item {
                Ok(item) => {
                    // SAFETY: both COM objects are live.
                    match unsafe { operation.DeleteItem(&item, None) } {
                        Ok(()) => queued += 1,
                        Err(err) => failures.push((
                            path.clone(),
                            StoraError::Windows(format!("DeleteItem: {err}")),
                        )),
                    }
                }
                Err(err) => failures.push((
                    path.clone(),
                    StoraError::Windows(format!("SHCreateItemFromParsingName: {err}")),
                )),
            }
        }

        if queued == 0 {
            return Ok(failures);
        }

        // SAFETY: `operation` is live and has at least one queued item.
        unsafe { operation.PerformOperations() }
            .map_err(|err| StoraError::Windows(format!("PerformOperations: {err}")))?;

        // SAFETY: valid after PerformOperations returns.
        let aborted = unsafe { operation.GetAnyOperationsAborted() }
            .map(|value| value.as_bool())
            .unwrap_or(false);

        if aborted {
            return Err(StoraError::Internal(
                "the shell reported that the recycle operation was aborted".into(),
            ));
        }

        Ok(failures)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn recycle(_paths: &[String]) -> Result<Vec<(String, StoraError)>> {
        Err(StoraError::UnsupportedFilesystem {
            filesystem: "the Recycle Bin requires Windows".into(),
        })
    }
}

/// Moves the given normalized paths to the Recycle Bin.
///
/// Returns per-item failures rather than aborting: one locked file should not
/// prevent the rest of a cleanup from completing.
pub fn move_to_recycle_bin(paths: &[String]) -> Result<Vec<(String, StoraError)>> {
    imp::recycle(paths)
}
