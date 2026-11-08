//! AVRDUDE integration: safe argument construction, signature detection,
//! firmware upload with verification, and failure diagnosis.
//!
//! Success is never inferred from a process having started: every operation
//! checks the exit status *and* the evidence AVRDUDE prints (signature bytes,
//! "bytes of flash verified").

use crate::device_registry::{DeviceDefinition, DeviceRegistry};
use crate::process::{CancelToken, ProcessOutput, ProcessSpec, Stream, run_process};
use crate::project_store::ProgrammerConfig;
use crate::toolchains::AvrdudeTool;
use crate::{CoreError, CoreResult};
use regex::Regex;
use serde::Serialize;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgrammerKind {
    /// In-system programming over SPI; can read/write fuses.
    Isp,
    /// Serial bootloader (e.g. Optiboot); flash only, no fuse access.
    Bootloader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Connection {
    Usb,
    Serial,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgrammerDef {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ProgrammerKind,
    pub connection: Connection,
    pub default_baud: Option<u32>,
    pub notes: &'static str,
}

pub const PROGRAMMERS: &[ProgrammerDef] = &[
    ProgrammerDef {
        id: "usbasp",
        label: "USBasp",
        kind: ProgrammerKind::Isp,
        connection: Connection::Usb,
        default_baud: None,
        notes: "Linux needs a udev rule for USB ID 16c0:05dc; Windows needs a libusb/WinUSB driver (e.g. via Zadig). Use a bit clock of 32 or more for targets running at 1 MHz or slower.",
    },
    ProgrammerDef {
        id: "usbtiny",
        label: "USBtinyISP",
        kind: ProgrammerKind::Isp,
        connection: Connection::Usb,
        default_baud: None,
        notes: "Linux needs a udev rule for USB ID 1781:0c9f; Windows needs a libusb driver.",
    },
    ProgrammerDef {
        id: "avrisp2",
        label: "Atmel AVRISP mkII",
        kind: ProgrammerKind::Isp,
        connection: Connection::Usb,
        default_baud: None,
        notes: "Requires a libusb-compatible driver.",
    },
    ProgrammerDef {
        id: "stk500v2",
        label: "STK500v2-compatible ISP (serial)",
        kind: ProgrammerKind::Isp,
        connection: Connection::Serial,
        default_baud: Some(115_200),
        notes: "Serial STK500v2 programmers and clones.",
    },
    ProgrammerDef {
        id: "stk500v1",
        label: "Arduino as ISP (stk500v1)",
        kind: ProgrammerKind::Isp,
        connection: Connection::Serial,
        default_baud: Some(19_200),
        notes: "An Arduino running the ArduinoISP sketch. Disable its auto-reset (10 µF from RESET to GND) if uploads fail.",
    },
    ProgrammerDef {
        id: "arduino",
        label: "Serial bootloader (Arduino / Optiboot)",
        kind: ProgrammerKind::Bootloader,
        connection: Connection::Serial,
        default_baud: Some(115_200),
        notes: "Requires a bootloader already on the chip. Uno/Optiboot uses 115200 baud, older Nano bootloaders 57600. Fuses cannot be read or written through a bootloader.",
    },
];

pub fn programmer_def(id: &str) -> CoreResult<&'static ProgrammerDef> {
    PROGRAMMERS
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| CoreError::InvalidRequest(format!("unknown programmer '{id}'")))
}

/// Validates a serial port name. Only plain device names are accepted.
pub fn validate_port(port: &str) -> CoreResult<()> {
    let linux = Regex::new(r"^/dev/[A-Za-z0-9_./-]{1,64}$").expect("regex");
    let windows = Regex::new(r"^(?i)(COM[0-9]{1,3}|\\\\\.\\COM[0-9]{1,3})$").expect("regex");
    if port.contains("..") || !(linux.is_match(port) || windows.is_match(port)) {
        return Err(CoreError::InvalidRequest(format!(
            "invalid serial port name '{port}'"
        )));
    }
    Ok(())
}

