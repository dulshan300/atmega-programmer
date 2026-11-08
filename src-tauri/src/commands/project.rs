use super::{Shared, blocking, now_ms};
use crate::state::lock;
use avr_core::CoreResult;
use avr_core::build::{analyze_project, preview_code};
use avr_core::codegen::GeneratedFile;
use avr_core::diagnostics::Diagnostic;
use avr_core::paths::require_absolute;
use avr_core::project_store::{CreateProjectRequest, LoadedProject, Project, ProjectRepository};
use avr_core::settings::RecentProject;
use serde::Serialize;
use std::path::PathBuf;

fn remember(state: &Shared<'_>, loaded: &LoadedProject) -> CoreResult<()> {
    let mut s = lock(&state.settings)?;
    s.touch_recent(RecentProject {
        path: loaded.root.clone(),
        name: loaded.project.manifest.name.clone(),
        mcu: loaded.project.manifest.device.mcu.clone(),
        opened_at_ms: now_ms(),
    });
    s.save(&state.settings_path)
}

#[tauri::command]
pub fn create_project(state: Shared<'_>, request: CreateProjectRequest) -> CoreResult<LoadedProject> {
    let loaded = ProjectRepository::create(&state.registry, &request)?;
    remember(&state, &loaded)?;
    Ok(loaded)
}

#[tauri::command]
pub fn open_project(state: Shared<'_>, path: PathBuf) -> CoreResult<LoadedProject> {
    let loaded = ProjectRepository::load(&state.registry, &path)?;
    remember(&state, &loaded)?;
    Ok(loaded)
}

#[tauri::command]
pub fn save_project(state: Shared<'_>, root: PathBuf, project: Project) -> CoreResult<()> {
    let root = require_absolute(&root)?;
    if !ProjectRepository::is_project_dir(&root) {
        return Err(avr_core::CoreError::InvalidRequest(format!(
            "{} is not a project folder",
            root.display()
        )));
    }
    // Reject devices/packages unknown to this build before anything is written.
    let device = state.registry.get(&project.manifest.device.mcu)?;
    if device.package(&project.manifest.device.package).is_none() {
        return Err(avr_core::CoreError::InvalidRequest(
            "unknown package for this device".into(),
        ));
    }
    ProjectRepository::save(&root, &project)
}

#[tauri::command]
pub fn duplicate_project(state: Shared<'_>, root: PathBuf, new_name: String) -> CoreResult<LoadedProject> {
    let loaded = ProjectRepository::duplicate(&state.registry, &root, &new_name)?;
    remember(&state, &loaded)?;
    Ok(loaded)
}

#[tauri::command]
pub async fn export_project(root: PathBuf, destination: PathBuf) -> CoreResult<()> {
    blocking(move || ProjectRepository::export(&root, &destination)).await
}

#[tauri::command]
pub fn validate_project(state: Shared<'_>, project: Project) -> CoreResult<Vec<Diagnostic>> {
    let device = state.registry.get(&project.manifest.device.mcu)?;
    Ok(analyze_project(device, &state.catalog, &project).diagnostics)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResponse {
    files: Vec<GeneratedFile>,
    diagnostics: Vec<Diagnostic>,
    legacy: bool,
}

#[tauri::command]
pub fn generate_preview(state: Shared<'_>, project: Project) -> CoreResult<PreviewResponse> {
    let device = state.registry.get(&project.manifest.device.mcu)?;
    let legacy = project.manifest.toolchain.profile == "winavr_legacy";
    let (code, diagnostics) = preview_code(device, &state.catalog, &project, legacy);
    Ok(PreviewResponse {
        files: code.map(|c| c.files).unwrap_or_default(),
        diagnostics,
        legacy,
    })
}
