//! Device-definition registry.
//!
//! Device definitions live in `devices/<id>/device.json` and are embedded at
//! compile time. Each definition is self-contained: nothing is inherited
//! between devices, because register names, fuse meanings, vectors and pin
//! assignments differ between AVR parts.

use crate::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const DEVICE_SCHEMA_VERSION: u32 = 1;

const BUILTIN_DEVICES: &[(&str, &str)] = &[
    (
        "atmega328p",
        include_str!("../../../devices/atmega328p/device.json"),
    ),
    (
        "atmega32a",
        include_str!("../../../devices/atmega32a/device.json"),
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceDefinition {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub family: String,
    pub architecture: String,
    pub register_profile: RegisterProfile,
    pub provenance: Provenance,
    pub compiler: CompilerInfo,
    pub avrdude: AvrdudeInfo,
    pub signature: [u8; 3],
    pub memory: MemoryInfo,
    pub clock: ClockInfo,
    pub reset_pin: Option<ResetPin>,
    pub jtag_pins: Option<JtagPins>,
    pub ports: BTreeMap<String, Vec<u8>>,
    pub pins: Vec<PinDef>,
    pub packages: Vec<PackageDef>,
    pub peripherals: Peripherals,
    pub interrupt_vectors: Vec<InterruptVector>,
    pub fuses: FuseDefinitions,
}

/// Register-level programming model used by the code generator. Devices with
/// the same profile share register *names* for the features we generate; the
/// profile is declared explicitly per device rather than inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegisterProfile {
    /// ATmega48/88/168/328 family: USART0 registers (UCSR0A…), EICRA/EIMSK, PINx toggle.
    MegaX8,
    /// ATmega16/32 family: USART without index (UCSRA…, URSEL), MCUCR/GICR, no PINx toggle.
    MegaLegacy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provenance {
    pub datasheet: String,
    pub header: String,
    pub revision: String,
    pub verification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilerInfo {
    pub mmcu: String,
    pub header: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AvrdudeInfo {
    pub part_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryInfo {
    pub flash_bytes: u32,
    pub flash_page_bytes: u32,
    pub sram_bytes: u32,
    pub sram_start: u32,
    pub eeprom_bytes: u32,
    pub eeprom_page_bytes: u32,
    pub register_file_bytes: u32,
    pub io_bytes: u32,
    pub extended_io_bytes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClockInfo {
    pub max_frequency_hz: u32,
    pub has_clock_prescaler_fuse: bool,
    pub sources: Vec<ClockSourceDef>,
    pub crystal_pins: Vec<String>,
    pub external_clock_pins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClockSourceDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub frequencies_hz: Option<Vec<u32>>,
    #[serde(default)]
    pub min_hz: Option<u32>,
    #[serde(default)]
    pub max_hz: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResetPin {
    pub pin: String,
    pub disable_fuse_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JtagPins {
    pub pins: Vec<String>,
    pub fuse_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinDef {
    pub name: String,
    pub port: String,
    pub bit: u8,
    pub functions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageDef {
    pub id: String,
    pub label: String,
    pub layout: String,
    /// Signal on each physical pin, index 0 = physical pin 1.
    pub pins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Peripherals {
    pub usart: Vec<UsartDef>,
    pub spi: Vec<SpiDef>,
    pub twi: Vec<TwiDef>,
    pub adc: AdcDef,
    pub timers: Vec<TimerDef>,
    pub external_interrupts: Vec<ExtIntDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsartDef {
    pub id: String,
    pub rx: String,
    pub tx: String,
    pub xck: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpiDef {
    pub id: String,
    pub ss: String,
    pub mosi: String,
    pub miso: String,
    pub sck: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TwiDef {
    pub id: String,
    pub sda: String,
    pub scl: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdcDef {
    pub resolution_bits: u8,
    pub channels: Vec<AdcChannelDef>,
    pub references: Vec<AdcReferenceDef>,
    pub prescalers: Vec<u32>,
    pub max_clock_hz: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdcChannelDef {
    pub channel: u8,
    pub pin: String,
    /// When present, the channel only exists on these packages.
    #[serde(default)]
    pub packages: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdcReferenceDef {
    pub id: String,
    pub label: String,
    pub refs_bits: u8,
    #[serde(default)]
    pub volts: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimerDef {
    pub id: String,
    pub bits: u8,
    pub outputs: Vec<TimerOutputDef>,
    pub prescalers: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimerOutputDef {
    pub id: String,
    pub pin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtIntDef {
    pub id: String,
    pub pin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InterruptVector {
    pub number: u8,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FuseDefinitions {
    pub notes: String,
    pub bytes: Vec<FuseByteDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FuseByteDef {
    /// AVRDUDE memory name (`lfuse`, `hfuse`, `efuse`).
    pub id: String,
    pub label: String,
    pub default_value: u8,
    /// Bits that carry meaning; other bits are unused and read back as 1 (or
    /// as 0 on some AVRDUDE versions), so comparisons must mask them.
    pub used_mask: u8,
    pub fields: Vec<FuseFieldDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FuseFieldDef {
    pub id: String,
    pub mask: u8,
    pub label: String,
    pub default_value: u8,
    pub datasheet_ref: String,
    pub description: String,
    pub options: Vec<FuseOptionDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FuseRisk {
    Caution,
    Danger,
    /// The application refuses to write this value.
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FuseOptionDef {
    pub value: u8,
    pub label: String,
    #[serde(default)]
    pub risk: Option<FuseRisk>,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(default)]
    pub clock_source: Option<String>,
    #[serde(default)]
    pub frequency_hz: Option<u32>,
}

impl FuseFieldDef {
    pub fn shift(&self) -> u32 {
        self.mask.trailing_zeros()
    }
    /// Extracts the raw field value from a full fuse byte.
    pub fn extract(&self, byte: u8) -> u8 {
        (byte & self.mask) >> self.shift()
    }
    pub fn insert(&self, byte: u8, value: u8) -> u8 {
        (byte & !self.mask) | ((value << self.shift()) & self.mask)
    }
    pub fn option(&self, value: u8) -> Option<&FuseOptionDef> {
        self.options.iter().find(|o| o.value == value)
    }
}

/// Signals on package pins that are not port pins.
pub const NON_PORT_SIGNALS: &[&str] = &[
    "VCC", "GND", "AVCC", "AREF", "RESET", "XTAL1", "XTAL2", "ADC6", "ADC7",
];

impl DeviceDefinition {
    pub fn pin(&self, name: &str) -> Option<&PinDef> {
        self.pins.iter().find(|p| p.name == name)
    }
    pub fn package(&self, id: &str) -> Option<&PackageDef> {
        self.packages.iter().find(|p| p.id == id)
    }
    /// Physical pin number of a signal in a package (first occurrence).
    pub fn physical_pin(&self, package: &str, signal: &str) -> Option<u32> {
        self.package(package)?
            .pins
            .iter()
            .position(|s| s == signal)
            .map(|i| i as u32 + 1)
    }
    pub fn pin_in_package(&self, package: &str, pin: &str) -> bool {
        self.physical_pin(package, pin).is_some()
    }
    pub fn clock_source(&self, id: &str) -> Option<&ClockSourceDef> {
        self.clock.sources.iter().find(|s| s.id == id)
    }
    pub fn fuse_byte(&self, id: &str) -> Option<&FuseByteDef> {
        self.fuses.bytes.iter().find(|b| b.id == id)
    }
    pub fn fuse_field(&self, field_id: &str) -> Option<(&FuseByteDef, &FuseFieldDef)> {
        self.fuses
            .bytes
            .iter()
            .find_map(|b| b.fields.iter().find(|f| f.id == field_id).map(|f| (b, f)))
    }
    pub fn default_fuses(&self) -> BTreeMap<String, u8> {
        self.fuses
            .bytes
            .iter()
            .map(|b| (b.id.clone(), b.default_value))
            .collect()
    }
    pub fn adc_channel(&self, package: &str, channel: u8) -> Option<&AdcChannelDef> {
        self.peripherals.adc.channels.iter().find(|c| {
            c.channel == channel && c.packages.as_ref().is_none_or(|p| p.iter().any(|x| x == package))
        })
    }
    pub fn signature_hex(&self) -> String {
        format!(
            "{:02X} {:02X} {:02X}",
            self.signature[0], self.signature[1], self.signature[2]
        )
    }

    /// Internal consistency checks. Returns a list of problems (empty = OK).
    pub fn self_check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.schema_version != DEVICE_SCHEMA_VERSION {
            problems.push(format!("unsupported schemaVersion {}", self.schema_version));
        }
        let pin_names: BTreeSet<&str> = self.pins.iter().map(|p| p.name.as_str()).collect();
        if pin_names.len() != self.pins.len() {
            problems.push("duplicate pin names".into());
        }
        for p in &self.pins {
            if p.name != format!("P{}{}", p.port, p.bit) {
                problems.push(format!("pin {} does not match port/bit", p.name));
            }
            if !self.ports.get(&p.port).is_some_and(|bits| bits.contains(&p.bit)) {
                problems.push(format!("pin {} not listed in ports", p.name));
            }
        }
        let port_pin_count: usize = self.ports.values().map(|v| v.len()).sum();
        if port_pin_count != self.pins.len() {
            problems.push("ports and pins lists disagree".into());
        }
        for pkg in &self.packages {
            for (i, s) in pkg.pins.iter().enumerate() {
                if !pin_names.contains(s.as_str()) && !NON_PORT_SIGNALS.contains(&s.as_str()) {
                    problems.push(format!(
                        "package {} pin {} has unknown signal {}",
                        pkg.id,
                        i + 1,
                        s
                    ));
                }
            }
            for p in &self.pins {
                if !pkg.pins.contains(&p.name) {
                    problems.push(format!("package {} is missing port pin {}", pkg.id, p.name));
                }
            }
        }
        let mut check_pin = |what: &str, pin: &str| {
            if !pin_names.contains(pin) && !NON_PORT_SIGNALS.contains(&pin) {
                problems.push(format!("{what} references unknown pin {pin}"));
            }
        };
        for u in &self.peripherals.usart {
            for p in [&u.rx, &u.tx, &u.xck] {
                check_pin(&u.id, p);
            }
        }
        for s in &self.peripherals.spi {
            for p in [&s.ss, &s.mosi, &s.miso, &s.sck] {
                check_pin(&s.id, p);
            }
        }
        for t in &self.peripherals.twi {
            check_pin(&t.id, &t.sda);
            check_pin(&t.id, &t.scl);
        }
        for c in &self.peripherals.adc.channels {
            check_pin("ADC", &c.pin);
        }
        for t in &self.peripherals.timers {
            for o in &t.outputs {
                check_pin(&t.id, &o.pin);
            }
        }
        for e in &self.peripherals.external_interrupts {
            check_pin(&e.id, &e.pin);
        }
        for p in self
            .clock
            .crystal_pins
            .iter()
            .chain(&self.clock.external_clock_pins)
        {
            check_pin("clock", p);
        }
        for b in &self.fuses.bytes {
            let mut union = 0u8;
            for f in &b.fields {
                if union & f.mask != 0 {
                    problems.push(format!("fuse {} field {} overlaps another field", b.id, f.id));
                }
                union |= f.mask;
                if f.extract(b.default_value) != f.default_value {
                    problems.push(format!(
                        "fuse {} field {} default {} disagrees with byte default 0x{:02X}",
                        b.id, f.id, f.default_value, b.default_value
                    ));
                }
                let max = f.mask >> f.shift();
                for o in &f.options {
                    if o.value > max {
                        problems.push(format!("fuse field {} option {} out of range", f.id, o.value));
                    }
                }
                if f.option(f.default_value).is_none() {
                    problems.push(format!("fuse field {} default has no option", f.id));
                }
            }
            if union != b.used_mask {
                problems.push(format!("fuse {} usedMask disagrees with fields", b.id));
            }
        }
        let mut last = 0;
        for v in &self.interrupt_vectors {
            if v.number <= last {
                problems.push(format!("interrupt vector {} out of order", v.name));
            }
            last = v.number;
        }
        if let Some(r) = &self.reset_pin
            && self.fuse_field(&r.disable_fuse_field).is_none()
        {
            problems.push("resetPin fuse field missing".into());
        }
        if let Some(j) = &self.jtag_pins
            && self.fuse_field(&j.fuse_field).is_none()
        {
            problems.push("jtagPins fuse field missing".into());
        }
        problems
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub id: String,
    pub name: String,
    pub packages: Vec<String>,
    pub flash_bytes: u32,
    pub sram_bytes: u32,
    pub eeprom_bytes: u32,
    pub max_frequency_hz: u32,
}

pub struct DeviceRegistry {
    devices: BTreeMap<String, DeviceDefinition>,
}

impl DeviceRegistry {
    /// Loads the definitions embedded in the binary. Fails loudly if any
    /// definition is malformed, since silently skipping a device would hide
    /// data errors.
    pub fn builtin() -> CoreResult<Self> {
        let mut devices = BTreeMap::new();
        for (id, json) in BUILTIN_DEVICES {
            let def: DeviceDefinition = serde_json::from_str(json)
                .map_err(|e| CoreError::InvalidRequest(format!("device definition {id}: {e}")))?;
            if def.id != *id {
                return Err(CoreError::InvalidRequest(format!(
                    "device definition {id} has id {}",
                    def.id
                )));
            }
            let problems = def.self_check();
            if !problems.is_empty() {
                return Err(CoreError::InvalidRequest(format!(
                    "device definition {id} failed self-check: {}",
                    problems.join("; ")
                )));
            }
            devices.insert(def.id.clone(), def);
        }
        Ok(DeviceRegistry { devices })
    }

    pub fn get(&self, id: &str) -> CoreResult<&DeviceDefinition> {
        self.devices
            .get(id)
            .ok_or_else(|| CoreError::UnknownDevice(id.to_string()))
    }

    pub fn all(&self) -> impl Iterator<Item = &DeviceDefinition> {
        self.devices.values()
    }

    pub fn summaries(&self) -> Vec<DeviceSummary> {
        self.devices
            .values()
            .map(|d| DeviceSummary {
                id: d.id.clone(),
                name: d.name.clone(),
                packages: d.packages.iter().map(|p| p.id.clone()).collect(),
                flash_bytes: d.memory.flash_bytes,
                sram_bytes: d.memory.sram_bytes,
                eeprom_bytes: d.memory.eeprom_bytes,
                max_frequency_hz: d.clock.max_frequency_hz,
            })
            .collect()
    }

    pub fn find_by_signature(&self, sig: [u8; 3]) -> Option<&DeviceDefinition> {
        self.devices.values().find(|d| d.signature == sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_loads_and_self_checks() {
        let reg = DeviceRegistry::builtin().expect("registry");
        assert_eq!(reg.all().count(), 2);
    }

    #[test]
    fn atmega328p_core_facts() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega328p").unwrap();
        assert_eq!(d.signature, [0x1E, 0x95, 0x0F]);
        assert_eq!(d.memory.flash_bytes, 32 * 1024);
        assert_eq!(d.memory.sram_bytes, 2048);
        assert_eq!(d.memory.eeprom_bytes, 1024);
        assert_eq!(d.physical_pin("PDIP-28", "PB5"), Some(19));
        assert_eq!(d.physical_pin("PDIP-28", "PC6"), Some(1));
        assert_eq!(d.physical_pin("TQFP-32", "PB5"), Some(17));
        assert_eq!(d.physical_pin("TQFP-32", "ADC6"), Some(19));
        assert!(d.adc_channel("PDIP-28", 6).is_none());
        assert!(d.adc_channel("TQFP-32", 6).is_some());
        assert_eq!(d.default_fuses().get("lfuse"), Some(&0x62));
        assert_eq!(d.default_fuses().get("hfuse"), Some(&0xD9));
        assert_eq!(d.default_fuses().get("efuse"), Some(&0xFF));
    }

    #[test]
    fn atmega32a_core_facts() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega32a").unwrap();
        assert_eq!(d.signature, [0x1E, 0x95, 0x02]);
        assert_eq!(d.memory.sram_start, 0x60);
        assert_eq!(d.physical_pin("PDIP-40", "PB0"), Some(1));
        assert_eq!(d.physical_pin("PDIP-40", "PA0"), Some(40));
        assert_eq!(d.physical_pin("TQFP-44", "PB5"), Some(1));
        assert!(d.fuse_byte("efuse").is_none(), "ATmega32A has no extended fuse");
        assert_eq!(d.default_fuses().get("lfuse"), Some(&0xE1));
        assert_eq!(d.default_fuses().get("hfuse"), Some(&0x99));
        let (_, jtag) = d.fuse_field("JTAGEN").unwrap();
        assert_eq!(jtag.mask, 0x40);
    }

    #[test]
    fn fuse_field_extract_insert_roundtrip() {
        let reg = DeviceRegistry::builtin().unwrap();
        let d = reg.get("atmega328p").unwrap();
        let (_, cksel) = d.fuse_field("CKSEL").unwrap();
        assert_eq!(cksel.extract(0x62), 0b0010);
        assert_eq!(cksel.insert(0x62, 0b1111), 0x6F);
        let (_, sut) = d.fuse_field("SUT").unwrap();
        assert_eq!(sut.extract(0x62), 0b10);
    }

    #[test]
    fn signature_lookup() {
        let reg = DeviceRegistry::builtin().unwrap();
        assert_eq!(reg.find_by_signature([0x1E, 0x95, 0x02]).unwrap().id, "atmega32a");
        assert!(reg.find_by_signature([0, 0, 0]).is_none());
    }
}
