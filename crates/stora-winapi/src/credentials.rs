//! Small, narrow wrapper around Windows Credential Manager.
//!
//! The application can ask whether its Advisor key exists, save a replacement,
//! or remove it. There is deliberately no public read function: no Tauri
//! command or UI surface should ever receive the secret after it is saved.

use stora_core::{Result, StoraError};

const ADVISOR_TARGET: &str = "Stora:AdvisorApiKey:v1";

#[cfg(windows)]
pub fn has_advisor_api_key() -> Result<bool> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredFree, CredReadW, CRED_TYPE_GENERIC};

    let target = wide(ADVISOR_TARGET);
    let mut credential = std::ptr::null_mut();
    match unsafe {
        CredReadW(
            PCWSTR(target.as_ptr()),
            CRED_TYPE_GENERIC,
            0,
            &mut credential,
        )
    } {
        Ok(()) => {
            unsafe { CredFree(credential.cast()) };
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

/// Reads the secret only inside the native process immediately before an
/// authorized Advisor request. Never expose this through IPC or serialize it.
#[cfg(windows)]
pub fn read_advisor_api_key() -> Result<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredFree, CredReadW, CRED_TYPE_GENERIC};

    let target = wide(ADVISOR_TARGET);
    let mut credential = std::ptr::null_mut();
    unsafe {
        CredReadW(
            PCWSTR(target.as_ptr()),
            CRED_TYPE_GENERIC,
            0,
            &mut credential,
        )
    }
    .map_err(|_| {
        StoraError::Internal("No Advisor API key is saved in Windows Credential Manager.".into())
    })?;

    let bytes = unsafe {
        std::slice::from_raw_parts(
            (*credential).CredentialBlob,
            (*credential).CredentialBlobSize as usize,
        )
    };
    let result = String::from_utf8(bytes.to_vec())
        .map_err(|_| StoraError::Internal("The saved Advisor API key is not valid text.".into()));
    unsafe { CredFree(credential.cast()) };
    result
}

#[cfg(not(windows))]
pub fn read_advisor_api_key() -> Result<String> {
    Err(StoraError::Internal(
        "Windows Credential Manager is only available on Windows".into(),
    ))
}

#[cfg(not(windows))]
pub fn has_advisor_api_key() -> Result<bool> {
    Ok(false)
}

#[cfg(windows)]
pub fn save_advisor_api_key(api_key: &str) -> Result<()> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err(StoraError::Internal("the API key cannot be empty".into()));
    }

    let mut target = wide(ADVISOR_TARGET);
    let mut secret = trimmed.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: secret.len() as u32,
        CredentialBlob: secret.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR::null(),
        ..Default::default()
    };

    unsafe { CredWriteW(&credential, 0) }.map_err(|error| {
        StoraError::Internal(format!(
            "Windows Credential Manager could not save the Advisor key: {error}"
        ))
    })
}

#[cfg(not(windows))]
pub fn save_advisor_api_key(_api_key: &str) -> Result<()> {
    Err(StoraError::Internal(
        "Windows Credential Manager is only available on Windows".into(),
    ))
}

#[cfg(windows)]
pub fn delete_advisor_api_key() -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = wide(ADVISOR_TARGET);
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0) } {
        Ok(()) => Ok(()),
        // A missing credential is already the intended end state.
        Err(_) => Ok(()),
    }
}

#[cfg(not(windows))]
pub fn delete_advisor_api_key() -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
