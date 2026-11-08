//! Programmer and fuse commands. Every operation runs against the project as
//! saved on disk, holds the single hardware-operation slot, and (for serial
//! programmers) leases the port so the serial monitor cannot open it at the
//! same time.

use super::{Shared, blocking};
use crate::state::{AppState, lock};
use avr_core::build::HEX_NAME;
use avr_core::fuses::{
    DecodedField, FusePlanInput, FuseReadResult, FuseWritePlan, FuseWriteResult, decode, plan_write,
    read_fuses, write_fuses,
};
use avr_core::paths::require_absolute;
use avr_core::process::{CancelToken, Stream};
use avr_core::programmer::{
    Avrdude, Connection, IdentifyResult, PROGRAMMERS, ProgrammerDef, UploadResult, programmer_def,
    validate_port,
};
use avr_core::project_store::{LoadedProject, ProjectRepository};
use avr_core::serial::PortLease;
use avr_core::toolchains::{AvrdudeTool, detect};
use avr_core::{CoreError, CoreResult};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

const OWNER: &str = "Device Programmer";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LineEvent {
    stream: Stream,
    text: String,
}

fn emitter(app: AppHandle) -> impl FnMut(Stream, &str) {
    move |stream, text| {
        let _ = app.emit(
            "programmer://line",
            LineEvent {
                stream,
                text: text.to_string(),
            },
        );
    }
}

/// Everything a hardware operation needs, resolved before the slot is taken.
struct Context {
    root: PathBuf,
    loaded: LoadedProject,
    tool: AvrdudeTool,
}

fn context(state: &AppState, root: &Path) -> CoreResult<Context> {
    let root = require_absolute(root)?;
    let loaded = ProjectRepository::load(&state.registry, &root)?;
    let settings = lock(&state.settings)?.toolchain.clone();
    let status = detect(&settings);
    let tool = status.avrdude.ok_or_else(|| {
        CoreError::Toolchain("AVRDUDE was not found. Set its path in Settings → Toolchains.".into())
    })?;
    let cfg = &loaded.project.manifest.programmer;
    let def = programmer_def(&cfg.programmer)?;
    if def.connection == Connection::Serial {
        if cfg.port.trim().is_empty() {
            return Err(CoreError::InvalidRequest(format!(
                "{} needs a serial port. Choose one in the programmer settings.",
                def.label
            )));
        }
        validate_port(&cfg.port)?;
    }
    Ok(Context { root, loaded, tool })
}

/// Holds the hardware slot (and a port lease) for the duration of one operation.
struct Slot {
    state: Arc<AppState>,
    _lease: Option<PortLease>,
}

impl Drop for Slot {
    fn drop(&mut self) {
        if let Ok(mut op) = self.state.hardware_op.lock() {
            *op = None;
        }
    }
}

fn take_slot(state: &Arc<AppState>, ctx: &Context) -> CoreResult<(Slot, CancelToken)> {
    let token = CancelToken::new();
    {
        let mut op = lock(&state.hardware_op)?;
        if op.is_some() {
            return Err(CoreError::Busy(
                "another programmer operation is already running".into(),
            ));
        }
        *op = Some(token.clone());
    }
    let mut slot = Slot {
        state: state.clone(),
        _lease: None,
    };
    let cfg = &ctx.loaded.project.manifest.programmer;
    if programmer_def(&cfg.programmer)?.connection == Connection::Serial {
        // On failure `slot` drops here and releases the hardware slot.
        slot._lease = Some(state.ports.acquire(&cfg.port, OWNER)?);
    }
    Ok((slot, token))
}

fn avrdude<'a>(state: &'a AppState, ctx: &'a Context) -> CoreResult<Avrdude<'a>> {
    Ok(Avrdude {
        tool: &ctx.tool,
        device: state.registry.get(&ctx.loaded.project.manifest.device.mcu)?,
        config: &ctx.loaded.project.manifest.programmer,
    })
}

fn fuse_dir(root: &Path) -> PathBuf {
    root.join("build").join("fuses")
}

fn hex_path(root: &Path) -> PathBuf {
    root.join("build").join(HEX_NAME)
}

