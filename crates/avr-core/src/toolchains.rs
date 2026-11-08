//! Toolchain discovery and profiles (WinAVR legacy, modern Windows, modern Linux).

use crate::process::{CancelToken, ProcessSpec, find_executable, run_process};
use crate::{CoreError, CoreResult};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolchainSettings {
    /// Explicit avr-gcc executable. Sibling binutils are taken from the same directory.
    pub avr_gcc_path: Option<PathBuf>,
    pub avrdude_path: Option<PathBuf>,
    /// Passed to AVRDUDE with `-C` when set.
    pub avrdude_conf_path: Option<PathBuf>,
    /// Additional directories searched before the well-known locations and PATH.
    pub extra_search_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolchainProfile {
    WinavrLegacy,
    ModernWindows,
    ModernLinux,
    ModernOther,
}

impl ToolchainProfile {
    pub fn is_legacy(self) -> bool {
        self == ToolchainProfile::WinavrLegacy
    }
    pub fn label(self) -> &'static str {
        match self {
            ToolchainProfile::WinavrLegacy => "WinAVR-compatible legacy AVR-GCC",
            ToolchainProfile::ModernWindows => "Modern AVR-GCC (Windows)",
            ToolchainProfile::ModernLinux => "Modern AVR-GCC (Linux)",
            ToolchainProfile::ModernOther => "Modern AVR-GCC",
        }
    }
    /// C language standard supported by every compiler in the profile.
    pub fn c_std(self) -> &'static str {
        if self.is_legacy() { "gnu99" } else { "gnu11" }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GccToolchain {
    pub avr_gcc: PathBuf,
    pub version: String,
    pub objcopy: PathBuf,
    pub size: PathBuf,
    pub objdump: Option<PathBuf>,
    pub nm: Option<PathBuf>,
    pub detected_profile: ToolchainProfile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvrdudeTool {
    pub path: PathBuf,
    pub version: Option<String>,
    pub conf: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainStatus {
    pub gcc: Option<GccToolchain>,
    pub avrdude: Option<AvrdudeTool>,
    pub problems: Vec<String>,
    pub searched_dirs: Vec<PathBuf>,
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Well-known installation directories for the current OS.
pub fn well_known_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![];
    let push_versions = |dirs: &mut Vec<PathBuf>, base: PathBuf| {
        if let Ok(rd) = std::fs::read_dir(&base) {
            let mut v: Vec<_> = rd
                .flatten()
                .map(|e| e.path().join("bin"))
                .filter(|p| p.is_dir())
                .collect();
            v.sort();
            v.reverse(); // newest version first
            dirs.extend(v);
        }
    };
    if cfg!(windows) {
        for d in [
            r"C:\WinAVR-20100110\bin",
            r"C:\WinAVR\bin",
            r"C:\avr8-gnu-toolchain-win32_x86_64\bin",
            r"C:\avr8-gnu-toolchain-win32_x86\bin",
            r"C:\Program Files (x86)\Atmel\Studio\7.0\toolchain\avr8\avr8-gnu-toolchain\bin",
            r"C:\avrdude",
            r"C:\Program Files\avrdude",
        ] {
            dirs.push(PathBuf::from(d));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let base = PathBuf::from(local).join(r"Arduino15\packages\arduino\tools");
            push_versions(&mut dirs, base.join("avr-gcc"));
            push_versions(&mut dirs, base.join("avrdude"));
        }
    } else {
        for d in [
            "/usr/bin",
            "/usr/local/bin",
            "/opt/avr8-gnu-toolchain-linux_x86_64/bin",
            "/opt/avr/bin",
        ] {
            dirs.push(PathBuf::from(d));
        }
        if let Some(home) = std::env::var_os("HOME") {
            let base = PathBuf::from(home).join(".arduino15/packages/arduino/tools");
            push_versions(&mut dirs, base.join("avr-gcc"));
            push_versions(&mut dirs, base.join("avrdude"));
        }
    }
    dirs.into_iter().filter(|d| d.is_dir()).collect()
}

fn quick(program: &Path, args: &[&str]) -> Option<String> {
    let spec = ProcessSpec::new(program, args.iter().copied()).timeout(Duration::from_secs(15));
    run_process(&spec, &CancelToken::new(), &mut |_, _| {})
        .ok()
        .map(|o| o.combined())
}

pub fn parse_avrdude_version(output: &str) -> Option<String> {
    let re = Regex::new(r"(?i)avrdude\S*\s+version\s+v?(\d+\.\d+(?:\.\d+)?)").ok()?;
    re.captures(output).map(|c| c[1].to_string())
}

pub fn classify_profile(gcc_path: &Path, version: &str) -> ToolchainProfile {
    let major: u32 = version
        .split('.')
        .next()
        .and_then(|m| m.trim().parse().ok())
        .unwrap_or(0);
    let winavr = gcc_path.to_string_lossy().to_lowercase().contains("winavr");
    if winavr || major < 5 {
        ToolchainProfile::WinavrLegacy
    } else if cfg!(windows) {
        ToolchainProfile::ModernWindows
    } else if cfg!(target_os = "linux") {
        ToolchainProfile::ModernLinux
    } else {
        ToolchainProfile::ModernOther
    }
}

pub fn detect(settings: &ToolchainSettings) -> ToolchainStatus {
    let mut dirs = settings.extra_search_dirs.clone();
    dirs.extend(well_known_dirs());
    let mut problems = vec![];

    let gcc_path = match &settings.avr_gcc_path {
        Some(p) if p.is_file() => Some(p.clone()),
        Some(p) => {
            problems.push(format!("Configured avr-gcc not found: {}", p.display()));
            None
        }
        None => find_executable("avr-gcc", &dirs),
    };
    let gcc = gcc_path.and_then(|gcc| {
        let version = quick(&gcc, &["-dumpversion"])
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let Some(version) = version else {
            problems.push(format!("{} did not report a version", gcc.display()));
            return None;
        };
        let dir = gcc.parent().map(Path::to_path_buf).unwrap_or_default();
        let sibling = |name: &str| {
            let p = dir.join(exe(name));
            if p.is_file() {
                Some(p)
            } else {
                find_executable(name, &dirs)
            }
        };
        let (Some(objcopy), Some(size)) = (sibling("avr-objcopy"), sibling("avr-size")) else {
            problems.push("avr-objcopy / avr-size not found next to avr-gcc".into());
            return None;
        };
        Some(GccToolchain {
            detected_profile: classify_profile(&gcc, &version),
            avr_gcc: gcc,
            version,
            objcopy,
            size,
            objdump: sibling("avr-objdump"),
            nm: sibling("avr-nm"),
        })
    });
    if gcc.is_none() && settings.avr_gcc_path.is_none() {
        problems
            .push("avr-gcc was not found. Install an AVR-GCC toolchain or set its path in Settings.".into());
    }

    let dude_path = match &settings.avrdude_path {
        Some(p) if p.is_file() => Some(p.clone()),
        Some(p) => {
            problems.push(format!("Configured avrdude not found: {}", p.display()));
            None
        }
        None => find_executable("avrdude", &dirs),
    };
    let avrdude = dude_path.map(|path| {
        let version = quick(&path, &["-?"]).and_then(|o| parse_avrdude_version(&o));
        if let Some(c) = &settings.avrdude_conf_path
            && !c.is_file()
        {
            problems.push(format!("Configured avrdude.conf not found: {}", c.display()));
        }
        AvrdudeTool {
            path,
            version,
            conf: settings.avrdude_conf_path.clone(),
        }
    });
    if avrdude.is_none() && settings.avrdude_path.is_none() {
        problems.push("avrdude was not found. Install AVRDUDE or set its path in Settings.".into());
    }
    ToolchainStatus {
        gcc,
        avrdude,
        problems,
        searched_dirs: dirs,
    }
}

/// Resolves the profile requested by a project (`auto`, `winavr_legacy`, `modern`).
pub fn resolve_profile(requested: &str, gcc: &GccToolchain) -> CoreResult<ToolchainProfile> {
    match requested {
        "auto" => Ok(gcc.detected_profile),
        "winavr_legacy" => Ok(ToolchainProfile::WinavrLegacy),
        "modern" if gcc.detected_profile.is_legacy() => Err(CoreError::Toolchain(format!(
            "The project requires a modern AVR-GCC, but the detected compiler is avr-gcc {} at {}",
            gcc.version,
            gcc.avr_gcc.display()
        ))),
        "modern" => Ok(gcc.detected_profile),
        other => Err(CoreError::InvalidProject(format!(
            "unknown toolchain profile '{other}'"
        ))),
    }
}

/// Verifies that the compiler accepts `-mmcu` and that the device header exists.
pub fn probe_mcu(gcc: &GccToolchain, mmcu: &str, work_dir: &Path) -> CoreResult<()> {
    std::fs::create_dir_all(work_dir).map_err(|e| CoreError::io(work_dir, e))?;
    let src = work_dir.join("avsv_probe.c");
    std::fs::write(&src, "#include <avr/io.h>\nint avsv_probe;\n").map_err(|e| CoreError::io(&src, e))?;
    let out_file = work_dir.join("avsv_probe.i");
    let spec = ProcessSpec::new(
        &gcc.avr_gcc,
        [
            format!("-mmcu={mmcu}"),
            "-E".into(),
            "-o".into(),
            out_file.to_string_lossy().into(),
            src.to_string_lossy().into(),
        ],
    )
    .timeout(Duration::from_secs(30));
    let out = run_process(&spec, &CancelToken::new(), &mut |_, _| {})?;
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&out_file);
    if out.success() {
        Ok(())
    } else {
        Err(CoreError::Toolchain(format!(
            "avr-gcc {} does not support -mmcu={mmcu} or its device header is missing:\n{}",
            gcc.version,
            out.stderr.trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avrdude_versions() {
        assert_eq!(
            parse_avrdude_version("avrdude version 7.1, URL: <https://github.com/avrdudes/avrdude>")
                .as_deref(),
            Some("7.1")
        );
        assert_eq!(
            parse_avrdude_version("avrdude: Version 6.3-20171130\n").as_deref(),
            Some("6.3")
        );
        assert_eq!(
            parse_avrdude_version("Avrdude version 8.0-20240722 (09ad9f5f)").as_deref(),
            Some("8.0")
        );
        assert_eq!(parse_avrdude_version("nothing"), None);
    }

    #[test]
    fn profiles() {
        assert!(classify_profile(Path::new(r"C:\WinAVR-20100110\bin\avr-gcc.exe"), "4.3.3").is_legacy());
        assert!(classify_profile(Path::new("/opt/x/avr-gcc"), "4.8.1").is_legacy());
        assert!(!classify_profile(Path::new("/usr/bin/avr-gcc"), "14.2.0").is_legacy());
    }
}
