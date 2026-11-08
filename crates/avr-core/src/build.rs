//! Build pipeline: validate → generate → check toolchain → compile → HEX → size.

use crate::codegen::{CodegenOptions, GeneratedCode, generate, write_generated};
use crate::config_validation::validate_config;
use crate::device_registry::{DeviceDefinition, DeviceRegistry};
use crate::diagnostics::{Diagnostic, Severity, has_errors, sort_diagnostics};
use crate::graph::catalog::BlockCatalog;
use crate::graph::compile_graph;
use crate::graph::ir::IrProgram;
use crate::memory::{MemoryReport, analyze, parse_nm, parse_size_sysv};
use crate::process::{CancelToken, ProcessOutput, ProcessSpec, Stream, run_process};
use crate::project_store::{Project, ProjectRepository};
use crate::toolchains::{
    GccToolchain, ToolchainProfile, ToolchainSettings, detect, probe_mcu, resolve_profile,
};
use crate::{CoreError, CoreResult};
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct ProjectAnalysis {
    pub diagnostics: Vec<Diagnostic>,
    pub ir: Option<IrProgram>,
}

/// Runs configuration and graph validation and lowers the graph to IR.
pub fn analyze_project(
    device: &DeviceDefinition,
    catalog: &BlockCatalog,
    project: &Project,
) -> ProjectAnalysis {
    let mut diagnostics = validate_config(device, project);
    let (ir, graph_diags) = compile_graph(catalog, device, project);
    diagnostics.extend(graph_diags);
    sort_diagnostics(&mut diagnostics);
    let ir = if has_errors(&diagnostics) { None } else { ir };
    ProjectAnalysis { diagnostics, ir }
}

