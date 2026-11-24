//! Shared application state: adapter connection, background sampler, ring buffer,
//! recorder. Both the web UI and the MCP tools operate through the Hub, so there is
//! exactly one owner of the adapter connection.

use crate::elm::{Elm, ElmError, InitReport};
use crate::obd::{self, PidDef, DEFAULT_PIDS};
use crate::redact::mask_vin;
use crate::store::{data_dir, iso, now_ms, Db, Ring, Sample};
use crate::transport::{self, Target};
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, Mutex as AsyncMutex};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Serialize, Default)]
pub struct Status {
    pub connected: bool,
    pub connecting: bool,
    pub adapter: String,
    pub protocol: String,
    pub version: String,
    pub voltage: Option<String>,
    pub error: Option<String>,
    pub vin_masked: Option<String>,
    pub notes: Vec<String>,
    pub unsupported_commands: Vec<String>,
    pub supported_pid_count: usize,
}

struct Rec {
    id: i64,
    label: String,
    count: usize,
    started_ms: i64,
}

struct Inner {
    target: Option<String>,
    status: Status,
    ring: Ring,
    pids: Option<Vec<String>>,
    audi_demo: bool,
    interval_ms: u64,
    recording: Option<Rec>,
    auto_record: bool,
    clients: Vec<Value>,
    last_tool: Option<Value>,
    latest: HashMap<String, Sample>,
    supported: Option<BTreeSet<u8>>,
    sampler_gen: u64,
    last_dtcs: Option<Value>,
    session_id: Option<i64>,
    pending: Vec<Sample>,
    storage_error: Option<String>,
}

pub struct Hub {
    operations: AsyncMutex<()>,
    identity: crate::identity::Identity,
    _ownership: Option<std::fs::File>,
    sampler_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    elm: AsyncMutex<Option<Elm>>,
    inner: Mutex<Inner>,
    db: Mutex<Db>,
    events: broadcast::Sender<String>,
    dir: PathBuf,
}

impl Inner {
    fn sampling_pids(&self) -> Vec<String> {
        self.pids.clone().unwrap_or_else(|| {
            if self.audi_demo {
                obd::PIDS.iter().map(|p| p.name.to_string()).collect()
            } else {
                DEFAULT_PIDS.iter().map(|p| p.to_string()).collect()
            }
        })
    }
}

fn friendly(e: &ElmError) -> String {
    match e {
        ElmError::UnableToConnect => "The adapter answers, but the car does not. Turn the ignition ON (engine running if needed), push the adapter in firmly and try again.".into(),
        other => other.to_string(),
    }
}

async fn open_and_init(target: &Target) -> Result<(Elm, InitReport)> {
    let t = transport::open(target).await?;
    let mut elm = Elm::new(t);
    match elm.init().await {
        Ok(r) => Ok((elm, r)),
        Err(e) => {
            elm.close().await;
            Err(anyhow!(friendly(&e)))
        }
    }
}

