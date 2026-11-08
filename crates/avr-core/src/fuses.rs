//! Fuse decoding and the read → review → confirm → write → verify workflow.
//!
//! Fuse writes can make a device unrecoverable without special hardware, so:
//! * meanings come exclusively from the selected device's definition;
//! * a write requires a plan whose confirmation token covers the exact target,
//!   programmer, arguments and bytes; any change invalidates the token;
//! * values marked `blocked` (ISP disable, RESET disable, reserved codes) are refused;
//! * fuses are never written as part of a firmware upload;
//! * after writing, fuses are read back and compared.

use crate::device_registry::{DeviceDefinition, FuseRisk};
use crate::process::{CancelToken, ProcessOutput, Stream, run_process};
use crate::programmer::{Avrdude, ProgrammerKind, diagnose_failure, programmer_def};
use crate::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedField {
    pub byte: String,
    pub field: String,
    pub label: String,
    pub raw: u8,
    pub meaning: String,
    pub risk: Option<FuseRisk>,
    pub warning: Option<String>,
}

pub fn decode(device: &DeviceDefinition, bytes: &BTreeMap<String, u8>) -> Vec<DecodedField> {
    let mut out = vec![];
    for b in &device.fuses.bytes {
        let Some(v) = bytes.get(&b.id) else { continue };
        for f in &b.fields {
            let raw = f.extract(*v);
            let opt = f.option(raw);
            out.push(DecodedField {
                byte: b.id.clone(),
                field: f.id.clone(),
                label: f.label.clone(),
                raw,
                meaning: opt
                    .map(|o| o.label.clone())
                    .unwrap_or_else(|| format!("undefined value {raw}")),
                risk: opt.and_then(|o| o.risk).or(if opt.is_none() {
                    Some(FuseRisk::Blocked)
                } else {
                    None
                }),
                warning: opt.and_then(|o| o.warning.clone()),
            });
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuseByteChange {
    pub id: String,
    pub label: String,
    pub current: Option<u8>,
    pub desired: u8,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuseWritePlan {
    pub device: String,
    pub signature: String,
    pub programmer: String,
    pub bytes: Vec<FuseByteChange>,
    pub warnings: Vec<String>,
    pub blocked: Vec<String>,
    pub command: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FusePlanInput {
    pub desired: BTreeMap<String, u8>,
    /// Values read from the device immediately before planning, when available.
    pub current: Option<BTreeMap<String, u8>>,
}

fn write_args(
    device: &DeviceDefinition,
    desired: &BTreeMap<String, u8>,
    current: Option<&BTreeMap<String, u8>>,
) -> Vec<OsString> {
    let mut a = vec![];
    for b in &device.fuses.bytes {
        let Some(v) = desired.get(&b.id) else { continue };
        let unchanged = current
            .and_then(|c| c.get(&b.id))
            .is_some_and(|c| c & b.used_mask == v & b.used_mask);
        if unchanged {
            continue;
        }
        // Unused bits are written as 1 (unprogrammed).
        let value = v | !b.used_mask;
        a.push("-U".into());
        a.push(format!("{}:w:0x{:02X}:m", b.id, value).into());
    }
    a
}

/// Produces a reviewable write plan. Never touches hardware.
pub fn plan_write(avr: &Avrdude, input: &FusePlanInput) -> CoreResult<FuseWritePlan> {
    let device = avr.device;
    let def = programmer_def(&avr.config.programmer)?;
    let mut blocked = vec![];
    if def.kind == ProgrammerKind::Bootloader {
        blocked.push(format!(
            "{} cannot write fuses. Use an ISP programmer.",
            def.label
        ));
    }
    for id in input.desired.keys() {
        if device.fuse_byte(id).is_none() {
            return Err(CoreError::InvalidRequest(format!(
                "{} has no fuse byte '{id}'",
                device.name
            )));
        }
    }
    let mut bytes = vec![];
    let mut warnings = vec![format!(
        "Fuse meanings below apply only to the {}. Bits are active-low: 0 = programmed.",
        device.name
    )];
    for b in &device.fuses.bytes {
        let Some(desired) = input.desired.get(&b.id).copied() else {
            blocked.push(format!("No desired value for {}", b.label));
            continue;
        };
        let current = input.current.as_ref().and_then(|c| c.get(&b.id)).copied();
        let changed = current.is_none_or(|c| c & b.used_mask != desired & b.used_mask);
        for f in &b.fields {
            let raw = f.extract(desired);
            let Some(opt) = f.option(raw) else {
                blocked.push(format!("{}: value {raw} has no defined meaning", f.id));
                continue;
            };
            let field_changed = current.is_none_or(|c| f.extract(c) != raw);
            match opt.risk {
                Some(FuseRisk::Blocked) => blocked.push(format!(
                    "{} = {}: {}",
                    f.id,
                    opt.label,
                    opt.warning.clone().unwrap_or_else(|| "not allowed".into())
                )),
                Some(FuseRisk::Danger) if field_changed => warnings.push(format!(
                    "DANGER — {}: {}. {}",
                    f.id,
                    opt.label,
                    opt.warning.clone().unwrap_or_default()
                )),
                Some(FuseRisk::Caution) if field_changed => warnings.push(format!(
                    "Caution — {}: {}. {}",
                    f.id,
                    opt.label,
                    opt.warning.clone().unwrap_or_default()
                )),
                _ => {}
            }
            if field_changed && (f.id == "CKSEL" || f.id == "SUT" || f.id == "CKDIV8" || f.id == "CKOPT") {
                warnings.push(format!(
                    "{} changes the clock configuration. If the selected clock is not physically present, the chip will stop responding to ISP until a valid clock is supplied.",
                    f.id
                ));
            }
            if field_changed && (f.id == "BOOTRST" || f.id == "BOOTSZ") {
                warnings.push(format!("{} changes the boot/bootloader configuration.", f.id));
            }
        }
        bytes.push(FuseByteChange {
            id: b.id.clone(),
            label: b.label.clone(),
            current,
            desired,
            changed,
        });
    }
    if input.current.is_none() {
        warnings.push(
            "Current fuse values were not read from the device; all fuse bytes will be written.".into(),
        );
    }
    let extra = write_args(device, &input.desired, input.current.as_ref());
    let command = avr.spec(extra, Duration::from_secs(60))?.display();
    let token = crate::codegen::content_hash(
        format!(
            "{}|{}|{:?}|{:?}|{}",
            device.id, avr.config.programmer, input.desired, input.current, command
        )
        .as_bytes(),
    );
    Ok(FuseWritePlan {
        device: device.name.clone(),
        signature: device.signature_hex(),
        programmer: def.label.to_string(),
        bytes,
        warnings,
        blocked,
        command,
        token,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuseReadResult {
    pub success: bool,
    pub values: BTreeMap<String, u8>,
    pub message: String,
    pub hints: Vec<String>,
    pub output: ProcessOutput,
}

fn parse_fuse_file(text: &str) -> Option<u8> {
    let t = text
        .trim()
        .split(|c: char| c.is_whitespace() || c == ',')
        .next()?
        .to_lowercase();
    let t = t.trim_start_matches("0x");
    u8::from_str_radix(t, 16).ok()
}

/// Reads every fuse byte into files in `work_dir` (one AVRDUDE run).
pub fn read_fuses(
    avr: &Avrdude,
    work_dir: &Path,
    cancel: &CancelToken,
    on_line: &mut dyn FnMut(Stream, &str),
) -> CoreResult<FuseReadResult> {
    let def = programmer_def(&avr.config.programmer)?;
    if def.kind == ProgrammerKind::Bootloader {
        return Err(CoreError::InvalidRequest(format!(
            "{} cannot read fuses. Use an ISP programmer.",
            def.label
        )));
    }
    std::fs::create_dir_all(work_dir).map_err(|e| CoreError::io(work_dir, e))?;
    let mut extra = vec![];
    let mut files = vec![];
    for b in &avr.device.fuses.bytes {
        let f = work_dir.join(format!("{}.txt", b.id));
        let _ = std::fs::remove_file(&f);
        let mut u = OsString::from(format!("{}:r:", b.id));
        u.push(f.as_os_str());
        u.push(":h");
        extra.push("-U".into());
        extra.push(u);
        files.push((b, f));
    }
    let out = run_process(&avr.spec(extra, Duration::from_secs(60))?, cancel, on_line)?;
    let mut values = BTreeMap::new();
    for (b, f) in &files {
        if let Some(v) = std::fs::read_to_string(f)
            .ok()
            .as_deref()
            .and_then(parse_fuse_file)
        {
            values.insert(b.id.clone(), v);
        }
    }
    let success = out.success() && values.len() == files.len();
    let message = if success {
        files
            .iter()
            .map(|(b, _)| format!("{} = 0x{:02X}", b.id, values[&b.id]))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "Could not read the fuses".into()
    };
    let hints = if success {
        vec![]
    } else {
        diagnose_failure(&out.combined(), Some(def))
    };
    Ok(FuseReadResult {
        success,
        values,
        message,
        hints,
        output: out,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuseWriteResult {
    pub success: bool,
    pub verified: bool,
    pub readback: BTreeMap<String, u8>,
    pub message: String,
    pub hints: Vec<String>,
    pub write_output: Option<ProcessOutput>,
    pub verify_output: Option<ProcessOutput>,
}

/// Executes a previously reviewed plan. The plan is recomputed and must
/// produce the same token, otherwise the write is refused.
pub fn write_fuses(
    avr: &Avrdude,
    input: &FusePlanInput,
    confirmed_token: &str,
    work_dir: &Path,
    cancel: &CancelToken,
    on_line: &mut dyn FnMut(Stream, &str),
) -> CoreResult<FuseWriteResult> {
    let plan = plan_write(avr, input)?;
    if plan.token != confirmed_token {
        return Err(CoreError::Safety(
            "the fuse write request differs from the reviewed plan; review it again before writing".into(),
        ));
    }
    if !plan.blocked.is_empty() {
        return Err(CoreError::Safety(format!(
            "fuse write blocked: {}",
            plan.blocked.join("; ")
        )));
    }
    let extra = write_args(avr.device, &input.desired, input.current.as_ref());
    if extra.is_empty() {
        return Err(CoreError::InvalidRequest(
            "nothing to write: the desired values match the device".into(),
        ));
    }
    let out = run_process(&avr.spec(extra, Duration::from_secs(60))?, cancel, on_line)?;
    if !out.success() {
        let hints = diagnose_failure(&out.combined(), programmer_def(&avr.config.programmer).ok());
        return Ok(FuseWriteResult {
            success: false,
            verified: false,
            readback: BTreeMap::new(),
            message: if out.cancelled {
                "Fuse write cancelled. The fuse state is unknown; read the fuses before doing anything else."
                    .into()
            } else {
                "AVRDUDE reported a failure while writing fuses. The fuse state is unknown; read the fuses to check.".into()
            },
            hints,
            write_output: Some(out),
            verify_output: None,
        });
    }
    let readback = read_fuses(avr, work_dir, &CancelToken::new(), on_line)?;
    let mut mismatches = vec![];
    for b in &avr.device.fuses.bytes {
        if let Some(d) = input.desired.get(&b.id) {
            match readback.values.get(&b.id) {
                Some(r) if r & b.used_mask == d & b.used_mask => {}
                Some(r) => mismatches.push(format!("{}: wrote 0x{:02X}, read 0x{:02X}", b.id, d, r)),
                None => mismatches.push(format!("{}: could not be read back", b.id)),
            }
        }
    }
    let verified = readback.success && mismatches.is_empty();
    Ok(FuseWriteResult {
        success: verified,
        verified,
        readback: readback.values.clone(),
        message: if verified {
            "Fuses written and verified by read-back".into()
        } else {
            format!("Read-back verification FAILED: {}", mismatches.join("; "))
        },
        hints: readback.hints.clone(),
        write_output: Some(out),
        verify_output: Some(readback.output),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_registry::DeviceRegistry;
    use crate::project_store::ProgrammerConfig;
    use crate::toolchains::AvrdudeTool;
    use std::path::PathBuf;

    fn tool() -> AvrdudeTool {
        AvrdudeTool {
            path: PathBuf::from("/usr/bin/avrdude"),
            version: None,
            conf: None,
        }
    }

    #[test]
    fn decodes_328p_defaults() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega328p").unwrap();
        let fields = decode(d, &d.default_fuses());
        let ckdiv8 = fields.iter().find(|f| f.field == "CKDIV8").unwrap();
        assert_eq!(ckdiv8.raw, 0);
        assert!(ckdiv8.meaning.starts_with("Programmed"));
        let spien = fields.iter().find(|f| f.field == "SPIEN").unwrap();
        assert_eq!(spien.raw, 0);
    }

    #[test]
    fn decodes_32a_without_extended_fuse() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega32a").unwrap();
        let fields = decode(d, &d.default_fuses());
        assert!(fields.iter().all(|f| f.byte != "efuse"));
        let cksel = fields.iter().find(|f| f.field == "CKSEL").unwrap();
        assert!(cksel.meaning.contains("1 MHz"));
        assert!(fields.iter().any(|f| f.field == "JTAGEN"));
        assert!(!fields.iter().any(|f| f.field == "RSTDISBL"));
    }

    #[test]
    fn plan_blocks_isp_disable_and_bootloader_programmers() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega328p").unwrap();
        let t = tool();
        let cfg = ProgrammerConfig {
            programmer: "usbasp".into(),
            port: "usb".into(),
            baud: None,
            bitclock: None,
        };
        let avr = Avrdude {
            tool: &t,
            device: d,
            config: &cfg,
        };
        let mut desired = d.default_fuses();
        desired.insert("hfuse".into(), 0xD9 | 0x20); // SPIEN unprogrammed
        let plan = plan_write(
            &avr,
            &FusePlanInput {
                desired: desired.clone(),
                current: None,
            },
        )
        .unwrap();
        assert!(plan.blocked.iter().any(|b| b.starts_with("SPIEN")));
        desired.insert("hfuse".into(), 0xD9 & 0x7F); // RSTDISBL programmed
        let plan = plan_write(
            &avr,
            &FusePlanInput {
                desired,
                current: None,
            },
        )
        .unwrap();
        assert!(plan.blocked.iter().any(|b| b.starts_with("RSTDISBL")));

        let boot = ProgrammerConfig {
            programmer: "arduino".into(),
            port: "/dev/ttyACM0".into(),
            baud: None,
            bitclock: None,
        };
        let avr = Avrdude {
            tool: &t,
            device: d,
            config: &boot,
        };
        let plan = plan_write(
            &avr,
            &FusePlanInput {
                desired: d.default_fuses(),
                current: None,
            },
        )
        .unwrap();
        assert!(!plan.blocked.is_empty());
    }

    #[test]
    fn plan_lists_only_changed_bytes_and_warns_on_clock() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega328p").unwrap();
        let t = tool();
        let cfg = ProgrammerConfig {
            programmer: "usbasp".into(),
            port: "usb".into(),
            baud: None,
            bitclock: None,
        };
        let avr = Avrdude {
            tool: &t,
            device: d,
            config: &cfg,
        };
        let current = d.default_fuses();
        let mut desired = current.clone();
        desired.insert("lfuse".into(), 0xFF); // 16 MHz crystal, no CKDIV8
        let plan = plan_write(
            &avr,
            &FusePlanInput {
                desired: desired.clone(),
                current: Some(current.clone()),
            },
        )
        .unwrap();
        assert!(plan.blocked.is_empty());
        assert!(plan.command.contains("lfuse:w:0xFF:m"));
        assert!(!plan.command.contains("hfuse:w"));
        assert!(plan.warnings.iter().any(|w| w.contains("clock")));
        // A different request yields a different token.
        let mut other = desired.clone();
        other.insert("lfuse".into(), 0xF7);
        let plan2 = plan_write(
            &avr,
            &FusePlanInput {
                desired: other,
                current: Some(current),
            },
        )
        .unwrap();
        assert_ne!(plan.token, plan2.token);
    }

    #[test]
    fn efuse_unused_bits_written_as_ones() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega328p").unwrap();
        let mut desired = BTreeMap::new();
        desired.insert("efuse".to_string(), 0x05u8);
        let args = write_args(d, &desired, None);
        assert_eq!(args[1], OsString::from("efuse:w:0xFD:m"));
    }

    #[test]
    fn fuse_file_parsing() {
        assert_eq!(parse_fuse_file("0x62\n"), Some(0x62));
        assert_eq!(parse_fuse_file("0xd9"), Some(0xD9));
        assert_eq!(parse_fuse_file(""), None);
    }
}
