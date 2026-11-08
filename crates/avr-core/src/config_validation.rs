//! Validation of MCU configuration: clock, pins, peripherals and the
//! agreement between desired fuses and the firmware's clock assumptions.

use crate::device_registry::{DeviceDefinition, FuseRisk};
use crate::diagnostics::{Diagnostic, sort_diagnostics};
use crate::paths::is_valid_identifier;
use crate::project_store::{PinMode, Project};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaudSetting {
    pub ubrr: u16,
    pub double_speed: bool,
    pub actual_baud: f64,
    pub error_percent: f64,
}

/// Picks the UBRR value (normal or double speed) with the smallest baud error.
pub fn compute_baud(f_cpu: u32, baud: u32) -> Option<BaudSetting> {
    if baud == 0 || f_cpu == 0 {
        return None;
    }
    let mut best: Option<BaudSetting> = None;
    for (double_speed, div) in [(false, 16u64), (true, 8u64)] {
        let ideal = f_cpu as f64 / (div as f64 * baud as f64) - 1.0;
        let ubrr = ideal.round();
        if !(0.0..=4095.0).contains(&ubrr) {
            continue;
        }
        let actual = f_cpu as f64 / (div as f64 * (ubrr + 1.0));
        let err = (actual / baud as f64 - 1.0) * 100.0;
        let cand = BaudSetting {
            ubrr: ubrr as u16,
            double_speed,
            actual_baud: actual,
            error_percent: err,
        };
        // Prefer normal speed on ties: it samples more robustly.
        if best.is_none_or(|b| err.abs() + 1e-9 < b.error_percent.abs()) {
            best = Some(cand);
        }
    }
    best
}

/// Pins claimed by enabled peripherals and the clock configuration, keyed by pin.
pub fn claimed_pins(device: &DeviceDefinition, project: &Project) -> BTreeMap<String, String> {
    let mut claims = BTreeMap::new();
    if project.peripherals.usart.enabled
        && let Some(u) = device.peripherals.usart.first()
    {
        claims.insert(u.rx.clone(), format!("{} RXD", u.id));
        claims.insert(u.tx.clone(), format!("{} TXD", u.id));
    }
    let crystal = matches!(project.clock.source.as_str(), "crystal" | "low_freq_crystal");
    if crystal {
        for p in &device.clock.crystal_pins {
            claims.insert(p.clone(), "crystal oscillator".into());
        }
    } else if project.clock.source == "external_clock" {
        for p in &device.clock.external_clock_pins {
            claims.insert(p.clone(), "external clock input".into());
        }
    }
    if let Some(r) = &device.reset_pin {
        claims.insert(r.pin.clone(), "RESET (RSTDISBL unprogrammed)".into());
    }
    claims
}

