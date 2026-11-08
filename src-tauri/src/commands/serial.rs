//! Serial monitor commands. A single monitor session at a time; the port is
//! leased through the shared registry so programmer operations cannot use it
//! concurrently.

use super::Shared;
use crate::state::lock;
use avr_core::programmer::validate_port;
use avr_core::serial::{
    PortInfo, SerialConfig, SerialSession, SerialSink, SerialStatus, list_ports, parse_hex_bytes,
};
use avr_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DataEvent {
    bytes: Vec<u8>,
    timestamp_ms: u64,
}

struct EventSink(AppHandle);

impl SerialSink for EventSink {
    fn data(&self, bytes: &[u8], timestamp_ms: u64) {
        let _ = self.0.emit(
            "serial://data",
            DataEvent {
                bytes: bytes.to_vec(),
                timestamp_ms,
            },
        );
    }
    fn status(&self, status: SerialStatus) {
        let _ = self.0.emit("serial://status", status);
    }
}

#[tauri::command]
pub async fn serial_list_ports(state: Shared<'_>) -> CoreResult<Vec<PortInfo>> {
    let state = state.inner().clone();
    super::blocking(move || list_ports(&state.ports)).await
}

#[tauri::command]
pub async fn serial_open(app: AppHandle, state: Shared<'_>, config: SerialConfig) -> CoreResult<()> {
    validate_port(&config.port)?;
    let state = state.inner().clone();
    super::blocking(move || {
        let mut current = lock(&state.serial)?;
        if let Some(s) = current.as_ref()
            && s.is_alive()
        {
            return Err(CoreError::Busy(format!(
                "the serial monitor is already connected to {}",
                s.port_name()
            )));
        }
        // Drop a dead session (device unplugged) before opening a new one.
        if let Some(old) = current.take() {
            old.close();
        }
        *current = Some(SerialSession::open(
            &config,
            &state.ports,
            Arc::new(EventSink(app)),
        )?);
        Ok(())
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineEnding {
    None,
    Lf,
    Cr,
    CrLf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode")]
pub enum SerialPayload {
    Text { text: String, line_ending: LineEnding },
    Hex { hex: String },
}

/// Encodes a payload into bytes. Exposed for tests.
pub fn encode_payload(payload: &SerialPayload) -> CoreResult<Vec<u8>> {
    let bytes = match payload {
        SerialPayload::Text { text, line_ending } => {
            let mut b = text.as_bytes().to_vec();
            b.extend_from_slice(match line_ending {
                LineEnding::None => b"",
                LineEnding::Lf => b"\n",
                LineEnding::Cr => b"\r",
                LineEnding::CrLf => b"\r\n",
            });
            b
        }
        SerialPayload::Hex { hex } => parse_hex_bytes(hex)?,
    };
    if bytes.len() > 4096 {
        return Err(CoreError::InvalidRequest(
            "a single send is limited to 4096 bytes".into(),
        ));
    }
    Ok(bytes)
}

#[tauri::command]
pub async fn serial_write(state: Shared<'_>, payload: SerialPayload) -> CoreResult<usize> {
    let bytes = encode_payload(&payload)?;
    let state = state.inner().clone();
    super::blocking(move || {
        let current = lock(&state.serial)?;
        let session = current
            .as_ref()
            .ok_or_else(|| CoreError::Serial("the serial monitor is not connected".into()))?;
        session.write(&bytes)?;
        Ok(bytes.len())
    })
    .await
}

#[tauri::command]
pub async fn serial_close(state: Shared<'_>) -> CoreResult<bool> {
    let state = state.inner().clone();
    super::blocking(move || {
        let session = lock(&state.serial)?.take();
        let was_open = session.is_some();
        if let Some(s) = session {
            s.close();
        }
        Ok(was_open)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_text_with_line_endings() {
        let p = SerialPayload::Text {
            text: "hi".into(),
            line_ending: LineEnding::CrLf,
        };
        assert_eq!(encode_payload(&p).unwrap(), b"hi\r\n");
    }

    #[test]
    fn encodes_hex() {
        let p = SerialPayload::Hex {
            hex: "48 0x69".into(),
        };
        assert_eq!(encode_payload(&p).unwrap(), b"Hi");
    }

    #[test]
    fn rejects_oversized_payloads() {
        let p = SerialPayload::Text {
            text: "x".repeat(5000),
            line_ending: LineEnding::None,
        };
        assert!(encode_payload(&p).is_err());
    }
}
