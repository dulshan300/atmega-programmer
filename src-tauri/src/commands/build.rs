use super::{Shared, blocking};
use crate::state::lock;
use avr_core::build::{BuildReport, BuildRequest, BuildSink, BuildStage, run_build};
use avr_core::paths::require_absolute;
use avr_core::process::{CancelToken, Stream};
use avr_core::{CoreError, CoreResult};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
enum BuildEvent {
    Stage { stage: BuildStage },
    Line { stream: Stream, text: String },
}

struct EventSink(AppHandle);

impl BuildSink for EventSink {
    fn stage(&mut self, stage: BuildStage) {
        let _ = self.0.emit("build://event", BuildEvent::Stage { stage });
    }
    fn line(&mut self, stream: Stream, text: &str) {
        let _ = self.0.emit(
            "build://event",
            BuildEvent::Line {
                stream,
                text: text.to_string(),
            },
        );
    }
}

#[tauri::command]
pub async fn start_build(
    app: AppHandle,
    state: Shared<'_>,
    root: PathBuf,
    force_regenerate: bool,
) -> CoreResult<BuildReport> {
    let root = require_absolute(&root)?;
    let token = CancelToken::new();
    {
        let mut builds = lock(&state.builds)?;
        if builds.contains_key(&root) {
            return Err(CoreError::Busy(
                "a build of this project is already running".into(),
            ));
        }
        builds.insert(root.clone(), token.clone());
    }
    let registry = state.registry.clone();
    let catalog = state.catalog.clone();
    let settings = lock(&state.settings)?.toolchain.clone();
    let root2 = root.clone();
    let result = blocking(move || {
        let req = BuildRequest {
            root: &root2,
            registry: &registry,
            catalog: &catalog,
            settings: &settings,
            force_regenerate,
        };
        Ok(run_build(&req, &mut EventSink(app), &token))
    })
    .await;
    lock(&state.builds)?.remove(&root);
    result
}

#[tauri::command]
pub fn cancel_build(state: Shared<'_>, root: PathBuf) -> CoreResult<bool> {
    Ok(lock(&state.builds)?.get(&root).map(|t| t.cancel()).is_some())
}
