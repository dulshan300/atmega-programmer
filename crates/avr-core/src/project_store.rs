//! Project model and on-disk repository.
//!
//! Layout:
//! ```text
//! <project>/
//!   project.json            manifest (schema version, device, toolchain, programmer)
//!   config/clock.json       clock assumptions used for F_CPU
//!   config/pins.json        GPIO pin assignments
//!   config/peripherals.json USART / ADC / JTAG settings
//!   config/fuses.json       desired fuse bytes (never written automatically)
//!   visual/main.flow.json   the visual program (authoritative source)
//!   src/generated/          generated C (reproducible, marked DO NOT EDIT)
//!   build/                  compiler artifacts
//!   logs/                   build and programmer logs
//! ```

use crate::device_registry::DeviceRegistry;
use crate::paths::{atomic_write, folder_name_for, require_absolute, validate_project_name};
use crate::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const PROJECT_SCHEMA_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "project.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Project {
    pub manifest: Manifest,
    pub clock: ClockConfig,
    pub pins: PinsConfig,
    pub peripherals: PeripheralsConfig,
    pub fuses: FusesConfig,
    pub graph: Graph,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub name: String,
    pub device: DeviceSelection,
    pub toolchain: ToolchainSelection,
    pub programmer: ProgrammerConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceSelection {
    pub mcu: String,
    pub package: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolchainSelection {
    /// `auto`, `winavr_legacy`, `modern`.
    pub profile: String,
    /// Optimization level passed to avr-gcc (`s`, `1`, `2`, `3`, `0`).
    pub optimization: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgrammerConfig {
    /// AVRDUDE programmer id from the built-in programmer catalog.
    pub programmer: String,
    /// Serial port or `usb`; empty when the programmer does not need one.
    pub port: String,
    pub baud: Option<u32>,
    /// AVRDUDE `-B` bit clock period in microseconds, as text (e.g. "10").
    pub bitclock: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClockConfig {
    /// Clock source id from the device definition.
    pub source: String,
    /// Oscillator frequency in Hz (before any prescaler).
    pub source_frequency_hz: u32,
    /// Whether the CKDIV8 fuse divides the clock by 8 at startup (devices with that fuse only).
    pub ckdiv8: bool,
}

impl ClockConfig {
    pub fn f_cpu(&self) -> u32 {
        if self.ckdiv8 {
            self.source_frequency_hz / 8
        } else {
            self.source_frequency_hz
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinsConfig {
    pub assignments: Vec<PinAssignment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinMode {
    Output,
    Input,
    InputPullup,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinAssignment {
    pub pin: String,
    pub label: String,
    pub mode: PinMode,
    /// Initial output level (ignored for inputs).
    pub initial_high: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PeripheralsConfig {
    pub usart: UsartConfig,
    pub adc: AdcConfig,
    /// ATmega32A-style parts: write JTD twice at startup so PC2–PC5 work as GPIO
    /// while the JTAGEN fuse stays programmed.
    pub disable_jtag_at_startup: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Parity {
    None,
    Even,
    Odd,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsartConfig {
    pub enabled: bool,
    pub baud: u32,
    pub data_bits: u8,
    pub parity: Parity,
    pub stop_bits: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdcConfig {
    pub enabled: bool,
    pub reference: String,
    pub prescaler: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FusesConfig {
    /// Desired fuse bytes keyed by AVRDUDE memory name. These are only ever
    /// written to hardware through the explicit review-and-confirm workflow.
    pub desired: BTreeMap<String, u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Graph {
    pub schema_version: u32,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub variables: Vec<Variable>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphNode {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: String,
    pub position: Position,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub source_handle: String,
    pub target: String,
    pub target_handle: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarType {
    Bool,
    Uint8,
    Int8,
    Uint16,
    Int16,
    Uint32,
    Int32,
}

impl VarType {
    pub fn c_type(self) -> &'static str {
        match self {
            VarType::Bool => "bool",
            VarType::Uint8 => "uint8_t",
            VarType::Int8 => "int8_t",
            VarType::Uint16 => "uint16_t",
            VarType::Int16 => "int16_t",
            VarType::Uint32 => "uint32_t",
            VarType::Int32 => "int32_t",
        }
    }
    pub fn range(self) -> (i64, i64) {
        match self {
            VarType::Bool => (0, 1),
            VarType::Uint8 => (0, u8::MAX as i64),
            VarType::Int8 => (i8::MIN as i64, i8::MAX as i64),
            VarType::Uint16 => (0, u16::MAX as i64),
            VarType::Int16 => (i16::MIN as i64, i16::MAX as i64),
            VarType::Uint32 => (0, u32::MAX as i64),
            VarType::Int32 => (i32::MIN as i64, i32::MAX as i64),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Variable {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: VarType,
    pub initial: i64,
}

impl Graph {
    pub fn empty() -> Self {
        Graph {
            schema_version: 1,
            nodes: vec![],
            edges: vec![],
            variables: vec![],
        }
    }
}

/// Starting templates offered in the New Project dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Template {
    Empty,
    Blink,
    UartEcho,
    AdcReading,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub parent_dir: PathBuf,
    pub name: String,
    pub mcu: String,
    pub package: String,
    pub template: Template,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedProject {
    pub root: PathBuf,
    pub project: Project,
    /// Non-fatal notes such as migrations performed or missing optional files.
    pub notes: Vec<String>,
}

fn default_led_pin(mcu: &str) -> &'static str {
    match mcu {
        "atmega328p" => "PB5", // Arduino Uno/Nano on-board LED
        _ => "PB0",
    }
}

/// Builds the in-memory project for a new project and template.
pub fn new_project(registry: &DeviceRegistry, req: &CreateProjectRequest) -> CoreResult<Project> {
    let name = validate_project_name(&req.name)?;
    let device = registry.get(&req.mcu)?;
    if device.package(&req.package).is_none() {
        return Err(CoreError::InvalidRequest(format!(
            "{} has no package {}",
            device.name, req.package
        )));
    }
    let (source, freq, ckdiv8) = match device.id.as_str() {
        // Factory default: internal 8 MHz RC with CKDIV8 programmed → 1 MHz.
        "atmega328p" => ("internal_rc", 8_000_000, true),
        // Factory default: internal 1 MHz RC.
        _ => ("internal_rc", 1_000_000, false),
    };
    let mut project = Project {
        manifest: Manifest {
            schema_version: PROJECT_SCHEMA_VERSION,
            name,
            device: DeviceSelection {
                mcu: device.id.clone(),
                package: req.package.clone(),
            },
            toolchain: ToolchainSelection {
                profile: "auto".into(),
                optimization: "s".into(),
            },
            programmer: ProgrammerConfig {
                programmer: "usbasp".into(),
                port: "usb".into(),
                baud: None,
                bitclock: None,
            },
        },
        clock: ClockConfig {
            source: source.into(),
            source_frequency_hz: freq,
            ckdiv8,
        },
        pins: PinsConfig { assignments: vec![] },
        peripherals: PeripheralsConfig {
            usart: UsartConfig {
                enabled: false,
                baud: 9600,
                data_bits: 8,
                parity: Parity::None,
                stop_bits: 1,
            },
            adc: AdcConfig {
                enabled: false,
                reference: "avcc".into(),
                prescaler: 8,
            },
            disable_jtag_at_startup: device.jtag_pins.is_some(),
        },
        fuses: FusesConfig {
            desired: device.default_fuses(),
        },
        graph: Graph::empty(),
    };
    crate::graph::templates::apply_template(&mut project, req.template, default_led_pin(&device.id));
    Ok(project)
}

fn read_json(path: &Path) -> CoreResult<Value> {
    let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
    serde_json::from_str(&text)
        .map_err(|e| CoreError::InvalidProject(format!("{} is not valid JSON: {e}", path.display())))
}

fn parse<T: serde::de::DeserializeOwned>(path: &Path, value: Value) -> CoreResult<T> {
    serde_json::from_value(value)
        .map_err(|e| CoreError::InvalidProject(format!("{} does not match the schema: {e}", path.display())))
}

/// Migrates raw manifest JSON to the current schema version in place.
/// Returns human-readable notes describing what changed.
pub fn migrate_manifest(raw: &mut Value) -> CoreResult<Vec<String>> {
    let version = raw
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or_else(|| CoreError::InvalidProject("project.json has no numeric schemaVersion".into()))?
        as u32;
    if version > PROJECT_SCHEMA_VERSION {
        return Err(CoreError::InvalidProject(format!(
            "project schema version {version} was created by a newer version of the application (this build supports {PROJECT_SCHEMA_VERSION})"
        )));
    }
    let notes = Vec::new();
    if version == 0 {
        return Err(CoreError::InvalidProject(
            "schemaVersion 0 is not a valid project".into(),
        ));
    }
    // Future migrations go here: `if version < 2 { ...; notes.push(...) }`.
    Ok(notes)
}

pub struct ProjectRepository;

impl ProjectRepository {
    pub const FILES: [&'static str; 6] = [
        MANIFEST_FILE,
        "config/clock.json",
        "config/pins.json",
        "config/peripherals.json",
        "config/fuses.json",
        "visual/main.flow.json",
    ];

    /// Creates a project folder `<parent>/<folder-name>`; refuses to touch an
    /// existing non-empty directory.
    pub fn create(registry: &DeviceRegistry, req: &CreateProjectRequest) -> CoreResult<LoadedProject> {
        let parent = require_absolute(&req.parent_dir)?;
        if !parent.is_dir() {
            return Err(CoreError::InvalidRequest(format!(
                "{} is not a directory",
                parent.display()
            )));
        }
        let project = new_project(registry, req)?;
        let root = parent.join(folder_name_for(&project.manifest.name));
        if root.exists()
            && std::fs::read_dir(&root)
                .map_err(|e| CoreError::io(&root, e))?
                .next()
                .is_some()
        {
            return Err(CoreError::Conflict(format!(
                "{} already exists and is not empty",
                root.display()
            )));
        }
        for dir in ["config", "visual", "src/generated", "include", "build", "logs"] {
            let d = root.join(dir);
            std::fs::create_dir_all(&d).map_err(|e| CoreError::io(&d, e))?;
        }
        Self::save(&root, &project)?;
        Ok(LoadedProject {
            root,
            project,
            notes: vec![],
        })
    }

    pub fn is_project_dir(root: &Path) -> bool {
        root.join(MANIFEST_FILE).is_file()
    }

    pub fn load(registry: &DeviceRegistry, root: &Path) -> CoreResult<LoadedProject> {
        let root = require_absolute(root)?;
        if !Self::is_project_dir(&root) {
            return Err(CoreError::InvalidProject(format!(
                "{} does not contain project.json",
                root.display()
            )));
        }
        let manifest_path = root.join(MANIFEST_FILE);
        let mut raw = read_json(&manifest_path)?;
        let notes = migrate_manifest(&mut raw)?;
        let manifest: Manifest = parse(&manifest_path, raw)?;
        let device = registry.get(&manifest.device.mcu)?;
        if device.package(&manifest.device.package).is_none() {
            return Err(CoreError::InvalidProject(format!(
                "{} has no package {}",
                device.name, manifest.device.package
            )));
        }
        let part = |rel: &str| -> CoreResult<Value> { read_json(&root.join(rel)) };
        let project = Project {
            clock: parse(&root.join("config/clock.json"), part("config/clock.json")?)?,
            pins: parse(&root.join("config/pins.json"), part("config/pins.json")?)?,
            peripherals: parse(
                &root.join("config/peripherals.json"),
                part("config/peripherals.json")?,
            )?,
            fuses: parse(&root.join("config/fuses.json"), part("config/fuses.json")?)?,
            graph: parse(
                &root.join("visual/main.flow.json"),
                part("visual/main.flow.json")?,
            )?,
            manifest,
        };
        for id in project.fuses.desired.keys() {
            if device.fuse_byte(id).is_none() {
                return Err(CoreError::InvalidProject(format!(
                    "config/fuses.json contains '{id}', which does not exist on {}",
                    device.name
                )));
            }
        }
        Ok(LoadedProject { root, project, notes })
    }

    /// Saves every project file atomically, keeping a `.bak` of each previous version.
    pub fn save(root: &Path, project: &Project) -> CoreResult<()> {
        let root = require_absolute(root)?;
        validate_project_name(&project.manifest.name)?;
        if project.manifest.schema_version != PROJECT_SCHEMA_VERSION {
            return Err(CoreError::InvalidProject(
                "refusing to save a project with a foreign schema version".into(),
            ));
        }
        let to_json = |v: &dyn erased::Json| -> CoreResult<Vec<u8>> {
            let mut s = v.to_json()?;
            s.push('\n');
            Ok(s.into_bytes())
        };
        let parts: [(&str, &dyn erased::Json); 6] = [
            (MANIFEST_FILE, &project.manifest),
            ("config/clock.json", &project.clock),
            ("config/pins.json", &project.pins),
            ("config/peripherals.json", &project.peripherals),
            ("config/fuses.json", &project.fuses),
            ("visual/main.flow.json", &project.graph),
        ];
        for (rel, value) in parts {
            atomic_write(&root.join(rel), &to_json(value)?, true)?;
        }
        Ok(())
    }

    /// Copies a project to `dest` excluding build artifacts and logs.
    /// `dest` must not exist.
    pub fn export(root: &Path, dest: &Path) -> CoreResult<()> {
        let root = require_absolute(root)?;
        let dest = require_absolute(dest)?;
        if dest.exists() {
            return Err(CoreError::Conflict(format!("{} already exists", dest.display())));
        }
        if dest.starts_with(&root) {
            return Err(CoreError::InvalidRequest(
                "cannot export a project into itself".into(),
            ));
        }
        copy_dir(&root, &dest, &|rel: &Path| {
            let first = rel
                .components()
                .next()
                .and_then(|c| c.as_os_str().to_str())
                .unwrap_or("");
            first != "build" && first != "logs" && !rel.to_string_lossy().ends_with(".bak")
        })?;
        for dir in ["build", "logs"] {
            let d = dest.join(dir);
            std::fs::create_dir_all(&d).map_err(|e| CoreError::io(&d, e))?;
        }
        Ok(())
    }

    /// Duplicates a project next to the original under a new name.
    pub fn duplicate(registry: &DeviceRegistry, root: &Path, new_name: &str) -> CoreResult<LoadedProject> {
        let name = validate_project_name(new_name)?;
        let root = require_absolute(root)?;
        let parent = root
            .parent()
            .ok_or_else(|| CoreError::InvalidRequest("project has no parent".into()))?;
        let dest = parent.join(folder_name_for(&name));
        Self::export(&root, &dest)?;
        let mut loaded = Self::load(registry, &dest)?;
        loaded.project.manifest.name = name;
        Self::save(&dest, &loaded.project)?;
        Ok(loaded)
    }
}

fn copy_dir(src: &Path, dst: &Path, include: &dyn Fn(&Path) -> bool) -> CoreResult<()> {
    fn walk(base: &Path, cur: &Path, dst: &Path, include: &dyn Fn(&Path) -> bool) -> CoreResult<()> {
        std::fs::create_dir_all(dst).map_err(|e| CoreError::io(dst, e))?;
        for entry in std::fs::read_dir(cur).map_err(|e| CoreError::io(cur, e))? {
            let entry = entry.map_err(|e| CoreError::io(cur, e))?;
            let path = entry.path();
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            if !include(&rel) {
                continue;
            }
            let ft = entry.file_type().map_err(|e| CoreError::io(&path, e))?;
            let target = dst.join(entry.file_name());
            if ft.is_dir() {
                walk(base, &path, &target, include)?;
            } else if ft.is_file() {
                std::fs::copy(&path, &target).map_err(|e| CoreError::io(&target, e))?;
            }
            // Symlinks are skipped deliberately.
        }
        Ok(())
    }
    walk(src, src, dst, include)
}

mod erased {
    use crate::{CoreError, CoreResult};
    pub trait Json {
        fn to_json(&self) -> CoreResult<String>;
    }
    impl<T: serde::Serialize> Json for T {
        fn to_json(&self) -> CoreResult<String> {
            serde_json::to_string_pretty(self).map_err(|e| CoreError::InvalidProject(e.to_string()))
        }
    }
}

pub fn project_paths(root: &Path) -> BTreeMap<&'static str, PathBuf> {
    BTreeMap::from([
        ("generated", root.join("src/generated")),
        ("build", root.join("build")),
        ("logs", root.join("logs")),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(dir: &Path, template: Template) -> CreateProjectRequest {
        CreateProjectRequest {
            parent_dir: dir.to_path_buf(),
            name: "Blink Test".into(),
            mcu: "atmega328p".into(),
            package: "PDIP-28".into(),
            template,
        }
    }

    #[test]
    fn create_save_load_roundtrip() {
        let reg = DeviceRegistry::builtin().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let created = ProjectRepository::create(&reg, &req(tmp.path(), Template::Blink)).unwrap();
        assert!(created.root.ends_with("Blink-Test"));
        let loaded = ProjectRepository::load(&reg, &created.root).unwrap();
        assert_eq!(loaded.project, created.project);
        assert!(!loaded.project.graph.nodes.is_empty());
    }

    #[test]
    fn create_refuses_non_empty_dir() {
        let reg = DeviceRegistry::builtin().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("Blink-Test")).unwrap();
        std::fs::write(tmp.path().join("Blink-Test/keep.txt"), "x").unwrap();
        let err = ProjectRepository::create(&reg, &req(tmp.path(), Template::Empty)).unwrap_err();
        assert_eq!(err.kind(), "conflict");
    }

    #[test]
    fn rejects_newer_schema_and_unknown_fields() {
        let reg = DeviceRegistry::builtin().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let created = ProjectRepository::create(&reg, &req(tmp.path(), Template::Empty)).unwrap();
        let mpath = created.root.join(MANIFEST_FILE);
        let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&mpath).unwrap()).unwrap();
        raw["schemaVersion"] = 99.into();
        std::fs::write(&mpath, raw.to_string()).unwrap();
        let err = ProjectRepository::load(&reg, &created.root).unwrap_err();
        assert!(err.to_string().contains("newer version"));

        raw["schemaVersion"] = 1.into();
        raw["unexpected"] = true.into();
        std::fs::write(&mpath, raw.to_string()).unwrap();
        assert_eq!(
            ProjectRepository::load(&reg, &created.root).unwrap_err().kind(),
            "invalidProject"
        );
    }

    #[test]
    fn rejects_corrupt_json_and_foreign_fuses() {
        let reg = DeviceRegistry::builtin().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let created = ProjectRepository::create(&reg, &req(tmp.path(), Template::Empty)).unwrap();
        std::fs::write(created.root.join("config/pins.json"), "{ not json").unwrap();
        assert!(
            ProjectRepository::load(&reg, &created.root)
                .unwrap_err()
                .to_string()
                .contains("not valid JSON")
        );

        ProjectRepository::save(&created.root, &created.project).unwrap();
        let mut p = created.project.clone();
        p.manifest.device.mcu = "atmega32a".into();
        p.manifest.device.package = "PDIP-40".into();
        ProjectRepository::save(&created.root, &p).unwrap(); // fuses still contain efuse from the 328P
        let err = ProjectRepository::load(&reg, &created.root).unwrap_err();
        assert!(err.to_string().contains("efuse"));
    }

    #[test]
    fn export_excludes_build_outputs() {
        let reg = DeviceRegistry::builtin().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let created = ProjectRepository::create(&reg, &req(tmp.path(), Template::Blink)).unwrap();
        std::fs::write(created.root.join("build/firmware.elf"), "elf").unwrap();
        let dest = tmp.path().join("exported");
        ProjectRepository::export(&created.root, &dest).unwrap();
        assert!(dest.join(MANIFEST_FILE).is_file());
        assert!(!dest.join("build/firmware.elf").exists());
        let dup = ProjectRepository::duplicate(&reg, &created.root, "Blink Copy").unwrap();
        assert_eq!(dup.project.manifest.name, "Blink Copy");
    }

    #[test]
    fn default_clock_matches_factory_fuses() {
        let reg = DeviceRegistry::builtin().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let p = new_project(&reg, &req(tmp.path(), Template::Empty)).unwrap();
        assert_eq!(p.clock.f_cpu(), 1_000_000);
        let mut r = req(tmp.path(), Template::Empty);
        r.mcu = "atmega32a".into();
        r.package = "PDIP-40".into();
        let p = new_project(&reg, &r).unwrap();
        assert_eq!(p.clock.f_cpu(), 1_000_000);
        assert!(!p.fuses.desired.contains_key("efuse"));
    }
}