impl Hub {
    pub fn new(dir: PathBuf, db: Db) -> Result<Arc<Self>> {
        let identity = crate::identity::Identity::open(&dir, db.has_vehicle_keys()?)?;
        let ring_s = std::env::var("OBDTRACE_RING_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);
        let (events, _) = broadcast::channel(256);
        Ok(Arc::new(Self {
            operations: AsyncMutex::new(()),
            identity,
            _ownership: None,
            sampler_task: Mutex::new(None),
            elm: AsyncMutex::new(None),
            inner: Mutex::new(Inner {
                target: None,
                status: Status::default(),
                ring: Ring::new(ring_s),
                pids: None,
                audi_demo: false,
                interval_ms: 0,
                recording: None,
                auto_record: true,
                clients: vec![],
                last_tool: None,
                latest: HashMap::new(),
                supported: None,
                sampler_gen: 0,
                last_dtcs: None,
                session_id: None,
                pending: vec![],
                storage_error: None,
            }),
            db: Mutex::new(db),
            events,
            dir,
        }))
    }

    pub fn open_default() -> Result<Arc<Self>> {
        crate::bridge::prepare_dir()?;
        let dir = data_dir();
        std::fs::create_dir_all(&dir)?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("app.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| anyhow!("another process owns this data directory; use its local app"))?;
        let db = Db::open(&dir.join("recordings.db"))?;
        db.recover_history()?;
        let mut hub = Self::new(dir, db)?;
        Arc::get_mut(&mut hub).unwrap()._ownership = Some(lock);
        Ok(hub)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.events.subscribe()
    }

    fn emit(&self, v: Value) {
        let _ = self.events.send(v.to_string());
    }

    fn with<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        f(&mut self.inner.lock().unwrap())
    }

    pub fn note_client(&self, name: &str, version: &str) {
        self.with(|i| {
            i.clients.retain(|c| c["name"] != name);
            i.clients
                .push(json!({"name": name, "version": version, "connected_at": iso(now_ms())}));
        });
        self.emit(json!({"type": "client", "name": name, "version": version}));
    }

    pub fn note_tool(&self, tool: &str, origin: &str) {
        self.with(|i| {
            i.last_tool = Some(json!({"tool": tool, "origin": origin, "at": iso(now_ms())}))
        });
        if origin == "mcp" {
            self.emit(json!({"type": "tool_call", "tool": tool, "at": iso(now_ms())}));
        }
    }

    pub fn status(&self) -> Value {
        self.with(|i| {
            let (n, raw) = i.ring.len();
            let rec = i.recording.as_ref().map(|r| json!({"id": r.id, "label": r.label, "samples": r.count, "seconds": (now_ms() - r.started_ms) / 1000}));
            json!({
                "app_version": APP_VERSION, "os": std::env::consts::OS, "data_dir": self.dir.to_string_lossy(),
                "adapter": i.status, "target": i.target,
                "ring_buffer": {"window_seconds": i.ring.window_s(), "samples": n, "raw_entries": raw, "span_seconds": i.ring.span_ms() / 1000},
                "sampling": {"pids": i.sampling_pids(), "interval_ms": i.interval_ms},
                "recording": rec, "auto_record": i.auto_record,
                "mcp_clients": i.clients, "last_tool_call": i.last_tool,
                "session_id": i.session_id, "storage_error":i.storage_error,
                "last_dtc_scan":i.last_dtcs, "dtc_target_interval_s":30,
            })
        })
    }

    pub async fn connect(self: &Arc<Self>, target_s: &str) -> Result<Value> {
        let _operation = self.operations.lock().await;
        let target = Target::parse(target_s)?;
        let audi_demo = matches!(&target, Target::Mock { scenario, .. } if scenario == transport::mock::AUDI_DEMO);
        self.disconnect_inner().await?;
        self.with(|i| {
            i.ring = Ring::new(i.ring.window_s());
            i.latest.clear();
            i.supported = None;
            i.last_dtcs = None;
            i.status = Status {
                connecting: true,
                ..Default::default()
            };
            i.target = Some(target_s.to_string());
        });
        self.emit(json!({"type": "status", "status": self.status()}));
        let (mut elm, report) = match open_and_init(&target).await {
            Ok(x) => x,
            Err(e) => {
                self.with(|i| {
                    i.status = Status {
                        error: Some(e.to_string()),
                        ..Default::default()
                    }
                });
                self.emit(json!({"type": "status", "status": self.status()}));
                return Err(e);
            }
        };
        let supported = elm.supported_pids().await.ok();
        let full_vin = elm.read_vin().await.ok();
        let vin = full_vin.as_ref().map(|v| mask_vin(v));
        let vehicle =
            if let Some(v) = full_vin
                .as_ref()
                .filter(|v| crate::redact::is_plausible_vin(v))
            {
                Some(self.db.lock().unwrap().resolve_vehicle(
                    &self.identity.fingerprint(v)?,
                    vin.as_deref().unwrap_or(""),
                )?)
            } else {
                None
            };
        let raw = elm.take_raw();
        let adapter = elm.describe();
        let demo = audi_demo
            .then(|| transport::mock::MockTransport::demo_history(now_ms()))
            .transpose()?;
        let session_result = self.db.lock().unwrap().start_history_with_demo(
            &adapter,
            &report.protocol,
            vin.as_deref().unwrap_or(""),
            vehicle,
            demo.as_deref(),
        );
        let session = match session_result {
            Ok(id) => id,
            Err(e) => {
                elm.close().await;
                self.with(|i| {
                    i.status.connecting = false;
                    i.status.error = Some(e.to_string());
                });
                self.storage_failure(&e);
                return Err(e);
            }
        };
        *self.elm.lock().await = Some(elm);
        let gen = self.with(|i| {
            i.session_id = Some(session);
            i.audi_demo = audi_demo;
            i.last_dtcs = None;
            i.storage_error = None;
            i.ring = Ring::new(i.ring.window_s());
            for r in raw {
                i.ring.push_raw(r);
            }
            i.status = Status {
                connected: true,
                connecting: false,
                adapter,
                protocol: report.protocol,
                version: report.version,
                voltage: report.voltage,
                error: None,
                vin_masked: vin,
                notes: report.notes,
                unsupported_commands: report.unsupported_commands,
                supported_pid_count: supported.as_ref().map_or(0, |s| s.len()),
            };
            i.supported = supported;
            i.sampler_gen += 1;
            i.sampler_gen
        });
        if let Err(e) = self.scan_dtcs().await {
            self.storage_failure(&e);
        }
        self.persist_event("sampling", self.status()["sampling"].clone())?;
        self.spawn_sampler(gen);
        self.emit(json!({"type": "status", "status": self.status()}));
        Ok(self.status())
    }

    pub async fn disconnect(&self) -> Result<()> {
        let _operation = self.operations.lock().await;
        self.disconnect_inner().await
    }

    async fn disconnect_inner(&self) -> Result<()> {
        self.with(|i| {
            i.sampler_gen += 1;
            i.status.connected = false;
            i.status.connecting = false;
            i.latest.clear();
        });
        let task = self.sampler_task.lock().unwrap().take();
        if let Some(task) = task {
            task.abort();
            if let Err(e) = task.await {
                if !e.is_cancelled() {
                    return Err(e.into());
                }
            }
        }
        if let Some(mut e) = self.elm.lock().await.take() {
            e.close().await;
        }
        if let Err(e) = self.flush_recording() {
            self.storage_failure(&e);
            return Err(e);
        }
        if let Some(id) = self.with(|i| i.session_id) {
            self.db.lock().unwrap().finish_history(id, "disconnect")?;
        }
        self.with(|i| {
            i.session_id = None;
            i.recording = None;
        });
        self.emit(json!({"type": "status", "status": self.status()}));
        Ok(())
    }

    fn spawn_sampler(self: &Arc<Self>, gen: u64) {
        let hub = self.clone();
        *self.sampler_task.lock().unwrap() =
            Some(tokio::spawn(async move { hub.sampler(gen).await }));
    }

    fn current(&self, gen: u64) -> bool {
        self.with(|i| i.sampler_gen == gen)
    }

    async fn sampler(self: Arc<Self>, gen: u64) {
        let mut cycle = 0u64;
        let mut fails: HashMap<u8, u32> = HashMap::new();
        let mut dead: HashSet<u8> = HashSet::new();
        let mut last_flush = now_ms();
        let mut last_dtc = tokio::time::Instant::now();
        while self.current(gen) {
            if last_dtc.elapsed() >= Duration::from_secs(30) {
                let _operation = self.operations.lock().await;
                if !self.current(gen) {
                    return;
                }
                if let Err(e) = self.scan_dtcs().await {
                    self.storage_failure(&e);
                }
                last_dtc = tokio::time::Instant::now();
            }
            let (names, interval, supported) =
                self.with(|i| (i.sampling_pids(), i.interval_ms, i.supported.clone()));
            let defs: Vec<&'static PidDef> = names
                .iter()
                .filter_map(|n| obd::pid_by_name(n))
                .filter(|d| {
                    !dead.contains(&d.pid)
                        && supported.as_ref().map_or(true, |s| s.contains(&d.pid))
                })
                .filter(|d| d.tier == 0 || cycle % 5 == 0)
                .collect();
            if defs.is_empty() {
                if now_ms() - last_flush >= 1000 {
                    if let Err(e) = self.flush_recording() {
                        self.storage_failure(&e);
                    }
                    last_flush = now_ms();
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                cycle += 1;
                continue;
            }
            for def in defs {
                let _operation = self.operations.lock().await;
                if !self.current(gen) {
                    return;
                }
                let (res, raw) = {
                    let mut g = self.elm.lock().await;
                    let Some(e) = g.as_mut() else { return };
                    let r = e.read_pid(def).await;
                    (r, e.take_raw())
                };
                let mut reconnect = false;
                self.with(|i| {
                    for r in raw {
                        i.ring.push_raw(r);
                    }
                    match &res {
                        Ok(v) => {
                            let s = Sample {
                                t_ms: now_ms(),
                                name: def.name.into(),
                                value: *v,
                                unit: def.unit.into(),
                            };
                            i.latest.insert(s.name.clone(), s.clone());
                            if i.pending.len() < 10_000 {
                                i.pending.push(s.clone());
                            } else {
                                i.storage_error = Some(
                                    "Storage backlog full; samples are missing from the archive"
                                        .into(),
                                );
                            }
                            if let Some(r) = i.recording.as_mut() {
                                r.count += 1;
                            }
                            i.ring.push(s);
                        }
                        Err(e) if e.is_link_failure() => reconnect = true,
                        Err(_) => {}
                    }
                });
                match res {
                    Ok(_) => {
                        fails.remove(&def.pid);
                    }
                    Err(ElmError::NoData) => {
                        let n = fails.entry(def.pid).or_insert(0);
                        *n += 1;
                        if *n >= 3 {
                            dead.insert(def.pid);
                        }
                    }
                    Err(_) => {}
                }
                drop(_operation);
                if reconnect && !self.reconnect(gen).await {
                    return;
                }
            }
            if now_ms() - last_flush >= 1000 {
                if let Err(e) = self.flush_recording() {
                    self.storage_failure(&e);
                }
                last_flush = now_ms();
            }
            let values = self.with(|i| {
                i.latest
                    .iter()
                    .map(|(k, s)| {
                        (
                            k.clone(),
                            json!({"v": s.value, "unit": s.unit, "t": s.t_ms}),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>()
            });
            self.emit(json!({"type": "live", "values": values}));
            cycle += 1;
            tokio::time::sleep(Duration::from_millis(interval.max(20))).await;
        }
    }

    /// One automatic reconnect attempt after a link failure. Returns false if the sampler should stop.
    async fn reconnect(self: &Arc<Self>, gen: u64) -> bool {
        let _operation = self.operations.lock().await;
        if !self.current(gen) {
            return false;
        }
        let Some(target_s) = self.with(|i| i.target.clone()) else {
            return false;
        };
        if let Err(e) = self.persist_event("connection_gap", json!({})) {
            self.storage_failure(&e);
        }
        self.emit(json!({"type": "notice", "message": "Adapter connection lost, reconnecting..."}));
        if let Some(mut e) = self.elm.lock().await.take() {
            e.close().await;
        }
        let ok = match Target::parse(&target_s) {
            Ok(t) => match open_and_init(&t).await {
                Ok((mut elm, _)) => {
                    if !self.current(gen) {
                        return false;
                    }
                    let verification: Result<bool> = async {
                        let vin = elm.read_vin().await?;
                        let session = self
                            .with(|i| i.session_id)
                            .ok_or_else(|| anyhow!("no active session"))?;
                        let expected =
                            self.db.lock().unwrap().session(session)?["vehicle_id"].as_i64();
                        let actual = self
                            .db
                            .lock()
                            .unwrap()
                            .resolve_vehicle(&self.identity.fingerprint(&vin)?, &mask_vin(&vin))?;
                        Ok(Some(actual) == expected)
                    }
                    .await;
                    if !matches!(verification, Ok(true)) {
                        elm.close().await;
                        let detail = match verification {
                            Err(e) => format!(" ({e})"),
                            _ => String::new(),
                        };
                        self.with(|i|i.status.error=Some(format!("Reconnect could not verify the same vehicle{detail}; connect again to start a new session")));
                        return self.failed_reconnect();
                    }
                    *self.elm.lock().await = Some(elm);
                    if let Err(e) = self.persist_event("connection_restored", json!({})) {
                        self.storage_failure(&e);
                    }
                    true
                }
                Err(e) => {
                    self.with(|i| i.status.error = Some(format!("Reconnect failed: {e}")));
                    false
                }
            },
            Err(_) => false,
        };
        if !ok {
            return self.failed_reconnect();
        }
        self.emit(json!({"type": "status", "status": self.status()}));
        ok
    }

    fn failed_reconnect(&self) -> bool {
        self.with(|i| i.status.connected = false);
        let result = self.flush_recording().and_then(|_| {
            if let Some(id) = self.with(|i| i.session_id) {
                self.db
                    .lock()
                    .unwrap()
                    .finish_history(id, "reconnect_failed")?;
            }
            self.with(|i| {
                i.session_id = None;
                i.recording = None;
            });
            Ok(())
        });
        if let Err(e) = result {
            self.storage_failure(&e);
        }
        self.emit(json!({"type":"status","status":self.status()}));
        false
    }

    fn storage_failure(&self, e: &anyhow::Error) {
        tracing::error!("history write failed: {e}");
        self.with(|i| i.storage_error = Some(e.to_string()));
        self.emit(json!({"type":"status","status":self.status()}));
    }

    fn persist_event(&self, kind: &str, data: Value) -> Result<i64> {
        let id = self
            .with(|i| i.session_id)
            .ok_or_else(|| anyhow!("no active session"))?;
        self.db.lock().unwrap().event(id, kind, data)
    }

    fn flush_recording(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(id) = inner.session_id {
            if !inner.pending.is_empty() {
                let mut db = self.db.lock().unwrap();
                db.insert(id, &inner.pending)?;
                inner.pending.clear();
                if let Some(error) = inner.storage_error.take() {
                    db.event(
                        id,
                        "storage_gap",
                        json!({"message":error,"note":"Some samples may not have been archived"}),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub async fn start_recording(
        &self,
        label: &str,
        pids: Option<Vec<String>>,
        interval_ms: Option<u64>,
    ) -> Result<Value> {
        let _operation = self.operations.lock().await;
        let (st, running) = self.with(|i| (i.status.clone(), i.recording.is_some()));
        if !st.connected {
            bail!("not connected to an adapter");
        }
        if running {
            bail!("a recording is already running; stop it first");
        }
        if let Some(p) = pids.filter(|p| !p.is_empty()) {
            self.set_sampling(Some(p), interval_ms, None)?;
        } else if interval_ms.is_some() {
            self.set_sampling(None, interval_ms, None)?;
        }
        let label = if label.is_empty() { "recording" } else { label };
        let session = self
            .with(|i| i.session_id)
            .ok_or_else(|| anyhow!("no active session"))?;
        let id = self
            .db
            .lock()
            .unwrap()
            .start_investigation(session, label)?;
        self.persist_event(
            "investigation_start",
            json!({"investigation_id":id,"label":label}),
        )?;
        self.with(|i| {
            i.recording = Some(Rec {
                id,
                label: label.to_string(),
                count: 0,
                started_ms: now_ms(),
            })
        });
        self.emit(json!({"type": "status", "status": self.status()}));
        Ok(
            json!({"recording_id": session,"session_id":session,"investigation_id":id, "label": label, "message": "Named investigation started. Automatic session recording continues independently."}),
        )
    }

    pub async fn stop_recording(&self) -> Result<Value> {
        let _operation = self.operations.lock().await;
        self.flush_recording()?;
        let result=self.with(|i|->Result<Value>{
            let r=i.recording.as_ref().ok_or_else(||anyhow!("no investigation is running"))?;
            self.db.lock().unwrap().stop_investigation(r.id)?;
            let v=json!({"recording_id":i.session_id,"session_id":i.session_id,"investigation_id":r.id,"label":r.label,"samples":r.count,"from_ms":r.started_ms,"to_ms":now_ms(),"seconds":(now_ms()-r.started_ms)/1000});
            i.recording=None;
            Ok(v)
        })?;
        self.persist_event("investigation_stop", result.clone())?;
        self.emit(json!({"type": "status", "status": self.status()}));
        Ok(result)
    }

    pub fn set_sampling(
        &self,
        pids: Option<Vec<String>>,
        interval_ms: Option<u64>,
        auto_record: Option<bool>,
    ) -> Result<Value> {
        if auto_record == Some(false) {
            bail!("session history is always recorded; auto_record=false is no longer supported");
        }
        if let Some(p) = &pids {
            for n in p {
                if obd::pid_by_name(n).is_none() {
                    bail!(
                        "unknown sensor '{n}'. Known sensors: {}",
                        obd::PIDS
                            .iter()
                            .map(|d| d.name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }
        self.with(|i| {
            if let Some(p) = pids.filter(|p| !p.is_empty()) {
                i.pids = Some(p);
            }
            if let Some(ms) = interval_ms {
                i.interval_ms = ms.min(60_000);
            }
            if let Some(a) = auto_record {
                i.auto_record = a;
            }
        });
        if self.with(|i| i.session_id.is_some()) {
            self.persist_event("sampling", self.status()["sampling"].clone())?;
        }
        Ok(
            json!({"sampling": self.status()["sampling"], "auto_record": self.status()["auto_record"]}),
        )
    }

    pub fn set_ring_buffer(&self, seconds: u64) -> Result<Value> {
        if !(10..=3600).contains(&seconds) {
            bail!("ring buffer length must be between 10 and 3600 seconds");
        }
        self.with(|i| i.ring.set_window(seconds));
        Ok(json!({"window_seconds": seconds}))
    }

    pub fn save_diagnostic_log(
        &self,
        duration_s: u64,
        _label: &str,
        include_raw: bool,
    ) -> Result<Value> {
        let (window, samples, raw, dtcs, pids) = self.with(|i| {
            let w = i.ring.window_s();
            let (s, r) = i.ring.last_seconds(duration_s.min(w));
            (w, s, r, i.last_dtcs.clone(), i.sampling_pids())
        });
        if samples.is_empty() && raw.is_empty() {
            bail!("the ring buffer is empty: connect an adapter first and wait a few seconds");
        }
        let dir = self.dir.join("logs");
        std::fs::create_dir_all(&dir)?;
        let timestamp = time::OffsetDateTime::now_utc().format(
            time::macros::format_description!("[year][month][day]-[hour][minute][second]"),
        )?;
        let base = format!("{timestamp}-{}", uuid::Uuid::new_v4());
        let t0 = samples.first().map_or(now_ms(), |s| s.t_ms);
        let mut csv = String::from("timestamp,seconds,sensor,value,unit\n");
        for s in &samples {
            csv.push_str(&format!(
                "{},{:.3},{},{},{}\n",
                iso(s.t_ms),
                (s.t_ms - t0) as f64 / 1000.0,
                s.name,
                s.value,
                s.unit
            ));
        }
        let csv_path = dir.join(format!("{base}.csv"));
        std::fs::write(&csv_path, csv)?;
        let mut files = vec![csv_path.to_string_lossy().to_string()];
        if include_raw {
            let mut log = String::new();
            for r in &raw {
                log.push_str(&format!("{} > {}  < {}\n", iso(r.t_ms), r.cmd, r.resp));
            }
            let p = dir.join(format!("{base}.raw.log"));
            std::fs::write(&p, log)?;
            files.push(p.to_string_lossy().to_string());
        }
        // Adapter errors can contain arbitrary text. Export only structured observations.
        let dtcs = dtcs.map(|scan| {
            let mut out = json!({});
            for kind in ["stored", "pending", "permanent", "readiness"] {
                if let Some(fields) = scan[kind].as_object() {
                    for key in ["status", "codes", "value", "read_at_ms", "last_success_ms"] {
                        if let Some(value) = fields.get(key) {
                            out[kind][key] = value.clone();
                        }
                    }
                }
            }
            out
        });
        let covered = samples.last().map_or(0, |l| (l.t_ms - t0) / 1000);
        let meta = json!({
            "app_version": APP_VERSION, "os": std::env::consts::OS, "saved_at": iso(now_ms()),
            "requested_seconds": duration_s, "seconds_covered": covered, "ring_window_seconds": window,
            "sensors": pids, "sample_count": samples.len(), "last_dtcs": dtcs,
            "privacy": "VIN and adapter identifiers are omitted from metadata. Readings and optional raw logs can still identify a vehicle or person. Review each file before sharing."
        });
        let mp = dir.join(format!("{base}.json"));
        std::fs::write(&mp, serde_json::to_string_pretty(&meta)?)?;
        files.push(mp.to_string_lossy().to_string());
        Ok(
            json!({"files": files, "samples": samples.len(), "seconds_covered": covered,
            "note": if duration_s > window { Some(format!("requested {duration_s}s but the ring buffer holds only {window}s")) } else { None }}),
        )
    }

    async fn with_elm<R>(
        &self,
        f: impl for<'a> FnOnce(&'a mut Elm) -> futures::future::BoxFuture<'a, Result<R, ElmError>>,
    ) -> Result<R> {
        let mut g = self.elm.lock().await;
        let Some(e) = g.as_mut() else {
            bail!("not connected to an adapter. Use list_adapters, then connect (or open the app and press Connect).")
        };
        let r = f(e).await;
        let raw = e.take_raw();
        drop(g);
        self.with(|i| {
            for x in raw {
                i.ring.push_raw(x);
            }
        });
        r.map_err(anyhow::Error::new)
    }

    pub async fn live(&self, names: &[String]) -> Result<Value> {
        let _operation = self.operations.lock().await;
        let names: Vec<String> = if names.is_empty() {
            self.with(|i| i.sampling_pids())
        } else {
            names.to_vec()
        };
        let mut out = serde_json::Map::new();
        for n in names {
            let Some(def) = obd::pid_by_name(&n) else {
                out.insert(n.clone(), json!({"error": "unknown sensor"}));
                continue;
            };
            match self.with_elm(|e| Box::pin(e.read_pid(def))).await {
                Ok(v) => {
                    let sample = Sample {
                        t_ms: now_ms(),
                        name: def.name.into(),
                        value: v,
                        unit: def.unit.into(),
                    };
                    let id = self
                        .with(|i| i.session_id)
                        .ok_or_else(|| anyhow!("no active session"))?;
                    let result = self.db.lock().unwrap().insert_direct(id, &sample);
                    if let Err(e) = result {
                        self.storage_failure(&e);
                        return Err(e);
                    }
                    self.with(|i| {
                        i.latest.insert(def.name.into(), sample.clone());
                        i.ring.push(sample);
                    });
                    out.insert(n, json!({"value": v, "unit": def.unit}));
                }
                Err(e) => {
                    out.insert(n, json!({"error": e.to_string()}));
                }
            }
        }
        Ok(json!({"read_at": iso(now_ms()), "sensors": out}))
    }

    pub async fn supported(&self) -> Result<Value> {
        let _operation = self.operations.lock().await;
        let s = self.with_elm(|e| Box::pin(e.supported_pids())).await?;
        let known: Vec<Value> = obd::PIDS
            .iter()
            .map(|d| json!({"name": d.name, "unit": d.unit, "supported": s.contains(&d.pid)}))
            .collect();
        self.with(|i| i.supported = Some(s.clone()));
        Ok(
            json!({"supported_sensor_names": obd::PIDS.iter().filter(|d| s.contains(&d.pid)).map(|d| d.name).collect::<Vec<_>>(),
            "sensors": known, "advertised_pid_count": s.len(),
            "note": "Advertised support does not guarantee a reading; some clone adapters advertise PIDs the car does not answer."}),
        )
    }

    pub async fn dtcs(&self) -> Result<Value> {
        let _operation = self.operations.lock().await;
        self.scan_dtcs().await
    }

    async fn scan_dtcs(&self) -> Result<Value> {
        let session = self
            .with(|i| i.session_id)
            .ok_or_else(|| anyhow!("no active session"))?;
        let started = now_ms();
        let previous = self.with(|i| i.last_dtcs.clone());
        let mut scan = json!({"started_ms":started});
        {
            let mut guard = self.elm.lock().await;
            let elm = guard.as_mut().ok_or_else(|| anyhow!("not connected"))?;
            for (category, mode) in [("stored", 3), ("pending", 7), ("permanent", 10)] {
                let mut value = observed(elm.read_dtcs(mode).await);
                if value["status"] == "ok" {
                    value["codes"] = value["value"].take();
                }
                scan[category] = value;
            }
            scan["readiness"] = observed(elm.read_readiness().await);
            let raw = elm.take_raw();
            self.with(|i| {
                for r in raw {
                    i.ring.push_raw(r);
                }
            });
        }
        scan["completed_ms"] = json!(now_ms());
        for category in ["stored", "pending", "permanent"] {
            scan[category]["last_success_ms"] = if scan[category]["status"] == "ok" {
                scan[category]["read_at_ms"].clone()
            } else {
                previous
                    .as_ref()
                    .map(|p| p[category]["last_success_ms"].clone())
                    .unwrap_or(Value::Null)
            };
        }
        scan["caution"]=json!("Observation times are not fault onset times. A code is a failed test, not a failed part.");
        let id = self.db.lock().unwrap().dtc_scan(session, &scan)?;
        scan["event_id"] = json!(id);
        self.with(|i| i.last_dtcs = Some(scan.clone()));
        // Retrieve a frame when a newly observed code appears, not on every poll.
        let new_code = ["stored", "pending", "permanent"].iter().any(|k| {
            scan[k]["codes"].as_array().is_some_and(|codes| {
                codes.iter().any(|code| {
                    !previous
                        .as_ref()
                        .and_then(|p| p[k]["codes"].as_array())
                        .is_some_and(|old| old.contains(code))
                })
            })
        });
        if new_code {
            self.capture_freeze_frame().await?;
        }
        self.emit(json!({"type":"status","status":self.status()}));
        Ok(scan)
    }

    pub async fn freeze_frame(&self) -> Result<Value> {
        let _operation = self.operations.lock().await;
        self.capture_freeze_frame().await
    }

    async fn capture_freeze_frame(&self) -> Result<Value> {
        let dtc = match self.with_elm(|e| Box::pin(e.freeze_frame_dtc())).await {
            Ok(dtc) => dtc,
            Err(e) => {
                let value = json!({"status":freeze_frame_error_status(&e),"error":e.to_string(),"read_at_ms":now_ms()});
                self.persist_event("freeze_frame", value.clone())?;
                return Ok(value);
            }
        };
        let mut vals = serde_json::Map::new();
        if dtc.is_some() {
            for n in DEFAULT_PIDS {
                let def = obd::pid_by_name(n).unwrap();
                match self.with_elm(|e| Box::pin(e.freeze_frame_pid(def))).await {
                    Ok(v) => {
                        vals.insert(n.to_string(), json!({"value": v, "unit": def.unit}));
                    }
                    Err(e) => {
                        vals.insert(
                            n.to_string(),
                            json!({"status":freeze_frame_error_status(&e),"error":e.to_string()}),
                        );
                    }
                }
            }
        }
        let value = json!({"status":if dtc.is_some(){"available"}else{"unavailable"},"read_at_ms":now_ms(),"dtc_that_set_freeze_frame": dtc, "values": vals,
            "note": "ECU freeze frame, not current readings. Exact onset time may be unknown and not every code has a frame."});
        self.persist_event("freeze_frame", value.clone())?;
        Ok(value)
    }

    pub async fn readiness(&self) -> Result<Value> {
        let _operation = self.operations.lock().await;
        Ok(serde_json::to_value(
            self.with_elm(|e| Box::pin(e.read_readiness())).await?,
        )?)
    }

    pub async fn vin(&self, full: bool) -> Result<Value> {
        let _operation = self.operations.lock().await;
        let v = self.with_elm(|e| Box::pin(e.read_vin())).await?;
        if full {
            Ok(json!({"vin": v}))
        } else {
            Ok(
                json!({"vin_masked": mask_vin(&v), "note": "Only the first 8 characters are shown. Full VIN is never stored or logged."}),
            )
        }
    }

    pub async fn clear_dtcs(&self, approved_session: i64) -> Result<Value> {
        let _operation = self.operations.lock().await;
        if self.with(|i| i.session_id) != Some(approved_session) {
            bail!("vehicle session changed; obtain a new confirmation");
        }
        let scan = self.scan_dtcs().await?;
        ensure_clear_evidence(&scan)?;
        let freeze_frame = self.capture_freeze_frame().await?;
        ensure_clear_evidence(&freeze_frame)?;
        self.flush_recording()?;
        let backup = self.save_diagnostic_log(300, "before-clear", true)?;
        let attempt = self.persist_event("clear_attempt", json!({"backup":backup}))?;
        let result = self.with_elm(|e| Box::pin(e.clear_dtcs())).await;
        let error = result.as_ref().err().map(|e| e.to_string());
        self.persist_event("clear_result",json!({"attempt_event_id":attempt,"status":if result.is_ok(){"acknowledged"}else{"unknown_or_failed"},"error":error}))?;
        let after = self.scan_dtcs().await?;
        result?;
        Ok(
            json!({"cleared": true, "backup_log_saved_before_clearing": backup["files"],"after":after,
            "warning": "Freeze-frame data was erased and readiness monitors were reset; the car needs a drive cycle to complete them."}),
        )
    }

    pub fn list_recordings(&self) -> Result<Value> {
        self.db.lock().unwrap().list()
    }

    pub fn recording_summary(&self, id: i64) -> Result<Value> {
        self.flush_recording()?;
        self.db.lock().unwrap().summary(id)
    }

    pub fn recording_query(
        &self,
        id: i64,
        pids: &[String],
        from_s: f64,
        to_s: Option<f64>,
        max_points: usize,
    ) -> Result<Value> {
        self.flush_recording()?;
        self.db
            .lock()
            .unwrap()
            .query(id, pids, from_s, to_s, max_points)
    }

    pub fn recording_compare(&self, a: i64, b: i64) -> Result<Value> {
        self.flush_recording()?;
        self.db.lock().unwrap().compare(a, b)
    }

    pub fn recording_export(&self, id: i64) -> Result<Value> {
        self.flush_recording()?;
        let dir = self.dir.join("exports");
        std::fs::create_dir_all(&dir)?;
        let p = dir.join(format!("recording-{id}.csv"));
        let n = self.db.lock().unwrap().export_csv(id, &p)?;
        Ok(json!({"file": p.to_string_lossy(), "rows": n}))
    }

    pub async fn live_snapshot(&self) -> Result<Value> {
        let (connected, latest) = self.with(|i| (i.status.connected, i.latest.clone()));
        if !connected {
            bail!("not connected to an adapter");
        }
        let now = now_ms();
        let mut m = serde_json::Map::new();
        for (k, s) in latest {
            m.insert(
                k,
                json!({"value": s.value, "unit": s.unit, "age_ms": now - s.t_ms}),
            );
        }
        Ok(json!({"sensors": m}))
    }

    pub async fn list_adapters(&self, scan_ble: bool) -> Value {
        let (list, warnings) = transport::list_adapters(scan_ble).await;
        json!({"adapters": list, "warnings": warnings})
    }

    pub fn history(&self, name: &str, args: &Value, ai: bool) -> Result<Value> {
        crate::history::validate(args)?;
        if ai && name == "assign_session_vehicle" {
            bail!("select the vehicle in the local app; AI cannot assign unidentified history to a car");
        }
        self.flush_recording()?;
        let result = {
            let mut db = self.db.lock().unwrap();
            match name {
                "list_sessions" => db.sessions(args),
                "get_session" => db.session(crate::history::integer(args, "session_id")?),
                "query_session" => db.query_samples(args),
                "get_session_events" => db.events(args),
                "list_vehicles" => db.vehicles(),
                "assign_session_vehicle" => db.assign_vehicle(
                    crate::history::integer(args, "session_id")?,
                    args["vehicle_id"].as_i64(),
                    args["name"].as_str().unwrap_or(""),
                ),
                "list_fault_annotations" => {
                    db.annotations(crate::history::integer(args, "session_id")?)
                }
                "save_fault_annotation" => db.annotate(args, if ai { "ai" } else { "user" }),
                "find_similar_conditions" => db.conditions(args),
                _ => bail!("unknown history tool"),
            }
        };
        if result.is_ok() && matches!(name, "assign_session_vehicle" | "save_fault_annotation") {
            self.emit(json!({"type":"history_changed"}));
        }
        result
    }
}

fn freeze_frame_error_status(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<ElmError>() {
        Some(ElmError::NoData | ElmError::Unsupported) => "unavailable",
        Some(_) | None => "error",
    }
}

fn observed<T: Serialize>(result: Result<T, ElmError>) -> Value {
    match result {
        Ok(v) => json!({"status":"ok","value":v,"read_at_ms":now_ms()}),
        Err(e) => {
            json!({"status":match e {ElmError::NoData=>"no_data",ElmError::Unsupported=>"unsupported",_=>"error"},"error":e.to_string(),"read_at_ms":now_ms()})
        }
    }
}

fn ensure_clear_evidence(evidence: &Value) -> Result<()> {
    let failed = |value: &Value| value["status"] == "error";
    if failed(evidence)
        || ["stored", "pending", "permanent", "readiness"]
            .iter()
            .any(|key| failed(&evidence[key]))
        || evidence["values"]
            .as_object()
            .is_some_and(|values| values.values().any(failed))
    {
        bail!("cannot clear DTCs because pre-clear diagnostic evidence could not be read");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub() -> (Arc<Hub>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("obd-history-{}", uuid::Uuid::new_v4()));
        let hub = Hub::new(dir.clone(), Db::open(&dir.join("recordings.db")).unwrap()).unwrap();
        (hub, dir)
    }

    #[tokio::test(start_paused = true)]
    async fn automatic_sessions_poll_new_codes_without_ai_and_keep_clear_history() {
        let (h, dir) = hub();
        h.connect("mock").await.unwrap();
        let sid = h.status()["session_id"].as_i64().unwrap();
        assert!(
            h.status()["recording"].is_null(),
            "no manual recording required"
        );
        let count = || {
            h.history("get_session_events", &json!({"session_id":sid}), false)
                .unwrap()["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v["kind"] == "dtc_scan")
                .count()
        };
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
        assert_eq!(count(), 1);
        tokio::time::advance(Duration::from_secs(29)).await;
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
        assert_eq!(count(), 1);
        let mut replacement = Elm::new(Box::new(
            transport::mock::MockTransport::new("lean", &[]).unwrap(),
        ));
        replacement.init().await.unwrap();
        *h.elm.lock().await = Some(replacement);
        tokio::time::advance(Duration::from_secs(2)).await;
        for _ in 0..200 {
            tokio::time::advance(Duration::from_millis(5)).await;
            tokio::task::yield_now().await;
            if count() == 2 {
                break;
            }
        }
        assert_eq!(count(), 2);
        assert_eq!(h.status()["last_dtc_scan"]["stored"]["codes"][0], "P0171");
        assert!(h.clear_dtcs(sid + 1).await.is_err());
        let result = h.clear_dtcs(sid).await.unwrap();
        assert_eq!(result["after"]["stored"]["codes"], json!([]));
        let session = h
            .history("get_session", &json!({"session_id":sid}), false)
            .unwrap();
        assert!(session["dtc_history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "P0171"));
        let events = h
            .history("get_session_events", &json!({"session_id":sid}), false)
            .unwrap();
        assert!(events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "clear_result"));
        assert_eq!(
            h.history("list_fault_annotations", &json!({"session_id":sid}), false)
                .unwrap(),
            json!([])
        );
        h.disconnect().await.unwrap();
        let reopened = Db::open(&dir.join("recordings.db")).unwrap();
        assert_eq!(reopened.session(sid).unwrap()["state"], "closed");
        assert!(!crate::history::flat_samples(
            &reopened.query_samples(&json!({"session_id":sid})).unwrap()
        )
        .is_empty());
        drop(reopened);
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn storage_failure_preserves_pending_batch_and_blocks_clear() {
        let (h, dir) = hub();
        h.connect("mock").await.unwrap();
        let sid = h.status()["session_id"].as_i64().unwrap();
        h.with(|i| {
            i.pending.push(Sample {
                t_ms: now_ms(),
                name: "rpm".into(),
                value: 750.0,
                unit: "rpm".into(),
            })
        });
        h.db.lock().unwrap().conn.execute_batch("CREATE TRIGGER no_samples BEFORE INSERT ON samples BEGIN SELECT RAISE(FAIL,'disk write failed'); END;").unwrap();
        let error = h.flush_recording().unwrap_err();
        h.storage_failure(&error);
        assert!(!h.with(|i| i.pending.is_empty()));
        assert!(h.status()["storage_error"]
            .as_str()
            .unwrap()
            .contains("disk write failed"));
        assert!(h.clear_dtcs(sid).await.is_err());
        h.db.lock().unwrap().conn.execute_batch("DROP TRIGGER no_samples; CREATE TRIGGER no_clear_archive BEFORE INSERT ON events WHEN NEW.kind='clear_attempt' BEGIN SELECT RAISE(FAIL,'archive failed'); END;").unwrap();
        assert!(h.clear_dtcs(sid).await.is_err());
        assert_eq!(
            h.dtcs().await.unwrap()["stored"]["codes"][0],
            "P0420",
            "no clear command may run after archive failure"
        );
        h.db.lock()
            .unwrap()
            .conn
            .execute_batch("DROP TRIGGER no_clear_archive;")
            .unwrap();
        h.flush_recording().unwrap();
        assert!(h.with(|i| i.pending.is_empty()));
        let events = h
            .history("get_session_events", &json!({"session_id":sid}), false)
            .unwrap();
        assert!(events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "storage_gap"));
        h.disconnect().await.unwrap();
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn empty_sampler_cycles_flush_buffered_samples() {
        let (h, dir) = hub();
        h.connect("mock").await.unwrap();
        let sid = h.status()["session_id"].as_i64().unwrap();
        h.with(|i| {
            i.supported = Some(BTreeSet::new());
            i.pending.push(Sample {
                t_ms: now_ms(),
                name: "rpm".into(),
                value: 750.0,
                unit: "rpm".into(),
            });
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if h.with(|i| i.pending.is_empty()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        let samples = h
            .history(
                "query_session",
                &json!({"session_id":sid,"pids":["rpm"]}),
                false,
            )
            .unwrap();
        assert_eq!(crate::history::flat_samples(&samples).len(), 1);
        h.disconnect().await.unwrap();
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn manual_segment_stop_does_not_close_session_and_vin_stays_private() {
        let (h, dir) = hub();
        h.connect("mock").await.unwrap();
        let first = h.status()["session_id"].as_i64().unwrap();
        let started = h.start_recording("idle", None, None).await.unwrap();
        assert_eq!(started["session_id"], first);
        h.live(&["RPM".into()]).await.unwrap();
        let samples = h
            .history(
                "query_session",
                &json!({"session_id":first,"pids":["rpm"]}),
                false,
            )
            .unwrap();
        assert!(crate::history::flat_samples(&samples)
            .iter()
            .any(|sample| sample["name"] == "rpm"));
        h.stop_recording().await.unwrap();
        h.live(&["rpm".into()]).await.unwrap();
        assert_eq!(
            h.history("get_session", &json!({"session_id":first}), false)
                .unwrap()["state"],
            "active"
        );
        let car = h
            .history("get_session", &json!({"session_id":first}), false)
            .unwrap()["vehicle_id"]
            .clone();
        h.disconnect().await.unwrap();
        h.connect("mock").await.unwrap();
        let second = h.status()["session_id"].as_i64().unwrap();
        assert_ne!(first, second);
        assert_eq!(
            h.history("get_session", &json!({"session_id":second}), false)
                .unwrap()["vehicle_id"],
            car
        );
        h.with(|i| {
            i.status.adapter = transport::mock::MOCK_VIN.into();
            i.last_dtcs =
                Some(json!({"pending":{"status":"error","error":transport::mock::MOCK_VIN}}));
        });
        let log = h
            .save_diagnostic_log(300, transport::mock::MOCK_VIN, true)
            .unwrap();
        for f in log["files"].as_array().unwrap() {
            assert!(!f.as_str().unwrap().contains(transport::mock::MOCK_VIN));
            let data = std::fs::read_to_string(f.as_str().unwrap()).unwrap();
            assert!(!data.contains(transport::mock::MOCK_VIN));
            assert!(!data.contains("vin_hmac"));
        }
        let all = h
            .history("list_sessions", &json!({}), false)
            .unwrap()
            .to_string();
        assert!(!all.contains(transport::mock::MOCK_VIN) && !all.contains("vin_hmac"));
        h.disconnect().await.unwrap();
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_category_is_never_an_empty_success() {
        let failure = observed::<Vec<String>>(Err(ElmError::NoData));
        assert_eq!(failure["status"], "no_data");
        assert!(failure.get("codes").is_none());
        assert_ne!(failure, observed(Ok(Vec::<String>::new())));
    }

    #[test]
    fn clear_requires_successful_or_explicitly_unavailable_evidence() {
        let unavailable = json!({"stored":{"status":"unavailable"},"pending":{"status":"ok"},"permanent":{"status":"unavailable"},"readiness":{"status":"unavailable"}});
        assert!(ensure_clear_evidence(&unavailable).is_ok());
        let failed = json!({"stored":{"status":"error"},"pending":{"status":"ok"},"permanent":{"status":"ok"},"readiness":{"status":"ok"}});
        assert!(ensure_clear_evidence(&failed).is_err());
        let failed_frame = json!({"status":"available","values":{"rpm":{"status":"error"}}});
        assert!(ensure_clear_evidence(&failed_frame).is_err());
        let unavailable_frame =
            json!({"status":"available","values":{"rpm":{"status":"unavailable"}}});
        assert!(ensure_clear_evidence(&unavailable_frame).is_ok());
        assert_eq!(
            freeze_frame_error_status(&anyhow!(ElmError::Timeout)),
            "error"
        );
        assert_eq!(
            freeze_frame_error_status(&anyhow!(ElmError::NoData)),
            "unavailable"
        );
    }

    #[tokio::test]
    async fn missing_vin_needs_explicit_assignment_and_reconnect_gaps_are_saved() {
        let (h, dir) = hub();
        h.connect("mock:healthy:no_vin").await.unwrap();
        let sid = h.status()["session_id"].as_i64().unwrap();
        assert!(h
            .history("get_session", &json!({"session_id":sid}), false)
            .unwrap()["vehicle_id"]
            .is_null());
        let assigned = h
            .history(
                "assign_session_vehicle",
                &json!({"session_id":sid,"name":"My car"}),
                false,
            )
            .unwrap();
        assert!(assigned["vehicle_id"].is_i64());
        assert!(h
            .history(
                "list_sessions",
                &json!({"vehicle_id":"not an integer"}),
                false
            )
            .is_err());
        h.disconnect().await.unwrap();
        h.connect("mock").await.unwrap();
        let active = h.status()["session_id"].as_i64().unwrap();
        let generation = h.with(|i| i.sampler_gen);
        assert!(h.reconnect(generation).await);
        assert_eq!(h.status()["session_id"], active);
        h.with(|i| i.target = Some("mock:unknown".into()));
        assert!(!h.reconnect(generation).await);
        assert_eq!(
            h.history("get_session", &json!({"session_id":active}), false)
                .unwrap()["end_reason"],
            "reconnect_failed"
        );
        let events = h
            .history("get_session_events", &json!({"session_id":active}), false)
            .unwrap();
        assert!(events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "connection_restored"));
        h.disconnect().await.unwrap();
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    fn hub() -> (Arc<Hub>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("obdtrace-hub-{}", uuid::Uuid::new_v4()));
        let hub = Hub::new(dir.clone(), Db::memory().unwrap()).unwrap();
        (hub, dir)
    }

    #[tokio::test]
    async fn concurrent_connections_leave_the_latest_request_active() {
        let (h, dir) = hub();
        let first = tokio::spawn({
            let h = h.clone();
            async move { h.connect("mock:catalyst:slow_boot").await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        h.connect("mock:healthy").await.unwrap();
        let _ = first.await.unwrap();
        assert_eq!(h.status()["target"], "mock:healthy");
        assert!(h.status()["adapter"]["connected"].as_bool().unwrap());
        assert!(h.connect("rfcomm:invalid-address").await.is_err());
        assert_eq!(h.status()["target"], "mock:healthy");
        assert_eq!(h.status()["adapter"]["connected"], true);
        h.disconnect().await.unwrap();
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn explicit_connection_resets_vehicle_caches_and_recording_stop_flushes() {
        let (h, dir) = hub();
        h.connect("mock:catalyst").await.unwrap();
        h.dtcs().await.unwrap();
        h.with(|i| {
            i.ring.push(Sample {
                t_ms: now_ms(),
                name: "old".into(),
                value: 1.0,
                unit: "".into(),
            })
        });
        h.connect("mock:healthy").await.unwrap();
        assert!(h.with(
            |i| i.last_dtcs.as_ref().unwrap()["stored"]["codes"] == json!([])
                && i.ring.last_seconds(300).0.iter().all(|s| s.name != "old")
        ));

        let start = h.start_recording("test", None, None).await.unwrap();
        let id = start["recording_id"].as_i64().unwrap();
        h.with(|i| {
            let sample = Sample {
                t_ms: now_ms(),
                name: "rpm".into(),
                value: 700.0,
                unit: "rpm".into(),
            };
            i.pending.push(sample);
            i.recording.as_mut().unwrap().count += 1;
        });
        assert!(
            h.stop_recording().await.unwrap()["samples"]
                .as_u64()
                .unwrap()
                >= 1
        );
        assert!(
            h.recording_summary(id).unwrap()["sensors"]["rpm"]["count"]
                .as_i64()
                .unwrap()
                >= 1
        );
        h.disconnect().await.unwrap();
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn clear_aborts_when_backup_cannot_be_written() {
        let (h, dir) = hub();
        h.connect("mock:catalyst").await.unwrap();
        std::fs::create_dir_all(&h.dir).unwrap();
        std::fs::write(h.dir.join("logs"), "not a directory").unwrap();
        assert!(h
            .clear_dtcs(h.status()["session_id"].as_i64().unwrap())
            .await
            .is_err());
        assert_eq!(h.dtcs().await.unwrap()["stored"]["codes"][0], "P0420");
        h.disconnect().await.unwrap();
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn identical_log_saves_use_distinct_bundles() {
        let (h, dir) = hub();
        h.connect("mock:catalyst").await.unwrap();
        let first = h.save_diagnostic_log(300, "same", true).unwrap();
        let second = h.save_diagnostic_log(300, "same", true).unwrap();
        assert_ne!(first["files"][0], second["files"][0]);
        assert_ne!(first["files"][1], second["files"][1]);
        assert_ne!(first["files"][2], second["files"][2]);
        h.disconnect().await.unwrap();
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
