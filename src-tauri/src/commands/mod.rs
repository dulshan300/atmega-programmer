pub mod build;
pub mod programmer;
pub mod project;
pub mod serial;
pub mod system;

use avr_core::{CoreError, CoreResult};

pub type Shared<'a> = tauri::State<'a, std::sync::Arc<crate::state::AppState>>;

/// Runs blocking work off the async runtime.
pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CoreResult<T> + Send + 'static,
) -> CoreResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CoreError::Process(format!("background task failed: {e}")))?
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