pub fn validate_config(device: &DeviceDefinition, project: &Project) -> Vec<Diagnostic> {
    let mut d = vec![];
    let package = &project.manifest.device.package;
    if device.package(package).is_none() {
        d.push(
            Diagnostic::error(
                "device.package",
                format!("{} has no package {package}", device.name),
            )
            .in_section("device"),
        );
    }

    // Clock
    let clock = &project.clock;
    match device.clock_source(&clock.source) {
        None => d.push(
            Diagnostic::error(
                "clock.source",
                format!(
                    "Clock source '{}' is not available on {}",
                    clock.source, device.name
                ),
            )
            .in_section("clock"),
        ),
        Some(src) => {
            let f = clock.source_frequency_hz;
            let ok = match (&src.frequencies_hz, src.min_hz, src.max_hz) {
                (Some(list), _, _) => list.contains(&f),
                (None, min, max) => min.is_none_or(|m| f >= m) && max.is_none_or(|m| f <= m),
            };
            if !ok {
                d.push(
                    Diagnostic::error(
                        "clock.frequency",
                        format!("{} Hz is not valid for {}", f, src.label),
                    )
                    .in_section("clock"),
                );
            }
        }
    }
    if clock.ckdiv8 && !device.clock.has_clock_prescaler_fuse {
        d.push(
            Diagnostic::error("clock.ckdiv8", format!("{} has no CKDIV8 fuse", device.name))
                .in_section("clock"),
        );
    }
    let f_cpu = clock.f_cpu();
    if f_cpu == 0 {
        d.push(
            Diagnostic::error("clock.zero", "CPU frequency must be greater than zero").in_section("clock"),
        );
    } else if f_cpu > device.clock.max_frequency_hz {
        d.push(
            Diagnostic::error(
                "clock.max",
                format!(
                    "F_CPU {} Hz exceeds the {} maximum of {} Hz",
                    f_cpu, device.name, device.clock.max_frequency_hz
                ),
            )
            .in_section("clock"),
        );
    }

    // Pins
    let claims = claimed_pins(device, project);
    let mut seen_pins = BTreeMap::new();
    let mut seen_labels = BTreeMap::new();
    for a in &project.pins.assignments {
        let pd = Diagnostic::error("pins.x", "").at_pin(&a.pin).in_section("pins");
        let err = |code: &str, msg: String| Diagnostic {
            code: code.into(),
            message: msg,
            ..pd.clone()
        };
        if device.pin(&a.pin).is_none() {
            d.push(err(
                "pins.unknown",
                format!("{} is not a GPIO pin on {}", a.pin, device.name),
            ));
            continue;
        }
        if !device.pin_in_package(package, &a.pin) {
            d.push(err(
                "pins.package",
                format!("{} is not available in package {package}", a.pin),
            ));
        }
        if seen_pins.insert(a.pin.clone(), ()).is_some() {
            d.push(err(
                "pins.duplicate",
                format!("{} is assigned more than once", a.pin),
            ));
        }
        if !is_valid_identifier(&a.label) {
            d.push(err(
                "pins.label",
                format!("Label '{}' of {} must be a valid C identifier", a.label, a.pin),
            ));
        } else if let Some(other) = seen_labels.insert(a.label.clone(), a.pin.clone()) {
            d.push(err(
                "pins.label_duplicate",
                format!("Label '{}' is used by {} and {}", a.label, other, a.pin),
            ));
        }
        if let Some(by) = claims.get(&a.pin) {
            d.push(err(
                "pins.conflict",
                format!("{} is used by {by} and cannot also be a GPIO pin", a.pin),
            ));
        }
        if let Some(j) = &device.jtag_pins
            && j.pins.contains(&a.pin)
            && !project.peripherals.disable_jtag_at_startup
        {
            let jtag_programmed = project
                .fuses
                .desired
                .get("hfuse")
                .and_then(|b| device.fuse_field(&j.fuse_field).map(|(_, f)| f.extract(*b) == 0))
                .unwrap_or(true);
            if jtag_programmed {
                d.push(
                    Diagnostic::warning(
                        "pins.jtag",
                        format!(
                            "{} is a JTAG pin and JTAGEN is programmed. Enable 'Disable JTAG at startup' in Peripherals or unprogram JTAGEN, otherwise the pin will not work as GPIO.",
                            a.pin
                        ),
                    )
                    .at_pin(&a.pin)
                    .in_section("pins"),
                );
            }
        }
    }

    // USART
    let u = &project.peripherals.usart;
    if u.enabled {
        if device.peripherals.usart.is_empty() {
            d.push(
                Diagnostic::error("usart.missing", format!("{} has no USART", device.name))
                    .in_section("peripherals"),
            );
        }
        if !(5..=8).contains(&u.data_bits) {
            d.push(Diagnostic::error("usart.data_bits", "Data bits must be 5–8").in_section("peripherals"));
        }
        if !(1..=2).contains(&u.stop_bits) {
            d.push(
                Diagnostic::error("usart.stop_bits", "Stop bits must be 1 or 2").in_section("peripherals"),
            );
        }
        match compute_baud(f_cpu, u.baud) {
            None => d.push(
                Diagnostic::error("usart.baud", format!("{} baud cannot be generated from {} Hz", u.baud, f_cpu))
                    .in_section("peripherals"),
            ),
            Some(b) if b.error_percent.abs() > 4.5 => d.push(
                Diagnostic::error(
                    "usart.baud_error",
                    format!("Baud error {:+.1}% at {} baud with F_CPU {} Hz is too large for reliable communication", b.error_percent, u.baud, f_cpu),
                )
                .in_section("peripherals"),
            ),
            Some(b) if b.error_percent.abs() > 2.0 => d.push(
                Diagnostic::warning(
                    "usart.baud_error",
                    format!("Baud error {:+.1}% at {} baud with F_CPU {} Hz; communication may be unreliable", b.error_percent, u.baud, f_cpu),
                )
                .in_section("peripherals"),
            ),
            Some(_) => {}
        }
    }

    // ADC
    let a = &project.peripherals.adc;
    if a.enabled {
        let adc = &device.peripherals.adc;
        if !adc.references.iter().any(|r| r.id == a.reference) {
            d.push(
                Diagnostic::error(
                    "adc.reference",
                    format!("Unknown ADC reference '{}'", a.reference),
                )
                .in_section("peripherals"),
            );
        }
        if !adc.prescalers.contains(&a.prescaler) {
            d.push(
                Diagnostic::error(
                    "adc.prescaler",
                    format!("ADC prescaler {} is not supported", a.prescaler),
                )
                .in_section("peripherals"),
            );
        } else if f_cpu > 0 {
            let adc_clk = f_cpu / a.prescaler;
            if adc_clk > adc.max_clock_hz || adc_clk < 50_000 {
                d.push(
                    Diagnostic::warning(
                        "adc.clock",
                        format!(
                            "ADC clock {} Hz is outside 50–200 kHz; full 10-bit accuracy is not guaranteed",
                            adc_clk
                        ),
                    )
                    .in_section("peripherals"),
                );
            }
        }
    }
    if project.peripherals.disable_jtag_at_startup && device.jtag_pins.is_none() {
        d.push(
            Diagnostic::error("jtag.none", format!("{} has no JTAG interface", device.name))
                .in_section("peripherals"),
        );
    }

    d.extend(validate_fuse_agreement(device, project));
    sort_diagnostics(&mut d);
    d
}

