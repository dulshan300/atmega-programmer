use avr_core::device_registry::DeviceRegistry;
use avr_core::graph::catalog::BlockCatalog;
use avr_core::process::CancelToken;
use avr_core::serial::{PortRegistry, SerialSession};
use avr_core::settings::AppSettings;
use avr_core::{CoreError, CoreResult};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct AppState {
    pub registry: Arc<DeviceRegistry>,
    pub catalog: Arc<BlockCatalog>,
    pub settings: Mutex<AppSettings>,
    pub settings_warning: Option<String>,
    pub settings_path: PathBuf,
    pub ports: PortRegistry,
    /// Running builds keyed by project root.
    pub builds: Mutex<BTreeMap<PathBuf, CancelToken>>,
    /// At most one programmer operation at a time.
    pub hardware_op: Mutex<Option<CancelToken>>,
    pub serial: Mutex<Option<SerialSession>>,
}

impl AppState {
    pub fn new(settings_path: PathBuf) -> CoreResult<Self> {
        let (settings, settings_warning) = AppSettings::load(&settings_path);
        Ok(AppState {
            registry: Arc::new(DeviceRegistry::builtin()?),
            catalog: Arc::new(BlockCatalog::builtin()?),
            settings: Mutex::new(settings),
            settings_warning,
            settings_path,
            ports: PortRegistry::new(),
            builds: Mutex::new(BTreeMap::new()),
            hardware_op: Mutex::new(None),
            serial: Mutex::new(None),
        })
    }
}

pub fn lock<T>(m: &Mutex<T>) -> CoreResult<MutexGuard<'_, T>> {
    m.lock()
        .map_err(|_| CoreError::Busy("internal state lock poisoned".into()))
}