/// Builds the common AVRDUDE argument prefix after validating every value.
pub fn base_args(
    tool: &AvrdudeTool,
    device: &DeviceDefinition,
    cfg: &ProgrammerConfig,
) -> CoreResult<Vec<OsString>> {
    let def = programmer_def(&cfg.programmer)?;
    let mut a: Vec<OsString> = vec![];
    if let Some(conf) = &tool.conf {
        a.push("-C".into());
        a.push(conf.as_os_str().to_owned());
    }
    a.push("-c".into());
    a.push(def.id.into());
    a.push("-p".into());
    a.push(device.avrdude.part_id.clone().into());
    match def.connection {
        Connection::Serial => {
            if cfg.port.is_empty() {
                return Err(CoreError::InvalidRequest(format!(
                    "{} needs a serial port",
                    def.label
                )));
            }
            validate_port(&cfg.port)?;
            a.push("-P".into());
            a.push(cfg.port.clone().into());
            let baud = cfg.baud.or(def.default_baud);
            if let Some(b) = baud {
                if !(300..=2_000_000).contains(&b) {
                    return Err(CoreError::InvalidRequest(format!("invalid baud rate {b}")));
                }
                a.push("-b".into());
                a.push(b.to_string().into());
            }
        }
        Connection::Usb => {
            if !cfg.port.is_empty() && cfg.port != "usb" {
                let re = Regex::new(r"^usb(:[A-Za-z0-9:._-]{1,64})?$").expect("regex");
                if !re.is_match(&cfg.port) {
                    return Err(CoreError::InvalidRequest(format!(
                        "invalid USB port specifier '{}'",
                        cfg.port
                    )));
                }
                a.push("-P".into());
                a.push(cfg.port.clone().into());
            }
        }
    }
    if let Some(bc) = cfg.bitclock.as_deref().filter(|s| !s.is_empty()) {
        let re = Regex::new(r"^[0-9]{1,4}(\.[0-9]{1,3})?$").expect("regex");
        if !re.is_match(bc) {
            return Err(CoreError::InvalidRequest(format!(
                "invalid bit clock '{bc}' (microseconds, e.g. 10)"
            )));
        }
        a.push("-B".into());
        a.push(bc.into());
    }
    Ok(a)
}

/// Extracts the device signature from AVRDUDE 6.x (`0x1e950f`) or 7.x/8.x (`1E 95 0F`) output.
pub fn parse_signature(output: &str) -> Option<[u8; 3]> {
    let re = Regex::new(r"(?i)device signature\s*=\s*(?:0x)?([0-9a-f]{2})\s*([0-9a-f]{2})\s*([0-9a-f]{2})")
        .ok()?;
    let c = re.captures(output)?;
    let b = |i| u8::from_str_radix(&c[i], 16).ok();
    Some([b(1)?, b(2)?, b(3)?])
}

/// Explains common AVRDUDE failures in plain language.
pub fn diagnose_failure(output: &str, def: Option<&ProgrammerDef>) -> Vec<String> {
    let o = output.to_lowercase();
    let mut hints = vec![];
    let mut add = |cond: bool, msg: &str| {
        if cond && !hints.iter().any(|h: &String| h == msg) {
            hints.push(msg.to_string());
        }
    };
    add(
        o.contains("cannot find usb device")
            || o.contains("could not find usb device")
            || o.contains("did not find any usb device"),
        "The USB programmer was not found. Check the cable, and that the selected programmer type matches your hardware.",
    );
    add(
        o.contains("permission denied")
            || o.contains("insufficient permissions")
            || o.contains("libusb_open")
            || o.contains("access denied"),
        "Permission denied. On Linux install a udev rule for the programmer (or add your user to the dialout group for serial ports) and replug it. On Windows install a libusb/WinUSB driver.",
    );
    add(
        o.contains("ser_open")
            || o.contains("can't open device")
            || o.contains("cannot open port")
            || o.contains("no such file or directory"),
        "The serial port could not be opened. Check the port name and that no other program (e.g. the Serial Monitor) is using it.",
    );
    add(
        o.contains("initialization failed")
            || o.contains("target doesn't answer")
            || o.contains("program enable: target doesn't answer"),
        "The target did not answer. Check ISP wiring (MOSI, MISO, SCK, RESET, GND), target power, and that the target has a working clock. Try a slower bit clock (-B 32).",
    );
    add(
        o.contains("0x000000") || o.contains("00 00 00") || o.contains("0xffffff") || o.contains("ff ff ff"),
        "The signature read as all zeros/ones: usually wiring, missing target power, a clock source that is not oscillating, or ISP disabled by fuses.",
    );
    add(
        o.contains("expected signature") || o.contains("signature mismatch"),
        "The chip's signature does not match the selected MCU. Check the MCU selected in the project.",
    );
    add(
        o.contains("not in sync")
            || o.contains("not responding")
            || o.contains("stk500_recv")
            || o.contains("stk500_getsync"),
        "No response from the bootloader/programmer. Check the port and baud rate, press reset just before uploading, and make sure a bootloader is installed.",
    );
    add(
        o.contains("verification error") || o.contains("verification mismatch") || o.contains("mismatch at"),
        "Verification failed: the flash content differs from the file. Check power supply stability and try a slower bit clock.",
    );
    add(
        o.contains("cannot set sck period"),
        "The programmer could not set the requested clock. Many USBasp clones need updated firmware; the upload may still work.",
    );
    if hints.is_empty() {
        hints.push("AVRDUDE reported an error. See the full log for details.".into());
    }
    if def.is_some_and(|d| d.kind == ProgrammerKind::Bootloader) {
        hints.push("Bootloader uploads require a bootloader on the chip and an auto-reset circuit or a manual reset.".into());
    }
    hints
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifyResult {
    pub success: bool,
    pub signature: Option<String>,
    pub detected_device: Option<String>,
    pub matches_project: bool,
    pub message: String,
    pub hints: Vec<String>,
    pub output: ProcessOutput,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub success: bool,
    pub verified: bool,
    pub bytes_verified: Option<u64>,
    pub message: String,
    pub hints: Vec<String>,
    pub output: ProcessOutput,
}

pub struct Avrdude<'a> {
    pub tool: &'a AvrdudeTool,
    pub device: &'a DeviceDefinition,
    pub config: &'a ProgrammerConfig,
}