/// Compares the desired fuse configuration with the clock configuration.
pub fn validate_fuse_agreement(device: &DeviceDefinition, project: &Project) -> Vec<Diagnostic> {
    let mut d = vec![];
    for b in &device.fuses.bytes {
        let Some(value) = project.fuses.desired.get(&b.id) else {
            d.push(
                Diagnostic::error(
                    "fuses.missing",
                    format!("Desired value for {} is missing", b.label),
                )
                .in_section("fuses"),
            );
            continue;
        };
        for f in &b.fields {
            let raw = f.extract(*value);
            match f.option(raw) {
                None => d.push(
                    Diagnostic::error(
                        "fuses.invalid",
                        format!("{} has no defined meaning for value {raw:#b}", f.label),
                    )
                    .in_section("fuses"),
                ),
                Some(o) if o.risk == Some(FuseRisk::Blocked) => d.push(
                    Diagnostic::error(
                        "fuses.blocked",
                        format!(
                            "{} = '{}' is not allowed: {}",
                            f.id,
                            o.label,
                            o.warning.clone().unwrap_or_default()
                        ),
                    )
                    .in_section("fuses"),
                ),
                _ => {}
            }
        }
    }
    if let (Some((lb, cksel)), Some(low)) = (
        device.fuse_field("CKSEL"),
        device
            .fuse_field("CKSEL")
            .and_then(|(b, _)| project.fuses.desired.get(&b.id)),
    ) {
        let _ = lb;
        if let Some(opt) = cksel.option(cksel.extract(*low)) {
            if let Some(src) = &opt.clock_source
                && src != &project.clock.source
            {
                d.push(
                    Diagnostic::warning(
                        "fuses.clock_mismatch",
                        format!(
                            "Desired CKSEL selects '{}' but the clock configuration assumes '{}'. The chip runs from what the fuses select; F_CPU will be wrong unless they agree.",
                            opt.label, project.clock.source
                        ),
                    )
                    .in_section("clock"),
                );
            } else if let Some(fh) = opt.frequency_hz
                && fh != project.clock.source_frequency_hz
            {
                d.push(
                    Diagnostic::warning(
                        "fuses.clock_mismatch",
                        format!(
                            "Desired CKSEL selects {} Hz but the clock configuration assumes {} Hz",
                            fh, project.clock.source_frequency_hz
                        ),
                    )
                    .in_section("clock"),
                );
            }
        }
    }
    if let Some((b, f)) = device.fuse_field("CKDIV8")
        && let Some(v) = project.fuses.desired.get(&b.id)
    {
        let programmed = f.extract(*v) == 0;
        if programmed != project.clock.ckdiv8 {
            d.push(
                Diagnostic::warning(
                    "fuses.ckdiv8_mismatch",
                    format!(
                        "Desired CKDIV8 is {} but the clock configuration {} the divide-by-8",
                        if programmed { "programmed" } else { "unprogrammed" },
                        if project.clock.ckdiv8 {
                            "assumes"
                        } else {
                            "does not assume"
                        }
                    ),
                )
                .in_section("clock"),
            );
        }
    }
    if let Some((b, f)) = device.fuse_field("CKOPT")
        && let Some(v) = project.fuses.desired.get(&b.id)
        && project.clock.source == "crystal"
        && project.clock.source_frequency_hz > 8_000_000
        && f.extract(*v) == 1
    {
        d.push(
            Diagnostic::warning(
                "fuses.ckopt",
                "Crystals above 8 MHz require CKOPT to be programmed on this device",
            )
            .in_section("clock"),
        );
    }
    d
}

pub fn pin_mode_label(m: PinMode) -> &'static str {
    match m {
        PinMode::Output => "output",
        PinMode::Input => "input",
        PinMode::InputPullup => "input with pull-up",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baud_16mhz_9600() {
        let b = compute_baud(16_000_000, 9600).unwrap();
        assert_eq!(b.ubrr, 103);
        assert!(!b.double_speed);
        assert!(b.error_percent.abs() < 0.2);
    }

    #[test]
    fn baud_16mhz_115200_prefers_double_speed() {
        let b = compute_baud(16_000_000, 115_200).unwrap();
        assert!(b.double_speed);
        assert_eq!(b.ubrr, 16);
        assert!((b.error_percent - 2.12).abs() < 0.05);
    }

    #[test]
    fn baud_1mhz_9600_uses_u2x() {
        let b = compute_baud(1_000_000, 9600).unwrap();
        assert!(b.double_speed);
        assert_eq!(b.ubrr, 12);
        assert!(b.error_percent.abs() < 0.2);
    }
}
