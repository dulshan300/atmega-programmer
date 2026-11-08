//! Integration tests that run the real process pipeline against mock
//! `avr-gcc` / `avrdude` executables (POSIX shell scripts). They check what
//! unit tests cannot: the exact argument vectors that reach the tools,
//! stateful fuse write/read-back, cancellation of a running tool, and that
//! refused operations never start a process.
#![cfg(unix)]

use avr_core::CoreError;
use avr_core::build::{BuildRequest, BuildSink, BuildStage, run_build};
use avr_core::device_registry::DeviceRegistry;
use avr_core::fuses::{FusePlanInput, plan_write, read_fuses, write_fuses};
use avr_core::graph::catalog::BlockCatalog;
use avr_core::process::{CancelToken, Stream};
use avr_core::programmer::Avrdude;
use avr_core::project_store::{CreateProjectRequest, ProgrammerConfig, ProjectRepository, Template};
use avr_core::toolchains::{AvrdudeTool, ToolchainSettings};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// A fake avrdude that logs its argv (one argument per line) to `argv.log`
/// and then runs `body`.
fn avrdude(dir: &Path, body: &str) -> AvrdudeTool {
    let log = dir.join("argv.log");
    let path = script(
        dir,
        "avrdude",
        &format!(
            "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\necho '--' >> '{}'\n{body}",
            log.display(),
            log.display()
        ),
    );
    AvrdudeTool {
        path,
        version: Some("7.1".into()),
        conf: None,
    }
}

fn argv_log(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join("argv.log")).ok()
}

fn usbasp() -> ProgrammerConfig {
    ProgrammerConfig {
        programmer: "usbasp".into(),
        port: "usb".into(),
        baud: None,
        bitclock: None,
    }
}

fn no_lines() -> impl FnMut(Stream, &str) {
    |_, _| {}
}

// ---------------------------------------------------------------- avrdude

#[test]
fn identify_streams_output_and_passes_exact_arguments() {
    let tmp = tempfile::tempdir().unwrap();
    let tool = avrdude(
        tmp.path(),
        "echo 'avrdude: Device signature = 0x1e950f (probably m328p)' >&2",
    );
    let reg = DeviceRegistry::builtin().unwrap();
    let cfg = usbasp();
    let avr = Avrdude {
        tool: &tool,
        device: reg.get("atmega328p").unwrap(),
        config: &cfg,
    };
    let mut lines = vec![];
    let r = avr
        .identify(&reg, &CancelToken::new(), &mut |s, l| {
            lines.push((s, l.to_string()))
        })
        .unwrap();
    assert!(r.success, "{}", r.message);
    assert_eq!(r.detected_device.as_deref(), Some("ATmega328P"));
    assert!(
        lines
            .iter()
            .any(|(s, l)| *s == Stream::Stderr && l.contains("signature"))
    );
    assert_eq!(argv_log(tmp.path()).unwrap(), "-c\nusbasp\n-p\nm328p\n-n\n--\n");
}

#[test]
fn identify_of_a_different_chip_is_not_a_success_even_with_exit_code_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let tool = avrdude(
        tmp.path(),
        "echo 'avrdude: Device signature = 0x1e9502' >&2; exit 0",
    );
    let reg = DeviceRegistry::builtin().unwrap();
    let cfg = usbasp();
    let avr = Avrdude {
        tool: &tool,
        device: reg.get("atmega328p").unwrap(),
        config: &cfg,
    };
    let r = avr.identify(&reg, &CancelToken::new(), &mut no_lines()).unwrap();
    assert!(!r.success && !r.matches_project);
    assert_eq!(r.detected_device.as_deref(), Some("ATmega32A"));
}

#[test]
fn upload_passes_the_hex_path_verbatim_and_requires_verification() {
    let tmp = tempfile::tempdir().unwrap();
    // A path with spaces and shell metacharacters must arrive as one argument.
    let fw_dir = tmp.path().join("my firmware; $(touch pwned)");
    std::fs::create_dir_all(&fw_dir).unwrap();
    let hex = fw_dir.join("firmware.hex");
    std::fs::write(&hex, ":00000001FF\n").unwrap();
    let reg = DeviceRegistry::builtin().unwrap();
    let cfg = usbasp();

    let tool = avrdude(
        tmp.path(),
        "echo 'avrdude: 162 bytes of flash written' >&2; echo 'avrdude: 162 bytes of flash verified' >&2",
    );
    let avr = Avrdude {
        tool: &tool,
        device: reg.get("atmega32a").unwrap(),
        config: &cfg,
    };
    let r = avr.upload(&hex, &CancelToken::new(), &mut no_lines()).unwrap();
    assert!(r.success && r.verified, "{}", r.message);
    let log = argv_log(tmp.path()).unwrap();
    assert!(log.contains(&format!("\nflash:w:{}:i\n", hex.display())), "{log}");
    assert!(!tmp.path().join("pwned").exists());

    // Exit code 0 without verification evidence is not a success.
    let tmp2 = tempfile::tempdir().unwrap();
    let tool = avrdude(tmp2.path(), "echo 'avrdude: 162 bytes of flash written' >&2");
    let avr = Avrdude {
        tool: &tool,
        device: reg.get("atmega32a").unwrap(),
        config: &cfg,
    };
    let r = avr.upload(&hex, &CancelToken::new(), &mut no_lines()).unwrap();
    assert!(!r.success && !r.verified);
}