#[tauri::command]
pub fn list_programmers() -> &'static [ProgrammerDef] {
    PROGRAMMERS
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgrammerPreview {
    identify: String,
    upload: Option<String>,
    hex_exists: bool,
    hex_modified_ms: Option<u64>,
    avrdude_version: Option<String>,
}

/// Shows the exact AVRDUDE commands that would run. Never touches hardware.
#[tauri::command]
pub async fn programmer_preview(state: Shared<'_>, root: PathBuf) -> CoreResult<ProgrammerPreview> {
    let state = state.inner().clone();
    blocking(move || {
        let ctx = context(&state, &root)?;
        let avr = avrdude(&state, &ctx)?;
        let hex = hex_path(&ctx.root);
        let hex_modified_ms = std::fs::metadata(&hex)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        Ok(ProgrammerPreview {
            identify: avr.identify_spec()?.display(),
            upload: avr.upload_spec(&hex).ok().map(|s| s.display()),
            hex_exists: hex.is_file(),
            hex_modified_ms,
            avrdude_version: ctx.tool.version.clone(),
        })
    })
    .await
}

#[tauri::command]
pub async fn programmer_identify(
    app: AppHandle,
    state: Shared<'_>,
    root: PathBuf,
) -> CoreResult<IdentifyResult> {
    let state = state.inner().clone();
    blocking(move || {
        let ctx = context(&state, &root)?;
        let (_slot, token) = take_slot(&state, &ctx)?;
        avrdude(&state, &ctx)?.identify(&state.registry, &token, &mut emitter(app))
    })
    .await
}

#[tauri::command]
pub async fn programmer_upload(app: AppHandle, state: Shared<'_>, root: PathBuf) -> CoreResult<UploadResult> {
    let state = state.inner().clone();
    blocking(move || {
        let ctx = context(&state, &root)?;
        let hex = hex_path(&ctx.root);
        if !hex.is_file() {
            return Err(CoreError::InvalidRequest(
                "no firmware has been built yet. Build the project first.".into(),
            ));
        }
        let (_slot, token) = take_slot(&state, &ctx)?;
        avrdude(&state, &ctx)?.upload(&hex, &token, &mut emitter(app))
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuseReadResponse {
    #[serde(flatten)]
    result: FuseReadResult,
    decoded: Vec<DecodedField>,
}

#[tauri::command]
pub async fn fuses_read(app: AppHandle, state: Shared<'_>, root: PathBuf) -> CoreResult<FuseReadResponse> {
    let state = state.inner().clone();
    blocking(move || {
        let ctx = context(&state, &root)?;
        let (_slot, token) = take_slot(&state, &ctx)?;
        let avr = avrdude(&state, &ctx)?;
        let result = read_fuses(&avr, &fuse_dir(&ctx.root), &token, &mut emitter(app))?;
        let decoded = decode(avr.device, &result.values);
        Ok(FuseReadResponse { result, decoded })
    })
    .await
}

/// Builds a reviewable plan. Never touches hardware.
#[tauri::command]
pub async fn fuses_plan(state: Shared<'_>, root: PathBuf, input: FusePlanInput) -> CoreResult<FuseWritePlan> {
    let state = state.inner().clone();
    blocking(move || {
        let ctx = context(&state, &root)?;
        plan_write(&avrdude(&state, &ctx)?, &input)
    })
    .await
}

#[tauri::command]
pub async fn fuses_write(
    app: AppHandle,
    state: Shared<'_>,
    root: PathBuf,
    input: FusePlanInput,
    token: String,
) -> CoreResult<FuseWriteResult> {
    let state = state.inner().clone();
    blocking(move || {
        let ctx = context(&state, &root)?;
        let (_slot, cancel) = take_slot(&state, &ctx)?;
        write_fuses(
            &avrdude(&state, &ctx)?,
            &input,
            &token,
            &fuse_dir(&ctx.root),
            &cancel,
            &mut emitter(app),
        )
    })
    .await
}

#[tauri::command]
pub fn cancel_hardware_operation(state: Shared<'_>) -> CoreResult<bool> {
    Ok(lock(&state.hardware_op)?.as_ref().map(|t| t.cancel()).is_some())
}
