use super::{AdapterInfo, Transport};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{SerialPortBuilderExt, SerialPortType, SerialStream};

pub struct SerialTransport {
    port: SerialStream,
    name: String,
}

impl SerialTransport {
    pub fn open(path: &str, baud: Option<u32>) -> Result<Self> {
        let port = tokio_serial::new(path, baud.unwrap_or(38400))
            .open_native_async()
            .with_context(|| {
                format!("cannot open serial port {path}; close other apps using it")
            })?;
        Ok(Self {
            port,
            name: path.to_string(),
        })
    }
}

#[async_trait]
impl Transport for SerialTransport {
    async fn write(&mut self, data: &[u8]) -> Result<()> {
        self.port.write_all(data).await?;
        Ok(())
    }

    async fn read_chunk(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let mut buf = [0u8; 256];
        match tokio::time::timeout(timeout, self.port.read(&mut buf)).await {
            Err(_) | Ok(Ok(0)) => Ok(None),
            Ok(Ok(n)) => Ok(Some(buf[..n].to_vec())),
            Ok(Err(e)) => Err(e.into()),
        }
    }

    async fn close(&mut self) {}

    fn describe(&self) -> String {
        format!("serial:{}", self.name)
    }
}

pub fn list_ports() -> Result<Vec<AdapterInfo>> {
    let mut ports: Vec<_> = tokio_serial::available_ports()?
        .into_iter()
        .map(|p| {
            let n = p.port_name.to_lowercase();
            let bt = matches!(p.port_type, SerialPortType::BluetoothPort)
                || n.contains("bluetooth")
                || n.contains("rfcomm");
            let (note_key, note) = if bt {
                (
                    "bluetoothSerial",
                    "Bluetooth (classic) serial port; choose the outgoing port",
                )
            } else if matches!(p.port_type, SerialPortType::UsbPort(_)) {
                ("usbSerial", "USB serial port")
            } else {
                (
                    "otherSerial",
                    "Other serial port (may be a paired Bluetooth adapter)",
                )
            };
            AdapterInfo {
                kind: "serial".into(),
                target: format!("serial:{}", p.port_name),
                name: p.port_name.clone(),
                recommended: bt,
                note_key,
                note: note.into(),
            }
        })
        .collect();
    ports.sort_by_key(|p| !p.recommended);
    Ok(ports)
}