impl Avrdude<'_> {
    pub fn spec(&self, extra: Vec<OsString>, timeout: Duration) -> CoreResult<ProcessSpec> {
        let mut args = base_args(self.tool, self.device, self.config)?;
        args.extend(extra);
        Ok(ProcessSpec::new(&self.tool.path, args).timeout(timeout))
    }

    pub fn identify_spec(&self) -> CoreResult<ProcessSpec> {
        self.spec(vec!["-n".into()], Duration::from_secs(60))
    }

    pub fn upload_spec(&self, hex: &Path) -> CoreResult<ProcessSpec> {
        if !hex.is_file() {
            return Err(CoreError::InvalidRequest(format!(
                "firmware file not found: {}",
                hex.display()
            )));
        }
        let mut u = OsString::from("flash:w:");
        u.push(hex.as_os_str());
        u.push(":i");
        self.spec(vec!["-U".into(), u], Duration::from_secs(300))
    }

    pub fn identify(
        &self,
        registry: &DeviceRegistry,
        cancel: &CancelToken,
        on_line: &mut dyn FnMut(Stream, &str),
    ) -> CoreResult<IdentifyResult> {
        let out = run_process(&self.identify_spec()?, cancel, on_line)?;
        Ok(interpret_identify(registry, self.device, out))
    }

    pub fn upload(
        &self,
        hex: &Path,
        cancel: &CancelToken,
        on_line: &mut dyn FnMut(Stream, &str),
    ) -> CoreResult<UploadResult> {
        let out = run_process(&self.upload_spec(hex)?, cancel, on_line)?;
        Ok(interpret_upload(
            out,
            programmer_def(&self.config.programmer).ok(),
        ))
    }
}

pub fn interpret_identify(
    registry: &DeviceRegistry,
    device: &DeviceDefinition,
    out: ProcessOutput,
) -> IdentifyResult {
    let text = out.combined();
    let sig = parse_signature(&text);
    let detected = sig
        .and_then(|s| registry.find_by_signature(s))
        .map(|d| d.name.clone());
    let matches = sig == Some(device.signature);
    let invalid_sig = matches!(sig, Some([0, 0, 0]) | Some([0xFF, 0xFF, 0xFF]));
    let success = out.success() && matches;
    let message = if out.cancelled {
        "Cancelled".to_string()
    } else if out.timed_out {
        "AVRDUDE timed out".to_string()
    } else if success {
        format!("Detected {} (signature {})", device.name, device.signature_hex())
    } else if let Some(s) = sig.filter(|_| !invalid_sig) {
        format!(
            "Signature {:02X} {:02X} {:02X} {} — the project targets {} ({})",
            s[0],
            s[1],
            s[2],
            detected
                .as_deref()
                .map(|d| format!("is {d}"))
                .unwrap_or_else(|| "is not a known device".into()),
            device.name,
            device.signature_hex()
        )
    } else {
        "Could not read a valid device signature".to_string()
    };
    let hints = if success {
        vec![]
    } else {
        diagnose_failure(&text, None)
    };
    IdentifyResult {
        success,
        signature: sig.map(|s| format!("{:02X} {:02X} {:02X}", s[0], s[1], s[2])),
        detected_device: if invalid_sig { None } else { detected },
        matches_project: matches,
        message,
        hints,
        output: out,
    }
}

