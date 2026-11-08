//! Tauri shell: typed commands that validate input and delegate to `avr-core`.
//! All privileged work (filesystem, processes, serial ports) happens here or
//! in the core crate, never in the web frontend.

mod commands;
mod state;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let state = AppState::new(config_dir.join("settings.json"))?;
            app.manage(Arc::new(state));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::app_info,
            commands::system::get_device,
            commands::system::get_settings,
            commands::system::save_toolchain_settings,
            commands::system::set_theme,
            commands::system::detect_toolchains,
            commands::system::remove_recent,
            commands::system::save_text_file,
            commands::project::create_project,
            commands::project::open_project,
            commands::project::save_project,
            commands::project::duplicate_project,
            commands::project::export_project,
            commands::project::validate_project,
            commands::project::generate_preview,
            commands::build::start_build,
            commands::build::cancel_build,
            commands::programmer::list_programmers,
            commands::programmer::programmer_preview,
            commands::programmer::programmer_identify,
            commands::programmer::programmer_upload,
            commands::programmer::fuses_read,
            commands::programmer::fuses_plan,
            commands::programmer::fuses_write,
            commands::programmer::cancel_hardware_operation,
            commands::serial::serial_list_ports,
            commands::serial::serial_open,
            commands::serial::serial_write,
            commands::serial::serial_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AVR Studio Visual");
}
