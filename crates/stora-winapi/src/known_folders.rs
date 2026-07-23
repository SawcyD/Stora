//! Safe wrappers for changing the current user's Windows Known Folder paths.
//!
//! The caller is responsible for copying and verifying data first. This module
//! only changes the shell's registered location; it never moves files itself.

use stora_core::{Result, StoraError};

#[cfg(windows)]
pub fn redirect(folder_name: &str, destination: &str) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::UI::Shell::{
        SHSetKnownFolderPath, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Pictures,
        FOLDERID_Videos,
    };

    let folder_id = match folder_name {
        "Downloads" => FOLDERID_Downloads,
        "Documents" => FOLDERID_Documents,
        "Pictures" => FOLDERID_Pictures,
        "Videos" => FOLDERID_Videos,
        _ => {
            return Err(StoraError::InvalidPath {
                reason: "this folder cannot be redirected".into(),
            })
        }
    };
    let wide: Vec<u16> = destination.encode_utf16().chain(std::iter::once(0)).collect();

    // SAFETY: the GUID and NUL-terminated UTF-16 destination remain valid for
    // the duration of the call. A null token means the current user.
    unsafe {
        SHSetKnownFolderPath(&folder_id, 0, HANDLE::default(), PCWSTR(wide.as_ptr()))
            .map_err(|error| StoraError::Windows(format!("could not update Windows folder location: {error}")))
    }
}

#[cfg(not(windows))]
pub fn redirect(_folder_name: &str, _destination: &str) -> Result<()> {
    Err(StoraError::UnsupportedFilesystem {
        filesystem: "Windows Known Folder redirection requires Windows".into(),
    })
}