pub fn interpret_upload(out: ProcessOutput, def: Option<&ProgrammerDef>) -> UploadResult {
    let text = out.combined();
    let re = Regex::new(r"(?i)(\d+)\s+bytes of flash verified").expect("regex");
    let bytes_verified = re.captures(&text).and_then(|c| c[1].parse().ok());
    let lower = text.to_lowercase();
    let mismatch = lower.contains("verification error") || lower.contains("mismatch");
    let verified = bytes_verified.is_some() && !mismatch;
    let success = out.success() && verified;
    let message = if out.cancelled {
        "Upload cancelled. The flash content may be incomplete; upload again before relying on the device."
            .into()
    } else if out.timed_out {
        "AVRDUDE timed out. The flash content may be incomplete.".into()
    } else if success {
        format!(
            "Upload complete: {} bytes written and verified",
            bytes_verified.unwrap_or(0)
        )
    } else if out.exit_code == Some(0) {
        "AVRDUDE exited successfully but no verification result was reported; the upload is NOT confirmed."
            .into()
    } else {
        "Upload failed".into()
    };
    let hints = if success {
        vec![]
    } else {
        diagnose_failure(&text, def)
    };
    UploadResult {
        success,
        verified,
        bytes_verified,
        message,
        hints,
        output: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn out(code: Option<i32>, stderr: &str) -> ProcessOutput {
        ProcessOutput {
            command: "avrdude".into(),
            exit_code: code,
            stdout: String::new(),
            stderr: stderr.into(),
            timed_out: false,
            cancelled: false,
            duration_ms: 1,
        }
    }

    fn tool() -> AvrdudeTool {
        AvrdudeTool {
            path: PathBuf::from("/usr/bin/avrdude"),
            version: Some("7.1".into()),
            conf: None,
        }
    }

    fn cfg(p: &str, port: &str) -> ProgrammerConfig {
        ProgrammerConfig {
            programmer: p.into(),
            port: port.into(),
            baud: None,
            bitclock: None,
        }
    }

    #[test]
    fn signature_formats() {
        assert_eq!(
            parse_signature("avrdude: Device signature = 0x1e950f (probably m328p)"),
            Some([0x1E, 0x95, 0x0F])
        );
        assert_eq!(
            parse_signature("Device signature = 1E 95 02 (ATmega32A)"),
            Some([0x1E, 0x95, 0x02])
        );
        assert_eq!(parse_signature("nothing here"), None);
    }

    #[test]
    fn builds_safe_args() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega32a").unwrap();
        let a = base_args(&tool(), d, &cfg("usbasp", "usb")).unwrap();
        assert_eq!(
            a,
            vec!["-c", "usbasp", "-p", "m32a"]
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
        );
        let a = base_args(&tool(), d, &cfg("arduino", "/dev/ttyUSB0")).unwrap();
        assert!(a.contains(&OsString::from("-P")) && a.contains(&OsString::from("115200")));
        assert!(base_args(&tool(), d, &cfg("arduino", "/dev/ttyUSB0; rm -rf /")).is_err());
        assert!(base_args(&tool(), d, &cfg("arduino", "")).is_err());
        assert!(base_args(&tool(), d, &cfg("evil", "usb")).is_err());
        let mut c = cfg("usbasp", "usb");
        c.bitclock = Some("32 -U x".into());
        assert!(base_args(&tool(), d, &c).is_err());
        assert!(validate_port("COM3").is_ok());
        assert!(validate_port("/dev/../etc/passwd").is_err());
    }

    #[test]
    fn identify_requires_matching_signature() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega328p").unwrap();
        let r = interpret_identify(
            &reg,
            d,
            out(Some(0), "avrdude: Device signature = 0x1e950f (probably m328p)\n"),
        );
        assert!(r.success && r.matches_project);
        let r = interpret_identify(
            &reg,
            d,
            out(
                Some(1),
                "Device signature = 1E 95 02\nExpected signature for ATmega328P is 1E 95 0F\n",
            ),
        );
        assert!(!r.success);
        assert_eq!(r.detected_device.as_deref(), Some("ATmega32A"));
        assert!(r.hints.iter().any(|h| h.contains("does not match")));
        let r = interpret_identify(
            &reg,
            d,
            out(
                Some(1),
                "avrdude: initialization failed, rc=-1\nDevice signature = 0x000000\n",
            ),
        );
        assert!(!r.success && r.detected_device.is_none());
        assert!(r.hints.iter().any(|h| h.contains("wiring")));
    }

    #[test]
    fn upload_requires_verification_evidence() {
        let ok = interpret_upload(
            out(
                Some(0),
                "avrdude: 162 bytes of flash written\navrdude: 162 bytes of flash verified\n",
            ),
            None,
        );
        assert!(ok.success && ok.verified);
        assert_eq!(ok.bytes_verified, Some(162));
        let unverified = interpret_upload(out(Some(0), "avrdude: 162 bytes of flash written\n"), None);
        assert!(!unverified.success);
        assert!(unverified.message.contains("NOT confirmed"));
        let mismatch = interpret_upload(
            out(
                Some(1),
                "avrdude: verification error, first mismatch at byte 0x0000\n",
            ),
            None,
        );
        assert!(!mismatch.success && mismatch.hints.iter().any(|h| h.contains("Verification failed")));
        let usb = interpret_upload(
            out(
                Some(1),
                "avrdude error: cannot find USB device with vid=0x16c0 pid=0x5dc",
            ),
            programmer_def("usbasp").ok(),
        );
        assert!(usb.hints[0].contains("USB programmer was not found"));
    }
}
