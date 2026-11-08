use super::Shared;
use crate::state::lock;
use avr_core::device_registry::{DeviceDefinition, DeviceSummary};
use avr_core::paths::require_absolute;
use avr_core::settings::AppSettings;
use avr_core::toolchains::{ToolchainSettings, ToolchainStatus, detect};
use avr_core::{CoreError, CoreResult};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    os: &'static str,
    devices: Vec<DeviceSummary>,
}

#[tauri::command]
pub fn app_info(state: Shared<'_>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        devices: state.registry.summaries(),
    }
}

#[tauri::command]
pub fn get_device(state: Shared<'_>, id: String) -> CoreResult<DeviceDefinition> {
    state.registry.get(&id).cloned()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResponse {
    settings: AppSettings,
    warning: Option<String>,
}

#[tauri::command]
pub fn get_settings(state: Shared<'_>) -> CoreResult<SettingsResponse> {
    let settings = lock(&state.settings)?.clone();
    // Drop recent entries whose folders disappeared.
    Ok(SettingsResponse {
        settings,
        warning: state.settings_warning.clone(),
    })
}

fn validate_optional(p: &Option<PathBuf>) -> CoreResult<()> {
    if let Some(p) = p {
        require_absolute(p)?;
    }
    Ok(())
}

#[tauri::command]
pub fn save_toolchain_settings(state: Shared<'_>, toolchain: ToolchainSettings) -> CoreResult<AppSettings> {
    validate_optional(&toolchain.avr_gcc_path)?;
    validate_optional(&toolchain.avrdude_path)?;
    validate_optional(&toolchain.avrdude_conf_path)?;
    for d in &toolchain.extra_search_dirs {
        require_absolute(d)?;
    }
    let mut s = lock(&state.settings)?;
    s.toolchain = toolchain;
    s.save(&state.settings_path)?;
    Ok(s.clone())
}

#[tauri::command]
pub fn set_theme(state: Shared<'_>, theme: String) -> CoreResult<()> {
    if theme != "dark" && theme != "light" {
        return Err(CoreError::InvalidRequest(
            "theme must be 'dark' or 'light'".into(),
        ));
    }
    let mut s = lock(&state.settings)?;
    s.theme = theme;
    s.save(&state.settings_path)
}

#[tauri::command]
pub async fn detect_toolchains(state: Shared<'_>) -> CoreResult<ToolchainStatus> {
    let settings = lock(&state.settings)?.toolchain.clone();
    super::blocking(move || Ok(detect(&settings))).await
}

#[tauri::command]
pub fn remove_recent(state: Shared<'_>, path: PathBuf) -> CoreResult<AppSettings> {
    let mut s = lock(&state.settings)?;
    s.remove_recent(&path);
    s.save(&state.settings_path)?;
    Ok(s.clone())
}

/// Writes a user-chosen text file (serial logs, exported code). Restricted to
/// text extensions; the path comes from a native save dialog.
#[tauri::command]
pub fn save_text_file(path: PathBuf, contents: String) -> CoreResult<()> {
    let path = require_absolute(&path)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !matches!(ext.as_str(), "txt" | "log" | "csv" | "c" | "h") {
        return Err(CoreError::InvalidRequest(
            "only .txt, .log, .csv, .c and .h files can be saved".into(),
        ));
    }
    std::fs::write(&path, contents).map_err(|e| CoreError::io(&path, e))
}
