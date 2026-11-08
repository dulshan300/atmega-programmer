//! Native serial monitor backend and serial-port ownership coordination.
//!
//! Every subsystem that opens a serial device (Serial Monitor, AVRDUDE with a
//! serial programmer/bootloader) must hold a lease from the `PortRegistry`,
//! so the same port is never silently opened twice.

use crate::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    pub name: String,
    pub kind: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
    /// Subsystem currently holding the port inside this application, if any.
    pub in_use_by: Option<String>,
}

pub fn list_ports(registry: &PortRegistry) -> CoreResult<Vec<PortInfo>> {
    let ports = serialport::available_ports().map_err(|e| CoreError::Serial(e.to_string()))?;
    let mut out: Vec<PortInfo> = ports
        .into_iter()
        .map(|p| {
            let in_use_by = registry.owner(&p.port_name);
            match p.port_type {
                serialport::SerialPortType::UsbPort(u) => PortInfo {
                    name: p.port_name,
                    kind: "usb".into(),
                    vid: Some(u.vid),
                    pid: Some(u.pid),
                    manufacturer: u.manufacturer,
                    product: u.product,
                    serial_number: u.serial_number,
                    in_use_by,
                },
                other => PortInfo {
                    name: p.port_name,
                    kind: match other {
                        serialport::SerialPortType::PciPort => "pci",
                        serialport::SerialPortType::BluetoothPort => "bluetooth",
                        _ => "unknown",
                    }
                    .into(),
                    vid: None,
                    pid: None,
                    manufacturer: None,
                    product: None,
                    serial_number: None,
                    in_use_by,
                },
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[derive(Clone, Default)]
pub struct PortRegistry(Arc<Mutex<BTreeMap<String, String>>>);

impl PortRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn owner(&self, port: &str) -> Option<String> {
        self.0.lock().ok().and_then(|m| m.get(port).cloned())
    }
    pub fn acquire(&self, port: &str, owner: &str) -> CoreResult<PortLease> {
        let mut m = self
            .0
            .lock()
            .map_err(|_| CoreError::Busy("port registry poisoned".into()))?;
        if let Some(cur) = m.get(port) {
            return Err(CoreError::Busy(format!(
                "{port} is in use by {cur}. Disconnect it there first."
            )));
        }
        m.insert(port.to_string(), owner.to_string());
        Ok(PortLease {
            registry: self.clone(),
            port: port.to_string(),
        })
    }
}

/// Releases the port when dropped.
pub struct PortLease {
    registry: PortRegistry,
    port: String,
}

impl Drop for PortLease {
    fn drop(&mut self) {
        if let Ok(mut m) = self.registry.0.lock() {
            m.remove(&self.port);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowControl {
    None,
    Software,
    Hardware,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialConfig {
    pub port: String,
    pub baud: u32,
    pub data_bits: u8,
    pub parity: SerialParity,
    pub stop_bits: u8,
    pub flow_control: FlowControl,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum SerialStatus {
    Connected { port: String },
    Disconnected { port: String, reason: Option<String> },
}

pub trait SerialSink: Send + Sync + 'static {
    fn data(&self, bytes: &[u8], timestamp_ms: u64);
    fn status(&self, status: SerialStatus);
}

pub struct SerialSession {
    port_name: String,
    writer: Mutex<Box<dyn serialport::SerialPort>>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl SerialSession {
    pub fn open(cfg: &SerialConfig, registry: &PortRegistry, sink: Arc<dyn SerialSink>) -> CoreResult<Self> {
        if !(50..=4_000_000).contains(&cfg.baud) {
            return Err(CoreError::InvalidRequest(format!(
                "invalid baud rate {}",
                cfg.baud
            )));
        }
        let data_bits = match cfg.data_bits {
            5 => serialport::DataBits::Five,
            6 => serialport::DataBits::Six,
            7 => serialport::DataBits::Seven,
            8 => serialport::DataBits::Eight,
            n => return Err(CoreError::InvalidRequest(format!("invalid data bits {n}"))),
        };
        let stop_bits = match cfg.stop_bits {
            1 => serialport::StopBits::One,
            2 => serialport::StopBits::Two,
            n => return Err(CoreError::InvalidRequest(format!("invalid stop bits {n}"))),
        };
        let lease = registry.acquire(&cfg.port, "Serial Monitor")?;
        let port = serialport::new(&cfg.port, cfg.baud)
            .data_bits(data_bits)
            .stop_bits(stop_bits)
            .parity(match cfg.parity {
                SerialParity::None => serialport::Parity::None,
                SerialParity::Odd => serialport::Parity::Odd,
                SerialParity::Even => serialport::Parity::Even,
            })
            .flow_control(match cfg.flow_control {
                FlowControl::None => serialport::FlowControl::None,
                FlowControl::Software => serialport::FlowControl::Software,
                FlowControl::Hardware => serialport::FlowControl::Hardware,
            })
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| CoreError::Serial(format!("cannot open {}: {e}", cfg.port)))?;
        let mut reader = port.try_clone().map_err(|e| CoreError::Serial(e.to_string()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let (stop2, alive2, name) = (stop.clone(), alive.clone(), cfg.port.clone());
        sink.status(SerialStatus::Connected {
            port: cfg.port.clone(),
        });
        let thread = std::thread::spawn(move || {
            let _lease = lease;
            let mut buf = [0u8; 1024];
            let reason = loop {
                if stop2.load(Ordering::SeqCst) {
                    break None;
                }
                match reader.read(&mut buf) {
                    Ok(0) => {}
                    Ok(n) => sink.data(&buf[..n], now_ms()),
                    Err(e)
                        if e.kind() == std::io::ErrorKind::TimedOut
                            || e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => break Some(format!("device error: {e}")),
                }
            };
            alive2.store(false, Ordering::SeqCst);
            sink.status(SerialStatus::Disconnected { port: name, reason });
        });
        Ok(SerialSession {
            port_name: cfg.port.clone(),
            writer: Mutex::new(port),
            stop,
            alive,
            thread: Some(thread),
        })
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub fn write(&self, bytes: &[u8]) -> CoreResult<()> {
        if !self.is_alive() {
            return Err(CoreError::Serial(format!("{} is disconnected", self.port_name)));
        }
        let mut w = self
            .writer
            .lock()
            .map_err(|_| CoreError::Serial("writer poisoned".into()))?;
        w.write_all(bytes)
            .map_err(|e| CoreError::Serial(format!("write failed: {e}")))?;
        w.flush()
            .map_err(|e| CoreError::Serial(format!("flush failed: {e}")))
    }

    pub fn close(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for SerialSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Parses user input such as `48 65 0x6C,6c 6F` into bytes.
pub fn parse_hex_bytes(input: &str) -> CoreResult<Vec<u8>> {
    input
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .map(|t| {
            let t = t.trim_start_matches("0x").trim_start_matches("0X");
            if t.len() > 2 {
                return Err(CoreError::InvalidRequest(format!("'{t}' is not a single byte")));
            }
            u8::from_str_radix(t, 16)
                .map_err(|_| CoreError::InvalidRequest(format!("'{t}' is not a hex byte")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_prevents_double_open() {
        let reg = PortRegistry::new();
        let lease = reg.acquire("/dev/ttyUSB0", "Serial Monitor").unwrap();
        let err = reg.acquire("/dev/ttyUSB0", "Programmer").err().unwrap();
        assert_eq!(err.kind(), "busy");
        assert!(err.to_string().contains("Serial Monitor"));
        drop(lease);
        assert!(reg.acquire("/dev/ttyUSB0", "Programmer").is_ok());
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex_bytes("48 65 0x6C,6c 6F").unwrap(), b"Hello");
        assert!(parse_hex_bytes("123").is_err());
        assert!(parse_hex_bytes("zz").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn pty_roundtrip_and_disconnect() {
        use serialport::SerialPort;
        use std::sync::mpsc;
        struct Sink(Mutex<mpsc::Sender<String>>);
        impl SerialSink for Sink {
            fn data(&self, bytes: &[u8], _ts: u64) {
                let _ = self
                    .0
                    .lock()
                    .unwrap()
                    .send(format!("data:{}", String::from_utf8_lossy(bytes)));
            }
            fn status(&self, s: SerialStatus) {
                let msg = match s {
                    SerialStatus::Connected { .. } => "connected".to_string(),
                    SerialStatus::Disconnected { .. } => "disconnected".to_string(),
                };
                let _ = self.0.lock().unwrap().send(msg);
            }
        }
        let (mut master, slave) = serialport::TTYPort::pair().expect("pty pair");
        let name = slave.name().expect("slave name");
        drop(slave);
        let (tx, rx) = mpsc::channel();
        let reg = PortRegistry::new();
        let cfg = SerialConfig {
            port: name.clone(),
            baud: 9600,
            data_bits: 8,
            parity: SerialParity::None,
            stop_bits: 1,
            flow_control: FlowControl::None,
        };
        let session = SerialSession::open(&cfg, &reg, Arc::new(Sink(Mutex::new(tx)))).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), "connected");
        assert!(
            reg.acquire(&name, "Programmer").is_err(),
            "port must be owned by the monitor"
        );
        master.write_all(b"ping").unwrap();
        let mut got = String::new();
        while !got.contains("ping") {
            got.push_str(
                &rx.recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .replace("data:", ""),
            );
        }
        session.write(b"pong").unwrap();
        let mut buf = [0u8; 4];
        master.set_timeout(Duration::from_secs(2)).unwrap();
        master.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"pong");
        session.close();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), "disconnected");
        assert!(
            reg.acquire(&name, "Programmer").is_ok(),
            "lease released after close"
        );
    }
}
