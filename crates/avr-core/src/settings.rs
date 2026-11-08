//! Application settings (recent projects, toolchain paths), stored as JSON in
//! the per-user configuration directory.

use crate::paths::atomic_write;
use crate::toolchains::ToolchainSettings;
use crate::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_RECENT: usize = 12;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
    pub mcu: String,
    pub opened_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub schema_version: u32,
    pub recent_projects: Vec<RecentProject>,
    pub toolchain: ToolchainSettings,
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            schema_version: 1,
            recent_projects: vec![],
            toolchain: ToolchainSettings::default(),
            theme: "dark".into(),
        }
    }
}

impl AppSettings {
    /// Loads settings; a corrupt file is preserved as `.corrupt` and defaults are used.
    pub fn load(path: &Path) -> (AppSettings, Option<String>) {
        match std::fs::read_to_string(path) {
            Err(_) => (AppSettings::default(), None),
            Ok(text) => match serde_json::from_str::<AppSettings>(&text) {
                Ok(s) => (s, None),
                Err(e) => {
                    let _ = std::fs::copy(path, path.with_extension("json.corrupt"));
                    (
                        AppSettings::default(),
                        Some(format!("Settings file was unreadable and has been reset: {e}")),
                    )
                }
            },
        }
    }

    pub fn save(&self, path: &Path) -> CoreResult<()> {
        let json =
            serde_json::to_string_pretty(self).map_err(|e| CoreError::InvalidRequest(e.to_string()))?;
        atomic_write(path, json.as_bytes(), false)
    }

    pub fn touch_recent(&mut self, entry: RecentProject) {
        self.recent_projects.retain(|r| r.path != entry.path);
        self.recent_projects.insert(0, entry);
        self.recent_projects.truncate(MAX_RECENT);
    }

    pub fn remove_recent(&mut self, path: &Path) {
        self.recent_projects.retain(|r| r.path != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_list_is_deduplicated_and_bounded() {
        let mut s = AppSettings::default();
        for i in 0..20 {
            s.touch_recent(RecentProject {
                path: format!("/p/{i}").into(),
                name: "x".into(),
                mcu: "atmega328p".into(),
                opened_at_ms: i,
            });
        }
        s.touch_recent(RecentProject {
            path: "/p/5".into(),
            name: "x".into(),
            mcu: "atmega328p".into(),
            opened_at_ms: 99,
        });
        assert_eq!(s.recent_projects.len(), MAX_RECENT);
        assert_eq!(s.recent_projects[0].path, PathBuf::from("/p/5"));
        assert_eq!(
            s.recent_projects
                .iter()
                .filter(|r| r.path == Path::new("/p/5"))
                .count(),
            1
        );
    }

    #[test]
    fn corrupt_settings_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, "{oops").unwrap();
        let (s, warn) = AppSettings::load(&p);
        assert_eq!(s, AppSettings::default());
        assert!(warn.is_some());
        assert!(dir.path().join("settings.json.corrupt").exists());
    }
}