/// Generates code for a project in memory (used for previews).
pub fn preview_code(
    device: &DeviceDefinition,
    catalog: &BlockCatalog,
    project: &Project,
    legacy: bool,
) -> (Option<GeneratedCode>, Vec<Diagnostic>) {
    let a = analyze_project(device, catalog, project);
    let code = a.ir.as_ref().map(|ir| {
        generate(
            device,
            project,
            ir,
            CodegenOptions {
                legacy_toolchain: legacy,
            },
        )
    });
    (code, a.diagnostics)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildStage {
    Validate,
    Toolchain,
    Generate,
    Compile,
    Size,
    Hex,
    Done,
}

pub trait BuildSink {
    fn stage(&mut self, stage: BuildStage);
    fn line(&mut self, stream: Stream, text: &str);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildArtifacts {
    pub elf: PathBuf,
    pub hex: PathBuf,
    pub eep: Option<PathBuf>,
    pub map: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReport {
    pub success: bool,
    pub cancelled: bool,
    pub stage: BuildStage,
    pub message: String,
    pub diagnostics: Vec<Diagnostic>,
    pub artifacts: Option<BuildArtifacts>,
    pub memory: Option<MemoryReport>,
    pub commands: Vec<String>,
    pub toolchain: Option<String>,
    pub log_path: Option<PathBuf>,
    pub duration_ms: u64,
}

pub struct BuildRequest<'a> {
    pub root: &'a Path,
    pub registry: &'a DeviceRegistry,
    pub catalog: &'a BlockCatalog,
    pub settings: &'a ToolchainSettings,
    /// Overwrite generated files even if they were edited by hand.
    pub force_regenerate: bool,
}

pub const ELF_NAME: &str = "firmware.elf";
pub const HEX_NAME: &str = "firmware.hex";

/// Parses GCC/ld diagnostics (`file:line:col: severity: message`).
pub fn parse_compiler_output(output: &str, root: &Path) -> Vec<Diagnostic> {
    let re =
        Regex::new(r"^(.+?):(\d+):(?:(\d+):)?\s*(fatal error|error|warning|note):\s*(.*)$").expect("regex");
    let undefined = Regex::new(r"undefined reference to").expect("regex");
    let mut out = vec![];
    for line in output.lines() {
        if let Some(c) = re.captures(line) {
            let sev = match &c[4] {
                "warning" => Severity::Warning,
                "note" => Severity::Info,
                _ => Severity::Error,
            };
            let file = PathBuf::from(&c[1]);
            let file = file.strip_prefix(root).map(Path::to_path_buf).unwrap_or(file);
            let mut d = Diagnostic::new(sev, "compiler", c[5].to_string());
            d.file = Some(file.to_string_lossy().replace('\\', "/"));
            d.line = c[2].parse().ok();
            d.column = c.get(3).and_then(|m| m.as_str().parse().ok());
            out.push(d);
        } else if undefined.is_match(line)
            || line.contains("ld returned")
            || line.contains("region `text' overflowed")
        {
            out.push(Diagnostic::error("linker", line.trim().to_string()));
        }
    }
    out
}

struct Run<'a> {
    sink: &'a mut dyn BuildSink,
    log: String,
    commands: Vec<String>,
    cancel: &'a CancelToken,
}

impl Run<'_> {
    fn say(&mut self, text: &str) {
        self.sink.line(Stream::Stdout, text);
        self.log.push_str(text);
        self.log.push('\n');
    }
    fn exec(&mut self, spec: ProcessSpec) -> CoreResult<ProcessOutput> {
        let cmd = spec.display();
        self.say(&format!("$ {cmd}"));
        self.commands.push(cmd);
        let mut buf = String::new();
        let sink = &mut *self.sink;
        let out = run_process(&spec, self.cancel, &mut |s, l| {
            sink.line(s, l);
            buf.push_str(l);
            buf.push('\n');
        })?;
        self.log.push_str(&buf);
        let status = match (out.exit_code, out.timed_out, out.cancelled) {
            (_, true, _) => "timed out".to_string(),
            (_, _, true) => "cancelled".to_string(),
            (Some(c), _, _) => format!("exit code {c}"),
            (None, _, _) => "terminated by signal".to_string(),
        };
        self.say(&format!("  -> {status} ({} ms)", out.duration_ms));
        Ok(out)
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn run_build(req: &BuildRequest, sink: &mut dyn BuildSink, cancel: &CancelToken) -> BuildReport {
    let start = Instant::now();
    let mut run = Run {
        sink,
        log: String::new(),
        commands: vec![],
        cancel,
    };
    let mut report = BuildReport {
        success: false,
        cancelled: false,
        stage: BuildStage::Validate,
        message: String::new(),
        diagnostics: vec![],
        artifacts: None,
        memory: None,
        commands: vec![],
        toolchain: None,
        log_path: None,
        duration_ms: 0,
    };
    let result = build_inner(req, &mut run, &mut report);
    if let Err(e) = result {
        report.message = e.to_string();
        run.say(&format!("BUILD FAILED: {e}"));
    }
    report.cancelled = cancel.is_cancelled();
    if report.cancelled {
        report.success = false;
        report.message = "Build cancelled".into();
    }
    report.duration_ms = start.elapsed().as_millis() as u64;
    report.commands = run.commands.clone();
    run.say(&format!(
        "== Build {} in {} ms ==",
        if report.success { "succeeded" } else { "failed" },
        report.duration_ms
    ));
    let logs = req.root.join("logs");
    let log_path = logs.join(format!("build-{}.log", now_ms()));
    if std::fs::create_dir_all(&logs).is_ok() && std::fs::write(&log_path, &run.log).is_ok() {
        report.log_path = Some(log_path);
        prune_logs(&logs, "build-", 20);
    }
    report
}

pub fn prune_logs(dir: &Path, prefix: &str, keep: usize) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut logs: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(prefix))
            })
            .collect();
        logs.sort();
        let excess = logs.len().saturating_sub(keep);
        for p in logs.into_iter().take(excess) {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn build_inner(req: &BuildRequest, run: &mut Run, report: &mut BuildReport) -> CoreResult<()> {
    // 1. Validate the project as saved on disk (the authoritative state).
    run.sink.stage(BuildStage::Validate);
    run.say("== Validating project ==");
    let loaded = ProjectRepository::load(req.registry, req.root)?;
    let project = &loaded.project;
    let device = req.registry.get(&project.manifest.device.mcu)?;
    let analysis = analyze_project(device, req.catalog, project);
    report.diagnostics = analysis.diagnostics.clone();
    for d in &analysis.diagnostics {
        run.say(&format!("  {:?}: {}", d.severity, d.message));
    }
    let Some(ir) = analysis.ir else {
        return Err(CoreError::InvalidProject(
            "the project has validation errors".into(),
        ));
    };

    // 2. Toolchain.
    report.stage = BuildStage::Toolchain;
    run.sink.stage(BuildStage::Toolchain);
    run.say("== Checking toolchain ==");
    let status = detect(req.settings);
    for p in &status.problems {
        run.say(&format!("  note: {p}"));
    }
    let gcc: GccToolchain = status.gcc.ok_or_else(|| {
        CoreError::Toolchain(
            "avr-gcc was not found. Install an AVR-GCC toolchain or set its path in Settings.".into(),
        )
    })?;
    let profile = resolve_profile(&project.manifest.toolchain.profile, &gcc)?;
    report.toolchain = Some(format!(
        "{} — avr-gcc {} ({})",
        profile.label(),
        gcc.version,
        gcc.avr_gcc.display()
    ));
    run.say(&format!("  {}", report.toolchain.clone().unwrap_or_default()));
    let build_dir = req.root.join("build");
    probe_mcu(&gcc, &device.compiler.mmcu, &build_dir)?;
    run.say(&format!("  -mmcu={} supported", device.compiler.mmcu));

    // 3. Generate sources.
    report.stage = BuildStage::Generate;
    run.sink.stage(BuildStage::Generate);
    run.say("== Generating C sources ==");
    let code = generate(
        device,
        project,
        &ir,
        CodegenOptions {
            legacy_toolchain: profile.is_legacy(),
        },
    );
    let written = write_generated(req.root, &code, req.force_regenerate)?;
    for f in &written.written {
        run.say(&format!("  wrote {f}"));
    }
    for f in &written.unchanged {
        run.say(&format!("  unchanged {f}"));
    }
    if run.cancel.is_cancelled() {
        return Ok(());
    }

    // 4. Compile and link.
    report.stage = BuildStage::Compile;
    run.sink.stage(BuildStage::Compile);
    run.say("== Compiling ==");
    let elf = build_dir.join(ELF_NAME);
    let hex = build_dir.join(HEX_NAME);
    let map = build_dir.join("firmware.map");
    for stale in [&elf, &hex, &build_dir.join("firmware.eep")] {
        let _ = std::fs::remove_file(stale);
    }
    let sources: Vec<PathBuf> = code
        .files
        .iter()
        .filter(|f| f.path.ends_with(".c"))
        .map(|f| req.root.join(&f.path))
        .collect();
    let args = compiler_args(
        &device.compiler.mmcu,
        code.f_cpu,
        profile,
        &project.manifest.toolchain.optimization,
        &req.root.join(crate::codegen::GENERATED_DIR),
        &req.root.join("include"),
        &elf,
        &map,
        &sources,
    )?;
    let out = run.exec(
        ProcessSpec::new(&gcc.avr_gcc, args)
            .cwd(req.root)
            .timeout(Duration::from_secs(300)),
    )?;
    let compiler_diags = parse_compiler_output(&out.combined(), req.root);
    report.diagnostics.extend(compiler_diags);
    if !out.success() || !elf.is_file() {
        return Err(CoreError::Toolchain(match out.exit_code {
            Some(c) => format!("avr-gcc failed with exit code {c}"),
            None => "avr-gcc did not complete".into(),
        }));
    }

    // 5. Size analysis.
    report.stage = BuildStage::Size;
    run.sink.stage(BuildStage::Size);
    run.say("== Analyzing memory ==");
    let size_out = run.exec(ProcessSpec::new(
        &gcc.size,
        ["-A".into(), elf.clone().into_os_string()],
    ))?;
    if !size_out.success() {
        return Err(CoreError::Toolchain("avr-size failed".into()));
    }
    let sections = parse_size_sysv(&size_out.stdout);
    let symbols = match &gcc.nm {
        Some(nm) => {
            let spec = ProcessSpec::new(
                nm,
                ["-S".into(), "--size-sort".into(), elf.clone().into_os_string()],
            );
            let o = run_process(&spec, run.cancel, &mut |_, _| {})?;
            if o.success() { parse_nm(&o.stdout) } else { vec![] }
        }
        None => vec![],
    };
    let mem = analyze(device, sections, symbols);
    run.say(&format!(
        "  Flash {} / {} bytes, SRAM (static) {} / {} bytes, EEPROM {} / {} bytes",
        mem.flash.used,
        mem.flash.total,
        mem.sram_static.used,
        mem.sram_static.total,
        mem.eeprom.used,
        mem.eeprom.total
    ));
    let eeprom_used = mem.eeprom.used;
    let too_big = mem.flash.used > mem.flash.total || mem.sram_static.used > mem.sram_static.total;
    report.memory = Some(mem);
    if too_big {
        return Err(CoreError::Toolchain(
            "the program does not fit in the device memory".into(),
        ));
    }

    // 6. Intel HEX (+ EEPROM image when used).
    report.stage = BuildStage::Hex;
    run.sink.stage(BuildStage::Hex);
    run.say("== Creating Intel HEX ==");
    let hex_args: Vec<std::ffi::OsString> = vec![
        "-O".into(),
        "ihex".into(),
        "-R".into(),
        ".eeprom".into(),
        "-R".into(),
        ".fuse".into(),
        "-R".into(),
        ".lock".into(),
        "-R".into(),
        ".signature".into(),
        elf.clone().into_os_string(),
        hex.clone().into_os_string(),
    ];
    let o = run.exec(ProcessSpec::new(&gcc.objcopy, hex_args))?;
    if !o.success() || !hex.is_file() {
        return Err(CoreError::Toolchain(
            "avr-objcopy failed to create the HEX file".into(),
        ));
    }
    let mut eep = None;
    if eeprom_used > 0 {
        let eep_path = build_dir.join("firmware.eep");
        let eep_args: Vec<std::ffi::OsString> = vec![
            "-O".into(),
            "ihex".into(),
            "-j".into(),
            ".eeprom".into(),
            "--set-section-flags=.eeprom=alloc,load".into(),
            "--change-section-lma".into(),
            ".eeprom=0".into(),
            elf.clone().into_os_string(),
            eep_path.clone().into_os_string(),
        ];
        let o = run.exec(ProcessSpec::new(&gcc.objcopy, eep_args))?;
        if !o.success() {
            return Err(CoreError::Toolchain(
                "avr-objcopy failed to create the EEPROM image".into(),
            ));
        }
        eep = Some(eep_path);
    }
    report.artifacts = Some(BuildArtifacts { elf, hex, eep, map });
    report.stage = BuildStage::Done;
    run.sink.stage(BuildStage::Done);
    report.success = !run.cancel.is_cancelled();
    report.message = "Build succeeded".into();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn compiler_args(
    mmcu: &str,
    f_cpu: u32,
    profile: ToolchainProfile,
    optimization: &str,
    generated_dir: &Path,
    include_dir: &Path,
    elf: &Path,
    map: &Path,
    sources: &[PathBuf],
) -> CoreResult<Vec<std::ffi::OsString>> {
    if !matches!(optimization, "s" | "0" | "1" | "2" | "3") {
        return Err(CoreError::InvalidProject(format!(
            "invalid optimization level '{optimization}'"
        )));
    }
    let mut a: Vec<std::ffi::OsString> = vec![
        format!("-mmcu={mmcu}").into(),
        format!("-DF_CPU={f_cpu}UL").into(),
        format!("-O{optimization}").into(),
        format!("-std={}", profile.c_std()).into(),
        "-Wall".into(),
        "-Wextra".into(),
        "-funsigned-char".into(),
        "-funsigned-bitfields".into(),
        "-fshort-enums".into(),
        "-ffunction-sections".into(),
        "-fdata-sections".into(),
        "-I".into(),
        generated_dir.as_os_str().to_owned(),
        "-I".into(),
        include_dir.as_os_str().to_owned(),
        "-Wl,--gc-sections".into(),
        format!("-Wl,-Map={}", map.display()).into(),
        "-o".into(),
        elf.as_os_str().to_owned(),
    ];
    a.extend(sources.iter().map(|s| s.as_os_str().to_owned()));
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gcc_diagnostics() {
        let root = Path::new("/p");
        let out = "/p/src/generated/main.c:12:5: error: 'x' undeclared (first use in this function)\n/p/src/generated/main.c:3: warning: old style\nC:\\proj\\main.c:7:1: error: expected ';'\nmain.o: In function `main':\nmain.c:(.text+0x4): undefined reference to `foo'\ncollect2: error: ld returned 1 exit status\n";
        let d = parse_compiler_output(out, root);
        assert_eq!(d[0].file.as_deref(), Some("src/generated/main.c"));
        assert_eq!(d[0].line, Some(12));
        assert_eq!(d[0].column, Some(5));
        assert_eq!(d[1].severity, Severity::Warning);
        assert_eq!(d[1].column, None);
        assert_eq!(d[2].file.as_deref(), Some("C:/proj/main.c"));
        assert!(
            d.iter()
                .any(|x| x.code == "linker" && x.message.contains("undefined reference"))
        );
    }

    #[test]
    fn rejects_bad_optimization() {
        let p = Path::new("/x");
        assert!(
            compiler_args(
                "atmega328p",
                1,
                ToolchainProfile::ModernLinux,
                "s; rm",
                p,
                p,
                p,
                p,
                &[]
            )
            .is_err()
        );
    }
}
