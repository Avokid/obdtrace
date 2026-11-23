//! Byte transports to an ELM327-compatible adapter.

pub mod ble;
pub mod mock;
#[cfg(target_os = "linux")]
mod rfcomm;
pub mod serial;
pub mod tcp;

use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::Serialize;
use std::time::Duration;

#[async_trait]
pub trait Transport: Send {
    async fn write(&mut self, data: &[u8]) -> Result<()>;
    /// Next chunk of bytes, or `None` when `timeout` elapsed without data.
    async fn read_chunk(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>>;
    async fn close(&mut self);
    fn describe(&self) -> String;
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AdapterInfo {
    pub kind: String,
    /// Value to pass to `connect`
    pub target: String,
    pub name: String,
    pub recommended: bool,
    pub note_key: &'static str,
    pub note: String,
}

/// Connection target syntax: `mock[:scenario[:quirk,quirk]]`, `ble[:name-or-address]`,
/// `rfcomm:MAC[@channel]` (Linux), `serial:PORT[@baud]`, `tcp:host:port`.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Mock {
        scenario: String,
        quirks: Vec<String>,
    },
    Ble {
        selector: Option<String>,
    },
    Rfcomm {
        address: [u8; 6],
        channel: u8,
    },
    Serial {
        port: String,
        baud: Option<u32>,
    },
    Tcp {
        addr: String,
    },
}

impl Target {
    pub fn parse(s: &str) -> Result<Target> {
        let (kind, rest) = s
            .split_once(':')
            .map(|(a, b)| (a, Some(b)))
            .unwrap_or((s, None));
        match kind {
            "mock" => {
                let mut it = rest.unwrap_or("").splitn(2, ':');
                let scenario = it.next().filter(|x| !x.is_empty()).unwrap_or("catalyst").to_string();
                let quirks = it.next().map(|q| q.split(',').map(str::to_string).collect()).unwrap_or_default();
                Ok(Target::Mock { scenario, quirks })
            }
            "ble" => Ok(Target::Ble { selector: rest.filter(|x| !x.is_empty()).map(str::to_string) }),
            "rfcomm" => {
                let rest = rest.unwrap_or_default();
                let (addr, channel) = rest.split_once('@').unwrap_or((rest, "1"));
                let channel = channel.parse::<u8>().ok().filter(|n| (1..=30).contains(n))
                    .ok_or_else(|| anyhow::anyhow!("RFCOMM channel must be between 1 and 30"))?;
                let parts: Vec<_> = addr.split(':').collect();
                if parts.len() != 6 || parts.iter().any(|p| p.len() != 2 || !p.bytes().all(|b| b.is_ascii_hexdigit())) {
                    bail!("use rfcomm:AA:BB:CC:DD:EE:FF[@channel] for classic Bluetooth");
                }
                let mut address = [0; 6];
                for (b, p) in address.iter_mut().zip(parts) {
                    *b = u8::from_str_radix(p, 16)?;
                }
                Ok(Target::Rfcomm { address, channel })
            }
            "serial" => {
                let Some(r) = rest.filter(|x| !x.is_empty()) else {
                    bail!("serial target needs a port, e.g. serial:COM3")
                };
                match r.rsplit_once('@') {
                    Some((p, b)) => {
                        let baud = b.parse::<u32>().ok().filter(|n| *n > 0)
                            .ok_or_else(|| anyhow::anyhow!("serial baud rate must be a positive integer"))?;
                        if p.is_empty() {
                            bail!("serial target needs a port, e.g. serial:COM3");
                        }
                        Ok(Target::Serial { port: p.to_string(), baud: Some(baud) })
                    }
                    None => Ok(Target::Serial { port: r.to_string(), baud: None }),
                }
            }
            "tcp" => Ok(Target::Tcp { addr: rest.filter(|x| !x.is_empty()).unwrap_or("192.168.0.10:35000").to_string() }),
            other => bail!("unknown connection type '{other}' (use mock, ble, rfcomm:MAC, serial:PORT or tcp:HOST:PORT)"),
        }
    }
}

pub async fn open(target: &Target) -> Result<Box<dyn Transport>> {
    Ok(match target {
        Target::Mock { scenario, quirks } => Box::new(mock::MockTransport::new(scenario, quirks)?),
        Target::Ble { selector } => Box::new(ble::BleTransport::connect(selector.as_deref()).await?),
        #[cfg(target_os = "linux")]
        Target::Rfcomm { address, channel } => Box::new(rfcomm::RfcommTransport::connect(*address, *channel).await?),
        #[cfg(not(target_os = "linux"))]
        Target::Rfcomm { .. } => bail!("direct RFCOMM is supported on Linux only; pair the adapter and use its outgoing serial port"),
        Target::Serial { port, baud } => Box::new(serial::SerialTransport::open(port, *baud)?),
        Target::Tcp { addr } => Box::new(tcp::TcpTransport::connect(addr).await?),
    })
}

pub async fn list_adapters(include_ble: bool) -> (Vec<AdapterInfo>, Vec<String>) {
    let mut found = vec![AdapterInfo {
        kind: "mock".into(),
        target: "mock:audi_demo".into(),
        name: "Demo vehicle (no hardware)".into(),
        recommended: false,
        note_key: "demo",
        note: "Simulated car with a stored P0420".into(),
    }];
    let mut warnings = Vec::new();
    match serial::list_ports() {
        Ok(ports) => found.extend(ports),
        Err(e) => warnings.push(format!(
            "Serial port listing failed: {e}; you can still enter the port manually"
        )),
    }
    if include_ble {
        #[cfg(target_os = "linux")]
        let (ble, paired) = tokio::join!(ble::scan(), rfcomm::paired_adapters());
        #[cfg(not(target_os = "linux"))]
        let ble = ble::scan().await;
        #[cfg(target_os = "linux")]
        match paired {
            Ok(v) => found.extend(v),
            Err(e) => warnings.push(format!("Paired Bluetooth device listing failed: {e}; RFCOMM targets can be entered manually")),
        }
        match ble {
            Ok(v) => found.extend(v),
            Err(e) => warnings.push(format!("Bluetooth LE scan failed: {e}")),
        }
    }
    found.sort_by_key(|a| match (a.recommended, a.kind.as_str()) {
        (true, _) => 0,
        (_, "rfcomm") => 1,
        (_, "ble") => 2,
        (_, "serial") => 3,
        _ => 4,
    });
    (found, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_targets() {
        assert_eq!(
            Target::parse("ble").unwrap(),
            Target::Ble { selector: None }
        );
        assert_eq!(
            Target::parse("serial:COM3@115200").unwrap(),
            Target::Serial {
                port: "COM3".into(),
                baud: Some(115200)
            }
        );
        assert_eq!(
            Target::parse("mock:lean:echo_stuck").unwrap(),
            Target::Mock {
                scenario: "lean".into(),
                quirks: vec!["echo_stuck".into()]
            }
        );
        assert!(Target::parse("foo").is_err());
        assert!(Target::parse("serial:COM3@typo").is_err());
        assert_eq!(
            Target::parse("rfcomm:01:23:45:67:89:ab").unwrap(),
            Target::Rfcomm {
                address: [0x01, 0x23, 0x45, 0x67, 0x89, 0xab],
                channel: 1
            }
        );
        assert!(matches!(
            Target::parse("rfcomm:01:23:45:67:89:ab@12"),
            Ok(Target::Rfcomm { channel: 12, .. })
        ));
        assert!(Target::parse("rfcomm:01:23:45:67:89:ab@31").is_err());
        assert!(Target::parse("rfcomm:01:23:45:67:89:+1").is_err());
    }
}