#[test]
fn upload_can_be_cancelled_and_is_reported_as_incomplete() {
    let tmp = tempfile::tempdir().unwrap();
    let hex = tmp.path().join("fw.hex");
    std::fs::write(&hex, ":00000001FF\n").unwrap();
    let tool = avrdude(tmp.path(), "echo 'Writing |' >&2; sleep 30");
    let reg = DeviceRegistry::builtin().unwrap();
    let cfg = usbasp();
    let avr = Avrdude {
        tool: &tool,
        device: reg.get("atmega328p").unwrap(),
        config: &cfg,
    };
    let cancel = CancelToken::new();
    let c2 = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        c2.cancel();
    });
    let started = std::time::Instant::now();
    let r = avr.upload(&hex, &cancel, &mut no_lines()).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancellation must kill the process"
    );
    assert!(!r.success && r.output.cancelled);
    assert!(r.message.contains("incomplete"), "{}", r.message);
}

/// Stateful fake: `-U <mem>:w:<val>:m` stores a value, `-U <mem>:r:<file>:h`
/// writes it to the file. Bytes listed in `sticky` ignore writes.
fn fuse_avrdude(dir: &Path, initial: &[(&str, u8)], sticky: &[&str]) -> AvrdudeTool {
    let state = dir.join("fuse-state");
    std::fs::create_dir_all(&state).unwrap();
    for (k, v) in initial {
        std::fs::write(state.join(k), format!("0x{v:02x}\n")).unwrap();
    }
    for k in sticky {
        std::fs::write(state.join(format!("{k}.sticky")), "").unwrap();
    }
    let body = format!(
        r#"S='{}'
echo 'avrdude: Device signature = 0x1e950f' >&2
while [ $# -gt 0 ]; do
  if [ "$1" = "-U" ]; then
    op="$2"; shift
    mem="${{op%%:*}}"; rest="${{op#*:}}"; mode="${{rest%%:*}}"; rest="${{rest#*:}}"; arg="${{rest%:*}}"
    case "$mode" in
      r) cat "$S/$mem" > "$arg" ;;
      w) [ -e "$S/$mem.sticky" ] || echo "$arg" > "$S/$mem"; echo "avrdude: 1 bytes of $mem written" >&2 ;;
    esac
  fi
  shift
done"#,
        state.display()
    );
    avrdude(dir, &body)
}

