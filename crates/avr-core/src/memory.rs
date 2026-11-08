//! Build-artifact-based memory analysis. Figures come from the linked ELF
//! file (static allocation only); runtime stack and heap usage are not known.

use crate::device_registry::DeviceDefinition;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub name: String,
    pub size: u64,
    pub address: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRegionUsage {
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReport {
    pub flash: MemoryRegionUsage,
    pub sram_static: MemoryRegionUsage,
    pub eeprom: MemoryRegionUsage,
    pub sections: Vec<Section>,
    pub symbols: Vec<Symbol>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub kind: char,
    pub region: &'static str,
}

/// Parses `avr-size -A <elf>` (SysV format, supported by all binutils versions).
pub fn parse_size_sysv(output: &str) -> Vec<Section> {
    output
        .lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let name = parts.next()?;
            if !name.starts_with('.') {
                return None;
            }
            let size = parts.next()?.parse().ok()?;
            let address = parts.next()?.parse().ok()?;
            Some(Section {
                name: name.to_string(),
                size,
                address,
            })
        })
        .collect()
}

pub fn analyze(device: &DeviceDefinition, sections: Vec<Section>, symbols: Vec<Symbol>) -> MemoryReport {
    let sum = |names: &[&str]| -> u64 {
        sections
            .iter()
            .filter(|s| names.contains(&s.name.as_str()))
            .map(|s| s.size)
            .sum()
    };
    let flash = sum(&[".text", ".data", ".bootloader"]);
    let sram = sum(&[".data", ".bss", ".noinit"]);
    let eeprom = sum(&[".eeprom"]);
    MemoryReport {
        flash: MemoryRegionUsage {
            used: flash,
            total: device.memory.flash_bytes as u64,
        },
        sram_static: MemoryRegionUsage {
            used: sram,
            total: device.memory.sram_bytes as u64,
        },
        eeprom: MemoryRegionUsage {
            used: eeprom,
            total: device.memory.eeprom_bytes as u64,
        },
        sections,
        symbols,
        notes: vec![
            "Flash = .text + .data (+ .bootloader). SRAM = .data + .bss + .noinit (static allocation only)."
                .into(),
            "Stack and heap usage at runtime are NOT included; leave SRAM headroom for the stack.".into(),
            "If a bootloader is installed, it occupies part of the flash shown as available.".into(),
        ],
    }
}

/// Parses `avr-nm -S --size-sort <elf>` output.
pub fn parse_nm(output: &str) -> Vec<Symbol> {
    output
        .lines()
        .filter_map(|l| {
            let parts: Vec<_> = l.split_whitespace().collect();
            if parts.len() != 4 {
                return None;
            }
            let address = u64::from_str_radix(parts[0], 16).ok()?;
            let size = u64::from_str_radix(parts[1], 16).ok()?;
            let kind = parts[2].chars().next()?;
            let (region, addr) = if address >= 0x81_0000 {
                ("eeprom", address - 0x81_0000)
            } else if address >= 0x80_0000 {
                ("sram", address - 0x80_0000)
            } else {
                ("flash", address)
            };
            Some(Symbol {
                name: parts[3].to_string(),
                address: addr,
                size,
                kind,
                region,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_registry::DeviceRegistry;

    const SIZE_OUT: &str = "firmware.elf  :\nsection                    size      addr\n.data                         2   8388864\n.text                       410         0\n.bss                          3   8388866\n.comment                     17         0\n.note.gnu.avr.deviceinfo     64         0\nTotal                       496\n\n\n";

    #[test]
    fn parses_size_and_computes_usage() {
        let reg = DeviceRegistry::builtin().unwrap();
        let s = parse_size_sysv(SIZE_OUT);
        assert_eq!(s.len(), 5);
        let r = analyze(reg.get("atmega328p").unwrap(), s, vec![]);
        assert_eq!(r.flash.used, 412);
        assert_eq!(r.sram_static.used, 5);
        assert_eq!(r.eeprom.used, 0);
        assert_eq!(r.flash.total, 32768);
    }

    #[test]
    fn parses_nm() {
        let syms = parse_nm("00800100 00000002 B v_count\n00000080 0000001a T main\nbad line\n");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].region, "sram");
        assert_eq!(syms[0].address, 0x100);
        assert_eq!(syms[1].region, "flash");
    }
}
