//! End-to-end builds against a real AVR-GCC.
//!
//! Uses `AVR_STUDIO_TEST_GCC` when set, otherwise the toolchain unpacked into
//! the repository's `.tools/` folder. Tests are skipped (with a message) when
//! neither is available, so CI without a toolchain still passes.

use avr_core::build::{BuildRequest, BuildSink, BuildStage, run_build};
use avr_core::device_registry::DeviceRegistry;
use avr_core::graph::catalog::BlockCatalog;
use avr_core::process::{CancelToken, Stream};
use avr_core::project_store::{CreateProjectRequest, ProjectRepository, Template};
use avr_core::toolchains::ToolchainSettings;
use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn gcc() -> Option<PathBuf> {
    let p = std::env::var_os("AVR_STUDIO_TEST_GCC")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo().join(".tools/root/usr/bin/avr-gcc"));
    if p.is_file() {
        Some(p)
    } else {
        eprintln!("SKIPPED: no avr-gcc at {} (set AVR_STUDIO_TEST_GCC)", p.display());
        None
    }
}

#[derive(Default)]
struct Collect {
    stages: Vec<BuildStage>,
    lines: Vec<String>,
}

impl BuildSink for Collect {
    fn stage(&mut self, stage: BuildStage) {
        self.stages.push(stage);
    }
    fn line(&mut self, _stream: Stream, text: &str) {
        self.lines.push(text.to_string());
    }
}

#[test]
fn every_template_builds_for_every_device() {
    let Some(gcc) = gcc() else { return };
    let registry = DeviceRegistry::builtin().unwrap();
    let catalog = BlockCatalog::builtin().unwrap();
    let settings = ToolchainSettings {
        avr_gcc_path: Some(gcc),
        ..Default::default()
    };
    let tmp = tempfile::tempdir().unwrap();

    for (mcu, package) in [("atmega328p", "PDIP-28"), ("atmega32a", "PDIP-40")] {
        for (i, template) in [
            Template::Empty,
            Template::Blink,
            Template::UartEcho,
            Template::AdcReading,
        ]
        .into_iter()
        .enumerate()
        {
            let label = format!("{mcu} template #{i}");
            let loaded = ProjectRepository::create(
                &registry,
                &CreateProjectRequest {
                    parent_dir: tmp.path().to_path_buf(),
                    name: format!("{mcu} t{i}"),
                    mcu: mcu.into(),
                    package: package.into(),
                    template,
                },
            )
            .unwrap();
            let req = BuildRequest {
                root: &loaded.root,
                registry: &registry,
                catalog: &catalog,
                settings: &settings,
                force_regenerate: false,
            };
            let mut sink = Collect::default();
            let report = run_build(&req, &mut sink, &CancelToken::new());
            assert!(
                report.success,
                "{label}: {}\n{}",
                report.message,
                sink.lines.join("\n")
            );
            assert_eq!(report.stage, BuildStage::Done, "{label}");
            let art = report.artifacts.as_ref().expect("artifacts");
            let hex = std::fs::read_to_string(&art.hex).unwrap();
            assert!(
                hex.starts_with(':') && hex.trim_end().ends_with(":00000001FF"),
                "{label}: not Intel HEX"
            );
            let mem = report.memory.as_ref().expect("memory report");
            assert!(
                mem.flash.used > 0 && mem.flash.used <= mem.flash.total,
                "{label}: flash {:?}",
                mem.flash
            );
            assert!(
                report.log_path.as_ref().is_some_and(|p| p.is_file()),
                "{label}: build log written"
            );

            // A second build with nothing changed must succeed and leave the
            // generated sources untouched.
            let mut sink = Collect::default();
            let again = run_build(&req, &mut sink, &CancelToken::new());
            assert!(again.success, "{label} rebuild: {}", again.message);
        }
    }
}

#[test]
fn hand_edited_generated_files_are_protected_until_regeneration_is_confirmed() {
    let Some(gcc) = gcc() else { return };
    let registry = DeviceRegistry::builtin().unwrap();
    let catalog = BlockCatalog::builtin().unwrap();
    let settings = ToolchainSettings {
        avr_gcc_path: Some(gcc),
        ..Default::default()
    };
    let tmp = tempfile::tempdir().unwrap();
    let loaded = ProjectRepository::create(
        &registry,
        &CreateProjectRequest {
            parent_dir: tmp.path().to_path_buf(),
            name: "edited".into(),
            mcu: "atmega328p".into(),
            package: "PDIP-28".into(),
            template: Template::Blink,
        },
    )
    .unwrap();
    let mut req = BuildRequest {
        root: &loaded.root,
        registry: &registry,
        catalog: &catalog,
        settings: &settings,
        force_regenerate: false,
    };
    assert!(run_build(&req, &mut Collect::default(), &CancelToken::new()).success);

    let main_c = loaded.root.join("src/generated/main.c");
    let mut text = std::fs::read_to_string(&main_c).unwrap();
    text.push_str("\n/* my hand edit */\n");
    std::fs::write(&main_c, &text).unwrap();

    let report = run_build(&req, &mut Collect::default(), &CancelToken::new());
    assert!(!report.success);
    assert_eq!(report.stage, BuildStage::Generate);
    assert!(report.message.contains("edited by hand"), "{}", report.message);
    assert_eq!(
        std::fs::read_to_string(&main_c).unwrap(),
        text,
        "the edit must survive"
    );
    assert!(report.artifacts.is_none());

    req.force_regenerate = true;
    let report = run_build(&req, &mut Collect::default(), &CancelToken::new());
    assert!(report.success, "{}", report.message);
    assert!(!std::fs::read_to_string(&main_c).unwrap().contains("my hand edit"));
}