fn fuses_328p_defaults() -> [(&'static str, u8); 3] {
    [("lfuse", 0x62), ("hfuse", 0xD9), ("efuse", 0xFF)]
}

#[test]
fn fuse_read_plan_write_and_readback() {
    let tmp = tempfile::tempdir().unwrap();
    let tool = fuse_avrdude(tmp.path(), &fuses_328p_defaults(), &[]);
    let reg = DeviceRegistry::builtin().unwrap();
    let cfg = usbasp();
    let avr = Avrdude {
        tool: &tool,
        device: reg.get("atmega328p").unwrap(),
        config: &cfg,
    };
    let work = tmp.path().join("work");

    let read = read_fuses(&avr, &work, &CancelToken::new(), &mut no_lines()).unwrap();
    assert!(read.success, "{}", read.message);
    assert_eq!(read.values.get("lfuse"), Some(&0x62));

    let mut desired: BTreeMap<String, u8> = read.values.clone();
    desired.insert("lfuse".into(), 0xE2); // unprogram CKDIV8: 1 MHz → 8 MHz
    let input = FusePlanInput {
        desired,
        current: Some(read.values.clone()),
    };
    let plan = plan_write(&avr, &input).unwrap();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    assert!(
        plan.command.contains("lfuse:w:0xE2:m") && !plan.command.contains("hfuse:w"),
        "{}",
        plan.command
    );

    std::fs::remove_file(tmp.path().join("argv.log")).unwrap();
    let res = write_fuses(
        &avr,
        &input,
        &plan.token,
        &work,
        &CancelToken::new(),
        &mut no_lines(),
    )
    .unwrap();
    assert!(res.success && res.verified, "{}", res.message);
    assert_eq!(res.readback.get("lfuse"), Some(&0xE2));
    // Only the changed byte was written; the read-back was a separate run.
    let log = argv_log(tmp.path()).unwrap();
    assert_eq!(log.matches(":w:").count(), 1, "{log}");
    assert_eq!(log.matches("--\n").count(), 2, "write run + verify run: {log}");
}

#[test]
fn fuse_readback_mismatch_is_reported_as_unverified() {
    let tmp = tempfile::tempdir().unwrap();
    let tool = fuse_avrdude(tmp.path(), &fuses_328p_defaults(), &["lfuse"]);
    let reg = DeviceRegistry::builtin().unwrap();
    let cfg = usbasp();
    let avr = Avrdude {
        tool: &tool,
        device: reg.get("atmega328p").unwrap(),
        config: &cfg,
    };
    let current: BTreeMap<String, u8> = fuses_328p_defaults()
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
    let mut desired = current.clone();
    desired.insert("lfuse".into(), 0xE2);
    let input = FusePlanInput {
        desired,
        current: Some(current),
    };
    let plan = plan_write(&avr, &input).unwrap();
    let res = write_fuses(
        &avr,
        &input,
        &plan.token,
        &tmp.path().join("w"),
        &CancelToken::new(),
        &mut no_lines(),
    )
    .unwrap();
    assert!(!res.success && !res.verified);
    assert!(res.message.to_lowercase().contains("lfuse"), "{}", res.message);
}

#[test]
fn fuse_write_with_a_stale_token_never_starts_avrdude() {
    let tmp = tempfile::tempdir().unwrap();
    let tool = fuse_avrdude(tmp.path(), &fuses_328p_defaults(), &[]);
    let reg = DeviceRegistry::builtin().unwrap();
    let cfg = usbasp();
    let avr = Avrdude {
        tool: &tool,
        device: reg.get("atmega328p").unwrap(),
        config: &cfg,
    };
    let current: BTreeMap<String, u8> = fuses_328p_defaults()
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
    let mut desired = current.clone();
    desired.insert("lfuse".into(), 0xE2);
    let reviewed = plan_write(
        &avr,
        &FusePlanInput {
            desired: desired.clone(),
            current: Some(current.clone()),
        },
    )
    .unwrap();

    // The request changes after review (e.g. another tab edited the value).
    desired.insert("lfuse".into(), 0xFF);
    let err = write_fuses(
        &avr,
        &FusePlanInput {
            desired,
            current: Some(current),
        },
        &reviewed.token,
        &tmp.path().join("w"),
        &CancelToken::new(),
        &mut no_lines(),
    )
    .unwrap_err();
    assert!(matches!(err, CoreError::Safety(_)), "{err}");
    assert!(
        argv_log(tmp.path()).is_none(),
        "avrdude must not have been started"
    );
}

#[test]
fn fuse_write_that_would_disable_isp_never_starts_avrdude() {
    let tmp = tempfile::tempdir().unwrap();
    let tool = fuse_avrdude(tmp.path(), &fuses_328p_defaults(), &[]);
    let reg = DeviceRegistry::builtin().unwrap();
    let cfg = usbasp();
    let avr = Avrdude {
        tool: &tool,
        device: reg.get("atmega328p").unwrap(),
        config: &cfg,
    };
    let current: BTreeMap<String, u8> = fuses_328p_defaults()
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
    let mut desired = current.clone();
    desired.insert("hfuse".into(), 0xD9 & 0x7F); // RSTDISBL programmed
    let input = FusePlanInput {
        desired,
        current: Some(current),
    };
    let plan = plan_write(&avr, &input).unwrap();
    assert!(!plan.blocked.is_empty());
    let err = write_fuses(
        &avr,
        &input,
        &plan.token,
        &tmp.path().join("w"),
        &CancelToken::new(),
        &mut no_lines(),
    )
    .unwrap_err();
    assert!(matches!(err, CoreError::Safety(_)));
    assert!(argv_log(tmp.path()).is_none());
}

// ---------------------------------------------------------------- avr-gcc

#[derive(Default)]
struct Collect(Vec<String>);
impl BuildSink for Collect {
    fn stage(&mut self, _: BuildStage) {}
    fn line(&mut self, _: Stream, text: &str) {
        self.0.push(text.to_string());
    }
}

/// Fake toolchain directory: avr-gcc answers `-dumpversion` and the `-E`
/// device probe, then runs `compile_body` for the real compile.
fn fake_gcc(dir: &Path, probe_ok: bool, compile_body: &str) -> PathBuf {
    let probe = if probe_ok {
        "exit 0"
    } else {
        "echo 'avr-gcc: error: unknown MCU' >&2; exit 1"
    };
    let gcc = script(
        dir,
        "avr-gcc",
        &format!(
            "case \"$1\" in -dumpversion) echo 14.1.0; exit 0;; esac\nfor a in \"$@\"; do [ \"$a\" = -E ] && {{ {probe}; }}; done\n{compile_body}"
        ),
    );
    script(dir, "avr-objcopy", "exit 0");
    script(dir, "avr-size", "exit 0");
    gcc
}

fn new_project(parent: &Path) -> PathBuf {
    let reg = DeviceRegistry::builtin().unwrap();
    ProjectRepository::create(
        &reg,
        &CreateProjectRequest {
            parent_dir: parent.to_path_buf(),
            name: "mock".into(),
            mcu: "atmega328p".into(),
            package: "PDIP-28".into(),
            template: Template::Blink,
        },
    )
    .unwrap()
    .root
}

#[test]
fn compiler_errors_become_located_diagnostics_and_fail_the_build() {
    let tmp = tempfile::tempdir().unwrap();
    let tools = tmp.path().join("bin");
    std::fs::create_dir_all(&tools).unwrap();
    let gcc = fake_gcc(
        &tools,
        true,
        "echo \"$PWD/src/generated/main.c:12:5: error: 'counter' undeclared (first use in this function)\" >&2; exit 1",
    );
    let root = new_project(tmp.path());
    let reg = DeviceRegistry::builtin().unwrap();
    let cat = BlockCatalog::builtin().unwrap();
    let settings = ToolchainSettings {
        avr_gcc_path: Some(gcc),
        ..Default::default()
    };
    let req = BuildRequest {
        root: &root,
        registry: &reg,
        catalog: &cat,
        settings: &settings,
        force_regenerate: false,
    };
    let mut sink = Collect::default();
    let report = run_build(&req, &mut sink, &CancelToken::new());
    assert!(!report.success);
    assert_eq!(report.stage, BuildStage::Compile);
    assert!(report.artifacts.is_none() && report.memory.is_none());
    let d = report
        .diagnostics
        .iter()
        .find(|d| d.message.contains("counter"))
        .expect("diagnostic");
    assert_eq!(d.file.as_deref(), Some("src/generated/main.c"));
    assert_eq!(d.line, Some(12));
    assert!(
        sink.0.iter().any(|l| l.contains("undeclared")),
        "compiler output is streamed"
    );
}

#[test]
fn unsupported_mcu_is_reported_at_the_toolchain_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let tools = tmp.path().join("bin");
    std::fs::create_dir_all(&tools).unwrap();
    let gcc = fake_gcc(&tools, false, "echo 'should not compile' >&2; exit 1");
    let root = new_project(tmp.path());
    let reg = DeviceRegistry::builtin().unwrap();
    let cat = BlockCatalog::builtin().unwrap();
    let settings = ToolchainSettings {
        avr_gcc_path: Some(gcc),
        ..Default::default()
    };
    let req = BuildRequest {
        root: &root,
        registry: &reg,
        catalog: &cat,
        settings: &settings,
        force_regenerate: false,
    };
    let mut sink = Collect::default();
    let report = run_build(&req, &mut sink, &CancelToken::new());
    assert!(!report.success);
    assert_eq!(report.stage, BuildStage::Toolchain);
    assert!(report.message.contains("-mmcu=atmega328p"), "{}", report.message);
    assert!(!sink.0.iter().any(|l| l.contains("should not compile")));
}

#[test]
fn missing_compiler_fails_cleanly() {
    let tmp = tempfile::tempdir().unwrap();
    let root = new_project(tmp.path());
    let reg = DeviceRegistry::builtin().unwrap();
    let cat = BlockCatalog::builtin().unwrap();
    let settings = ToolchainSettings {
        avr_gcc_path: Some(tmp.path().join("nope/avr-gcc")),
        ..Default::default()
    };
    let req = BuildRequest {
        root: &root,
        registry: &reg,
        catalog: &cat,
        settings: &settings,
        force_regenerate: false,
    };
    let report = run_build(&req, &mut Collect::default(), &CancelToken::new());
    assert!(!report.success);
    assert_eq!(report.stage, BuildStage::Toolchain);
    assert!(
        report.message.contains("avr-gcc was not found"),
        "{}",
        report.message
    );
}
