use serde::Serialize;
use tauri::State;

use stora_core::model::{Exclusion, ExclusionKind, ExclusionReason};
use stora_core::{Result, Settings, UiState};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAppearance {
    /// Windows accent color as `#rrggbb`, or `null` when unavailable.
    pub accent_color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorKeyStatus {
    /// The key itself is never returned to the frontend.
    pub saved: bool,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    state.settings()
}

/// Reports only whether an Advisor key exists in Windows Credential Manager.
#[tauri::command]
pub fn get_advisor_key_status() -> Result<AdvisorKeyStatus> {
    Ok(AdvisorKeyStatus {
        saved: stora_winapi::has_advisor_api_key()?,
    })
}

/// Writes a replacement key to Windows Credential Manager. It is intentionally
/// not persisted in Stora's SQLite settings database and cannot be read back
/// through IPC once saved.
#[tauri::command]
pub fn save_advisor_api_key(api_key: String) -> Result<AdvisorKeyStatus> {
    stora_winapi::save_advisor_api_key(&api_key)?;
    Ok(AdvisorKeyStatus { saved: true })
}

#[tauri::command]
pub fn delete_advisor_api_key() -> Result<AdvisorKeyStatus> {
    stora_winapi::delete_advisor_api_key()?;
    Ok(AdvisorKeyStatus { saved: false })
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    state.save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn get_ui_state(state: State<'_, AppState>) -> Result<UiState> {
    match state.index.get_setting("uiState")? {
        Some(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
        None => Ok(UiState::default()),
    }
}

#[tauri::command]
pub fn save_ui_state(state: State<'_, AppState>, ui_state: UiState) -> Result<()> {
    let raw = serde_json::to_string(&ui_state)
        .map_err(|err| stora_core::StoraError::Internal(err.to_string()))?;
    state.index.set_setting("uiState", &raw)
}

#[tauri::command]
pub fn get_system_appearance() -> SystemAppearance {
    SystemAppearance {
        accent_color: stora_winapi::accent_color(),
    }
}

#[tauri::command]
pub fn get_exclusions(state: State<'_, AppState>) -> Result<Vec<Exclusion>> {
    state.index.exclusions()
}

/// Adds a user exclusion. The pattern is normalized so it matches the same way
/// the scanner and cleaner compare paths.
#[tauri::command]
pub fn create_exclusion(
    state: State<'_, AppState>,
    pattern: String,
    kind: String,
) -> Result<Vec<Exclusion>> {
    let kind = ExclusionKind::parse(&kind);

    let pattern = match kind {
        ExclusionKind::Extension => pattern.trim_start_matches('.').to_ascii_lowercase(),
        ExclusionKind::Category => pattern,
        _ => stora_security::normalize(&pattern)?,
    };

    state.index.add_exclusion(
        &pattern,
        kind,
        ExclusionReason::UserExclusion,
        stora_core::now_seconds(),
    )?;
    state.index.exclusions()
}

#[tauri::command]
pub fn delete_exclusion(state: State<'_, AppState>, id: i64) -> Result<Vec<Exclusion>> {
    state.index.remove_exclusion(id)?;
    state.index.exclusions()
}

/// Total bytes recovered by cleanups in the last 30 days.
#[tauri::command]
pub fn get_recovered_this_month(state: State<'_, AppState>) -> Result<u64> {
    let cutoff = stora_core::now_seconds() - 30 * 24 * 60 * 60;
    state.index.recovered_since(cutoff)
}

/// Removes stored scan and cleanup history, leaving settings intact.
#[tauri::command]
pub fn clear_local_data(state: State<'_, AppState>) -> Result<()> {
    state.index.clear_local_data()
}

#[tauri::command]
pub fn get_data_folder(state: State<'_, AppState>) -> String {
    state.data_dir().to_string_lossy().to_string()
}
