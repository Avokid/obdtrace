//! Bluetooth LE transport (btleplug: BlueZ on Linux, CoreBluetooth on macOS, WinRT on Windows).
//!
//! Most adapters do not need pairing. Try known UART profiles first, then
//! confirm other GATT candidates with an ELM identification command.

use super::{AdapterInfo, Transport};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use btleplug::api::{
    BDAddr, Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter,
    ValueNotification, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::{stream::BoxStream, StreamExt};
use std::collections::{BTreeSet, HashSet};
use std::future::Future;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;

const SCAN_TIME: Duration = Duration::from_secs(6);
const GATT_TIMEOUT: Duration = Duration::from_secs(5);
static SCAN: Mutex<()> = Mutex::const_new(());

const fn short(id: u16) -> Uuid {
    Uuid::from_u128(((id as u128) << 96) | 0x0000_0000_0000_1000_8000_0080_5f9b_34fb)
}

/// Known UART-over-GATT profiles: (service, notify characteristic, write characteristic).
const PROFILES: &[(Uuid, Uuid, Uuid)] = &[
    (short(0xFFF0), short(0xFFF1), short(0xFFF2)),
    (short(0xFFE0), short(0xFFE1), short(0xFFE1)),
    (short(0x18F0), short(0x2AF0), short(0x2AF1)),
    (
        Uuid::from_u128(0x6E400001_B5A3_F393_E0A9_E50E24DCCA9E),
        Uuid::from_u128(0x6E400003_B5A3_F393_E0A9_E50E24DCCA9E),
        Uuid::from_u128(0x6E400002_B5A3_F393_E0A9_E50E24DCCA9E),
    ),
    (
        Uuid::from_u128(0xE7810A71_73AE_499D_8C15_FAA9AEF0C3F2),
        Uuid::from_u128(0xBEF8D6C9_9C21_4C9E_B632_BD58C1009F9F),
        Uuid::from_u128(0xBEF8D6C9_9C21_4C9E_B632_BD58C1009F9F),
    ),
];

const NAME_HINTS: &[&str] = &[
    "obd",
    "elm",
    "vgate",
    "icar",
    "vlink",
    "veepeak",
    "konnwei",
    "lelink",
    "carista",
    "viecar",
    "kw9",
    "bluedriver",
    "scan",
    "mini",
    "v-link",
    "fixd",
    "autel",
    "thinkdiag",
    "obdlink",
];

fn likely_adapter(name: &str) -> bool {
    let n = name.to_lowercase();
    NAME_HINTS.iter().any(|h| n.contains(h))
}

fn is_standard_service(id: &Uuid) -> bool {
    [0x1800, 0x1801, 0x180a, 0x180f, 0x1812]
        .iter()
        .any(|s| *id == short(*s))
}

async fn adapters() -> Result<Vec<Adapter>> {
    let manager = Manager::new().await.map_err(|e| {
        anyhow!("Bluetooth is unavailable: {e}. Turn Bluetooth on and allow this app to use it.")
    })?;
    let adapters = manager
        .adapters()
        .await
        .map_err(|e| anyhow!("cannot list Bluetooth adapters: {e}"))?;
    if adapters.is_empty() {
        bail!("no Bluetooth controller found. Turn Bluetooth on (Linux: check that the bluetooth service is running).");
    }
    Ok(adapters)
}

struct Seen {
    id: String,
    address: Option<BDAddr>,
    name: String,
    known_service: bool,
    peripheral: Peripheral,
}

async fn collect(central: &Adapter, dur: Duration) -> Result<Vec<Seen>> {
    bounded(central.start_scan(ScanFilter::default()))
        .await
        .context("Bluetooth scan could not start (permission denied or Bluetooth is off)")?;
    tokio::time::sleep(dur).await;
    let _ = bounded(central.stop_scan()).await;
    let mut out = Vec::new();
    for (p, props) in
        futures::future::join_all(bounded(central.peripherals()).await?.into_iter().map(
            |p| async move {
                let props = bounded(p.properties()).await.ok().flatten();
                (p, props)
            },
        ))
        .await
    {
        let known_service = props
            .as_ref()
            .is_some_and(|x| PROFILES.iter().any(|(s, _, _)| x.services.contains(s)));
        let address = props
            .as_ref()
            .map(|x| x.address)
            .filter(|a| *a != BDAddr::default());
        let name = props
            .and_then(|x| x.local_name)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_default();
        out.push(Seen {
            id: p.id().to_string(),
            address,
            name,
            known_service,
            peripheral: p,
        });
    }
    Ok(out)
}

async fn discover(dur: Duration) -> Result<Vec<Seen>> {
    let _scan = SCAN.lock().await;
    let adapters = tokio::time::timeout(GATT_TIMEOUT, adapters()).await??;
    let mut seen = Vec::new();
    let mut errors = Vec::new();
    let mut success = false;
    for result in futures::future::join_all(adapters.iter().map(|a| collect(a, dur))).await {
        match result {
            Ok(devices) => {
                success = true;
                seen.extend(devices);
            }
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    if !success {
        bail!("{}", errors.join("; "));
    }
    Ok(seen)
}

fn recommendation(s: &Seen) -> bool {
    s.known_service || likely_adapter(&s.name)
}

pub async fn scan() -> Result<Vec<AdapterInfo>> {
    let seen = discover(SCAN_TIME).await?;
    let mut out: Vec<AdapterInfo> = seen
        .into_iter()
        .map(|s| {
            let rec = recommendation(&s);
            AdapterInfo {
                kind: "ble".into(),
                target: format!("ble:{}", s.id),
                name: if s.name.is_empty() {
                    format!("BLE · {}", s.id)
                } else {
                    format!("{} · {}", s.name, s.id)
                },
                recommended: rec,
                note_key: if rec { "likelyBle" } else { "otherBle" },
                note: if rec {
                    "Possible OBD-II adapter; checked on connect".into()
                } else {
                    "Other Bluetooth LE device; select only if this is your adapter".into()
                },
            }
        })
        .collect();
    out.sort_by(|a, b| (!a.recommended, &a.target).cmp(&(!b.recommended, &b.target)));
    let mut unique = HashSet::new();
    out.retain(|a| unique.insert(a.target.clone()));
    Ok(out)
}

async fn bounded<T>(f: impl Future<Output = btleplug::Result<T>>) -> Result<T> {
    Ok(tokio::time::timeout(GATT_TIMEOUT, f)
        .await
        .context("Bluetooth operation timed out")??)
}

pub struct BleTransport {
    peripheral: Peripheral,
    notify_char: Characteristic,
    write_char: Characteristic,
    write_type: WriteType,
    notifications: BoxStream<'static, ValueNotification>,
    name: String,
}

fn match_priority(
    selector: Option<&str>,
    id: &str,
    address: Option<BDAddr>,
    name: &str,
    recommended: bool,
) -> Option<u8> {
    match selector {
        Some(s) if id.eq_ignore_ascii_case(s) => Some(0),
        Some(s) if address.is_some() && s.parse::<BDAddr>().ok() == address => Some(1),
        Some(s) if name.eq_ignore_ascii_case(s) => Some(2),
        Some(s) if name.to_lowercase().contains(&s.to_lowercase()) => Some(3),
        None if recommended => Some(0),
        _ => None,
    }
}

impl BleTransport {
    pub async fn connect(selector: Option<&str>) -> Result<Self> {
        let mut matches: Vec<_> = discover(SCAN_TIME)
            .await?
            .into_iter()
            .filter_map(|s| {
                match_priority(selector, &s.id, s.address, &s.name, recommendation(&s))
                    .map(|priority| (priority, s))
            })
            .collect();
        let Some(priority) = matches.iter().map(|(p, _)| *p).min() else {
            bail!("no matching Bluetooth LE adapter found; scan and select the adapter by ID (unnamed devices are listed too)");
        };
        matches.retain(|(p, _)| *p == priority);
        let ids: BTreeSet<_> = matches
            .iter()
            .map(|(_, s)| {
                s.address
                    .map_or_else(|| s.id.to_ascii_lowercase(), |a| a.to_string())
            })
            .collect();
        if ids.len() > 1 {
            bail!("several Bluetooth LE devices match ({}); choose the exact target from list_adapters", ids.into_iter().collect::<Vec<_>>().join(", "));
        }
        let mut errors = Vec::new();
        for (_, device) in matches {
            match Self::open_device(&device).await {
                Ok(transport) => return Ok(transport),
                Err(e) => {
                    errors.push(format!("{e:#}"));
                    let _ = bounded(device.peripheral.disconnect()).await;
                }
            }
        }
        bail!("Bluetooth LE connection failed: {}. If this is a classic Bluetooth adapter, pair it in the OS and use SPP instead", errors.join("; "))
    }

    async fn open_device(device: &Seen) -> Result<Self> {
        bounded(device.peripheral.connect())
            .await
            .context("could not connect to the BLE device (is another app connected to it?)")?;
        bounded(device.peripheral.discover_services())
            .await
            .context("could not discover GATT services")?;
        let candidates = candidate_channels(&device.peripheral.characteristics());
        if candidates.is_empty() {
            bail!("device has no supported GATT write/notification channel");
        }
        tokio::time::timeout(Duration::from_secs(40), async {
            let mut last_error = None;
            for (notify, write, write_type) in candidates {
                let result = async {
                    let notifications = bounded(device.peripheral.notifications()).await?;
                    tokio::time::timeout(
                        Duration::from_secs(20),
                        device.peripheral.subscribe(&notify),
                    )
                    .await
                    .context(
                        "GATT subscription timed out; complete any Bluetooth pairing prompt",
                    )??;
                    let mut transport = Self {
                        peripheral: device.peripheral.clone(),
                        notify_char: notify.clone(),
                        write_char: write,
                        write_type,
                        notifications,
                        name: if device.name.is_empty() {
                            device.id.clone()
                        } else {
                            device.name.clone()
                        },
                    };
                    transport.identify().await?;
                    Ok::<_, anyhow::Error>(transport)
                }
                .await;
                match result {
                    Ok(transport) => return Ok(transport),
                    Err(e) => {
                        last_error = Some(e);
                        let _ = bounded(device.peripheral.unsubscribe(&notify)).await;
                    }
                }
            }
            Err(last_error.unwrap())
                .context("no GATT channel answered as an ELM-compatible adapter")
        })
        .await
        .context("timed out trying GATT channels")?
    }

    async fn identify(&mut self) -> Result<()> {
        for (command, wait) in [
            ("ATI", Duration::from_millis(1300)),
            ("ATZ", Duration::from_secs(3)),
        ] {
            self.write(format!("{command}\r").as_bytes()).await?;
            let answer = tokio::time::timeout(wait, async {
                let mut buf = Vec::new();
                while buf.len() < 1024 {
                    let Some(chunk) = self.read_chunk(wait).await? else {
                        return Ok(false);
                    };
                    buf.extend(chunk);
                    if let Some(end) = buf.iter().position(|b| *b == b'>') {
                        return Ok(is_identity_reply(command, &buf[..end]));
                    }
                }
                Ok::<_, anyhow::Error>(false)
            })
            .await;
            if answer.unwrap_or(Ok(false))? {
                return Ok(());
            }
        }
        bail!("no adapter identification response")
    }
}

fn is_identity_reply(command: &str, reply: &[u8]) -> bool {
    String::from_utf8_lossy(reply)
        .split(['\r', '\n'])
        .any(|line| {
            let line = line.trim();
            let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if compact.eq_ignore_ascii_case(command) || !line.is_ascii() {
                return false;
            }
            let upper = line.to_ascii_uppercase();
            ["ELM", "OBD", "STN", "VLINK"]
                .iter()
                .any(|prefix| upper.starts_with(prefix))
                || (line.bytes().any(|b| b.is_ascii_digit()) && line.contains('.'))
        })
}

fn candidate_channels(
    chars: &BTreeSet<Characteristic>,
) -> Vec<(Characteristic, Characteristic, WriteType)> {
    let can_notify = |c: &Characteristic| {
        c.properties
            .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
    };
    let mut pairs = Vec::new();
    for notify in chars
        .iter()
        .filter(|c| can_notify(c) && !is_standard_service(&c.service_uuid))
    {
        for write in chars
            .iter()
            .filter(|c| !is_standard_service(&c.service_uuid))
        {
            let known = PROFILES.iter().any(|(s, n, w)| {
                *s == notify.service_uuid
                    && *s == write.service_uuid
                    && *n == notify.uuid
                    && *w == write.uuid
            });
            let priority = if known {
                0
            } else if notify == write {
                1
            } else if notify.service_uuid == write.service_uuid {
                2
            } else {
                3
            };
            for (property, mode) in [
                (
                    CharPropFlags::WRITE_WITHOUT_RESPONSE,
                    WriteType::WithoutResponse,
                ),
                (CharPropFlags::WRITE, WriteType::WithResponse),
            ] {
                if write.properties.contains(property) {
                    pairs.push((priority, notify.clone(), write.clone(), mode));
                }
            }
        }
    }
    pairs.sort_by_key(|(priority, _, _, _)| *priority);
    pairs
        .into_iter()
        .map(|(_, n, w, mode)| (n, w, mode))
        .collect()
}

#[async_trait]
impl Transport for BleTransport {
    async fn write(&mut self, data: &[u8]) -> Result<()> {
        // Cheap adapters drop writes larger than the default ATT payload or sent back-to-back.
        for chunk in data.chunks(20) {
            bounded(
                self.peripheral
                    .write(&self.write_char, chunk, self.write_type),
            )
            .await
            .context("Bluetooth write failed (adapter disconnected?)")?;
            tokio::time::sleep(Duration::from_millis(8)).await;
        }
        Ok(())
    }

    async fn read_chunk(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let uuid = self.notify_char.uuid;
        match tokio::time::timeout(timeout, async {
            while let Some(event) = self.notifications.next().await {
                if event.uuid == uuid && !event.value.is_empty() {
                    return Some(event.value);
                }
            }
            None
        })
        .await
        {
            Err(_) => Ok(None),
            Ok(Some(v)) => Ok(Some(v)),
            Ok(None) => bail!("Bluetooth adapter disconnected"),
        }
    }

    async fn close(&mut self) {
        let _ = bounded(self.peripheral.disconnect()).await;
    }

    fn describe(&self) -> String {
        format!("ble:{}", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn characteristic(service_uuid: Uuid, uuid: Uuid, properties: CharPropFlags) -> Characteristic {
        Characteristic {
            service_uuid,
            uuid,
            properties,
            descriptors: BTreeSet::new(),
        }
    }

    #[test]
    fn known_profile_first_with_both_write_modes_and_unknown_fallbacks() {
        let uart = short(0xfff0);
        let unknown = short(0xfa01);
        let chars = BTreeSet::from([
            characteristic(uart, short(0xfff1), CharPropFlags::NOTIFY),
            characteristic(
                uart,
                short(0xfff2),
                CharPropFlags::WRITE | CharPropFlags::WRITE_WITHOUT_RESPONSE,
            ),
            characteristic(unknown, short(0xfa02), CharPropFlags::INDICATE),
            characteristic(unknown, short(0xfa03), CharPropFlags::WRITE),
            characteristic(short(0xfa04), short(0xfa05), CharPropFlags::WRITE),
            characteristic(
                short(0x180f),
                short(0x2a19),
                CharPropFlags::NOTIFY | CharPropFlags::WRITE,
            ),
        ]);
        let pairs = candidate_channels(&chars);
        assert_eq!(
            (pairs[0].0.uuid, pairs[0].1.uuid, pairs[0].2),
            (short(0xfff1), short(0xfff2), WriteType::WithoutResponse)
        );
        assert_eq!(pairs[1].2, WriteType::WithResponse);
        assert!(pairs
            .iter()
            .any(|(n, w, _)| n.uuid == short(0xfa02) && w.uuid == short(0xfa03)));
        assert!(pairs
            .iter()
            .any(|(n, w, _)| n.uuid == short(0xfa02) && w.uuid == short(0xfa05)));
        assert!(pairs
            .iter()
            .all(|(n, w, _)| n.uuid != short(0x2a19) && w.uuid != short(0x2a19)));
    }

    #[test]
    fn selection_prefers_id_and_does_not_depend_on_name_being_present() {
        assert_eq!(match_priority(Some("abc"), "ABC", None, "", false), Some(0));
        let mac = "01:23:45:67:89:AB";
        assert_eq!(
            match_priority(
                Some(mac),
                "hci0/dev_01_23_45_67_89_AB",
                mac.parse().ok(),
                "",
                false
            ),
            Some(1)
        );
        assert_eq!(
            match_priority(Some("OBD"), "123", None, "obd", false),
            Some(2)
        );
        assert_eq!(
            match_priority(Some("OBD"), "123", None, "My obd", false),
            Some(3)
        );
        assert_eq!(match_priority(None, "123", None, "", true), Some(0));
        assert_eq!(match_priority(None, "123", None, "", false), None);
    }

    #[test]
    fn discovery_probe_needs_identity_not_just_an_echo_or_prompt() {
        assert!(is_identity_reply("ATI", b"AT I\rELM327 v1.5\r"));
        assert!(is_identity_reply("ATZ", b"\rSTN1170 v5.0\r"));
        assert!(!is_identity_reply("ATI", b"AT I\r?\r"));
        assert!(!is_identity_reply("ATZ", b"ATZ\rOK\r"));
    }
}
