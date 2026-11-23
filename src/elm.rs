//! ELM327 protocol layer: init sequence, command/response handling and OBD-II queries.
//! Hardened against the common misbehaviour of cheap clones (stuck echo, v1.5
//! firmware without AT commands, slow boot, auto-protocol search failures,
//! dropped replies, wrong "supported PID" bitmaps, multi-frame replies).

use crate::obd::{self, PidDef};
use crate::redact;
use crate::transport::Transport;
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub enum ElmError {
    NoData,
    UnableToConnect,
    BusInit,
    CanError,
    DataError,
    BufferFull,
    Stopped,
    Unsupported,
    Timeout,
    Negative(u8),
    Malformed(String),
    Disconnected(String),
}

impl fmt::Display for ElmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ElmError::NoData => write!(f, "NO DATA: the vehicle did not answer this request (unsupported or ignition off)"),
            ElmError::UnableToConnect => write!(f, "UNABLE TO CONNECT: the adapter cannot talk to the car. Turn the ignition ON and check the adapter is firmly in the OBD port"),
            ElmError::BusInit => write!(f, "BUS INIT ERROR: bus initialisation failed"),
            ElmError::CanError => write!(f, "CAN ERROR: CAN bus error or wrong protocol"),
            ElmError::DataError => write!(f, "DATA ERROR: corrupted reply from the vehicle"),
            ElmError::BufferFull => write!(f, "BUFFER FULL: adapter buffer overflow"),
            ElmError::Stopped => write!(f, "STOPPED: the request was interrupted"),
            ElmError::Unsupported => write!(f, "the adapter does not support this command"),
            ElmError::Timeout => write!(f, "timed out waiting for the adapter"),
            ElmError::Negative(c) => write!(f, "negative response from the vehicle (code {c:02X})"),
            ElmError::Malformed(s) => write!(f, "unexpected reply: {s}"),
            ElmError::Disconnected(s) => write!(f, "adapter connection lost: {s}"),
        }
    }
}

impl std::error::Error for ElmError {}

impl ElmError {
    /// True when the link itself is probably gone (as opposed to a per-request vehicle answer).
    pub fn is_link_failure(&self) -> bool {
        matches!(self, ElmError::Disconnected(_) | ElmError::Timeout)
    }
}

#[derive(Debug, Clone)]
pub struct Timings {
    pub atz: Duration,
    pub normal: Duration,
    pub probe: Duration,
    pub fallback_probe: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self { atz: Duration::from_secs(4), normal: Duration::from_millis(2500), probe: Duration::from_secs(12), fallback_probe: Duration::from_secs(6) }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RawEntry {
    pub t_ms: i64,
    pub cmd: String,
    pub resp: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct InitReport {
    pub version: String,
    pub protocol: String,
    pub voltage: Option<String>,
    pub unsupported_commands: Vec<String>,
    pub notes: Vec<String>,
}

pub struct Elm {
    t: Box<dyn Transport>,
    pub timings: Timings,
    pub unsupported: HashSet<String>,
    pub dead_pids: HashSet<u8>,
    pub raw: Vec<RawEntry>,
    dtc_count_prefix: bool,
}

fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase()
}

fn classify(line: &str) -> Option<ElmError> {
    let u = line.trim().to_uppercase();
    let u = u.trim_start_matches('<');
    Some(match u {
        "NO DATA" => ElmError::NoData,
        "UNABLE TO CONNECT" => ElmError::UnableToConnect,
        "BUS INIT: ...ERROR" | "BUS INIT:...ERROR" | "BUS ERROR" | "BUS BUSY" | "FB ERROR" => ElmError::BusInit,
        "CAN ERROR" => ElmError::CanError,
        "DATA ERROR" | "LV RESET" => ElmError::DataError,
        "BUFFER FULL" => ElmError::BufferFull,
        "STOPPED" | "ACT ALERT" => ElmError::Stopped,
        "?" => ElmError::Unsupported,
        _ if u.starts_with("ERR") && u.len() <= 6 => ElmError::DataError,
        _ => return None,
    })
}

impl Elm {
    pub fn new(t: Box<dyn Transport>) -> Self {
        Self { t, timings: Timings::default(), unsupported: HashSet::new(), dead_pids: HashSet::new(), raw: Vec::new(), dtc_count_prefix: false }
    }

    pub fn describe(&self) -> String {
        self.t.describe()
    }

    pub async fn close(&mut self) {
        self.t.close().await;
    }

    pub fn take_raw(&mut self) -> Vec<RawEntry> {
        std::mem::take(&mut self.raw)
    }

    /// Send one command and collect the reply up to the `>` prompt. Echo and
    /// SEARCHING... lines are removed; adapter status lines become errors.
    pub async fn command(&mut self, cmd: &str, timeout: Duration) -> Result<Vec<String>, ElmError> {
        while let Ok(Some(_)) = self.t.read_chunk(Duration::from_millis(1)).await {}
        let line = format!("{cmd}\r");
        self.t.write(line.as_bytes()).await.map_err(|e| ElmError::Disconnected(e.to_string()))?;
        let deadline = Instant::now() + timeout;
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.log_raw(cmd, &String::from_utf8_lossy(&buf));
                return Err(ElmError::Timeout);
            }
            match self.t.read_chunk(left.min(Duration::from_millis(500))).await {
                Ok(Some(c)) => {
                    buf.extend(c.into_iter().filter(|b| *b == b'\r' || *b == b'\n' || (0x20..0x7f).contains(b)));
                    if buf.contains(&b'>') {
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => return Err(ElmError::Disconnected(e.to_string())),
            }
        }
        let text = String::from_utf8_lossy(&buf).to_string();
        self.log_raw(cmd, &text);
        let echo = strip_ws(cmd);
        let mut lines = Vec::new();
        for l in text.split(['\r', '\n']) {
            let l = l.trim().trim_end_matches('>').trim();
            if l.is_empty() || strip_ws(l) == echo || l.to_uppercase().starts_with("SEARCHING") {
                continue;
            }
            lines.push(l.to_string());
        }
        for l in &lines {
            if let Some(e) = classify(l) {
                if e == ElmError::Unsupported {
                    self.unsupported.insert(strip_ws(cmd));
                }
                return Err(e);
            }
        }
        Ok(lines)
    }

    fn log_raw(&mut self, cmd: &str, resp: &str) {
        let resp = redact_reply(cmd, &resp.replace('\r', " | "));
        self.raw.push(RawEntry { t_ms: crate::store::now_ms(), cmd: cmd.to_string(), resp });
        if self.raw.len() > 4000 {
            self.raw.drain(..2000);
        }
    }

    async fn soft(&mut self, cmd: &str, report: &mut InitReport) {
        for _ in 0..2 {
            match self.command(cmd, self.timings.normal).await {
                Ok(_) => return,
                Err(ElmError::Unsupported) => {
                    report.unsupported_commands.push(cmd.to_string());
                    return;
                }
                Err(ElmError::Disconnected(_)) => return,
                Err(_) => {}
            }
        }
    }

    pub async fn init(&mut self) -> Result<InitReport, ElmError> {
        let mut report = InitReport::default();
        let mut version = None;
        for attempt in 0..3 {
            match self.command("ATZ", self.timings.atz * (attempt + 1)).await {
                Ok(lines) => {
                    version = lines.iter().find(|l| l.to_uppercase().contains("ELM") || l.to_uppercase().contains("OBD")).cloned().or_else(|| lines.first().cloned());
                    break;
                }
                Err(e @ ElmError::Disconnected(_)) => return Err(e),
                Err(_) => {
                    report.notes.push("adapter was slow to answer ATZ, retrying".into());
                    let _ = self.t.write(b"\r").await;
                }
            }
        }
        report.version = version.ok_or_else(|| ElmError::Malformed("adapter does not answer (is it an ELM327 adapter? is another app connected to it?)".into()))?;
        for cmd in ["ATE0", "ATL0", "ATS0", "ATH0", "ATAT1"] {
            self.soft(cmd, &mut report).await;
        }
        let mut connected = false;
        self.soft("ATSP0", &mut report).await;
        match self.query_raw("0100", self.timings.probe).await {
            Ok(_) => connected = true,
            Err(e @ ElmError::Disconnected(_)) => return Err(e),
            Err(_) => report.notes.push("automatic protocol search failed, trying protocols one by one".into()),
        }
        if !connected {
            for sp in ["ATSP6", "ATSP7", "ATSP8", "ATSP9", "ATSP5", "ATSP4", "ATSP3", "ATSP2", "ATSP1"] {
                if self.command(sp, self.timings.normal).await.is_err() {
                    continue;
                }
                if self.query_raw("0100", self.timings.fallback_probe).await.is_ok() {
                    connected = true;
                    break;
                }
            }
        }
        if !connected {
            return Err(ElmError::UnableToConnect);
        }
        report.protocol = self.protocol().await;
        self.dtc_count_prefix = report.protocol.contains("CAN");
        report.voltage = self.command("ATRV", self.timings.normal).await.ok().and_then(|l| l.first().cloned());
        report.unsupported_commands = self.unsupported.iter().cloned().collect();
        report.unsupported_commands.sort();
        Ok(report)
    }

    async fn protocol(&mut self) -> String {
        let names = ["", "SAE J1850 PWM", "SAE J1850 VPW", "ISO 9141-2", "ISO 14230-4 KWP (5 baud)", "ISO 14230-4 KWP (fast)", "ISO 15765-4 CAN (11 bit, 500 kbaud)", "ISO 15765-4 CAN (29 bit, 500 kbaud)", "ISO 15765-4 CAN (11 bit, 250 kbaud)", "ISO 15765-4 CAN (29 bit, 250 kbaud)", "SAE J1939 CAN"];
        match self.command("ATDPN", self.timings.normal).await {
            Ok(l) => {
                let s = l.first().cloned().unwrap_or_default();
                let n = s.trim_start_matches('A');
                n.chars().next().and_then(|c| c.to_digit(16)).and_then(|d| names.get(d as usize)).map(|n| n.to_string()).unwrap_or(s)
            }
            Err(_) => match self.command("ATDP", self.timings.normal).await {
                Ok(l) => l.first().cloned().unwrap_or_else(|| "unknown".into()),
                Err(_) => "unknown".into(),
            },
        }
    }

    /// Run an OBD request and return the unique reply frames (starting with the response mode byte).
    pub async fn query_raw(&mut self, req: &str, timeout: Duration) -> Result<Vec<Vec<u8>>, ElmError> {
        let lines = self.command(req, timeout).await?;
        let want = u8::from_str_radix(&strip_ws(req)[..2], 16).map_err(|_| ElmError::Malformed(req.into()))? + 0x40;
        let want_pid = if matches!(want, 0x41 | 0x42 | 0x49) { u8::from_str_radix(&strip_ws(req)[2..4], 16).ok() } else { None };
        let frames = assemble_frames(&lines);
        let mut good: Vec<Vec<u8>> = Vec::new();
        for f in &frames {
            let ok = f.first() == Some(&want) && want_pid.map_or(true, |p| f.get(1) == Some(&p));
            if ok && !good.contains(f) {
                good.push(f.clone());
            }
        }
        if good.is_empty() {
            if let Some(f) = frames.iter().find(|f| f.first() == Some(&0x7F)) {
                return Err(ElmError::Negative(f.get(2).copied().unwrap_or(0)));
            }
            return Err(ElmError::Malformed(redact_reply(req, &lines.join(" | "))));
        }
        Ok(good)
    }

    async fn query(&mut self, req: &str) -> Result<Vec<Vec<u8>>, ElmError> {
        self.query_raw(req, self.timings.normal).await
    }

    pub async fn read_pid(&mut self, def: &PidDef) -> Result<f64, ElmError> {
        let frames = self.query(&format!("01{:02X}", def.pid)).await?;
        obd::decode_pid(def, &frames[0][2..]).ok_or_else(|| ElmError::Malformed(format!("short reply for {}", def.name)))
    }

    pub async fn read_dtcs(&mut self, mode: u8) -> Result<Vec<String>, ElmError> {
        let frames = self.query(&format!("{mode:02X}")).await?;
        let mut out: Vec<String> = Vec::new();
        for f in frames {
            for c in obd::decode_dtc_payload(&f[1..], self.dtc_count_prefix) {
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        Ok(out)
    }

    pub async fn clear_dtcs(&mut self) -> Result<(), ElmError> {
        self.query("04").await.map(|_| ())
    }

    pub async fn read_vin(&mut self) -> Result<String, ElmError> {
        let frames = self.query("0902").await?;
        let mut sorted = frames.clone();
        sorted.sort_by_key(|f| f.get(2).copied().unwrap_or(0));
        let bytes: Vec<u8> = if sorted.len() == 1 { sorted[0][3.min(sorted[0].len())..].to_vec() } else { sorted.iter().flat_map(|f| f[3.min(f.len())..].to_vec()).collect() };
        let vin: String = bytes.into_iter().filter(|b| b.is_ascii_alphanumeric()).map(|b| b as char).collect();
        let vin = if vin.len() > 17 { vin[..17].to_string() } else { vin };
        if redact::is_plausible_vin(&vin) {
            Ok(vin)
        } else {
            Err(ElmError::Malformed("VIN reply did not contain a valid 17-character VIN".into()))
        }
    }

    pub async fn read_readiness(&mut self) -> Result<obd::Readiness, ElmError> {
        let frames = self.query("0101").await?;
        obd::decode_readiness(&frames[0][2..]).ok_or_else(|| ElmError::Malformed("short readiness reply".into()))
    }

    pub async fn freeze_frame_dtc(&mut self) -> Result<Option<String>, ElmError> {
        match self.query("020200").await {
            Ok(f) => Ok(f[0].get(3..5).filter(|d| d[0] != 0 || d[1] != 0).map(|d| obd::decode_dtc(d[0], d[1]))),
            Err(ElmError::NoData) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn freeze_frame_pid(&mut self, def: &PidDef) -> Result<f64, ElmError> {
        let f = self.query(&format!("02{:02X}00", def.pid)).await?;
        obd::decode_pid(def, f[0].get(3..).unwrap_or(&[])).ok_or_else(|| ElmError::Malformed(format!("short freeze-frame reply for {}", def.name)))
    }

    /// Discover supported Mode 01 PIDs, following continuation flags.
    pub async fn supported_pids(&mut self) -> Result<BTreeSet<u8>, ElmError> {
        let mut out = BTreeSet::new();
        let mut base = 0u8;
        loop {
            let frames = match self.query(&format!("01{base:02X}")).await {
                Ok(f) => f,
                Err(ElmError::NoData) if base != 0 => break,
                Err(e) => return Err(e),
            };
            let (pids, cont) = decode_supported_frames(base, &frames);
            out.extend(pids);
            if !cont || base >= 0x60 {
                break;
            }
            base += 0x20;
        }
        Ok(out)
    }

    pub async fn voltage(&mut self) -> Option<String> {
        self.command("ATRV", self.timings.normal).await.ok().and_then(|l| l.first().cloned())
    }
}

fn redact_reply(cmd: &str, resp: &str) -> String {
    if strip_ws(cmd).starts_with("0902") {
        "[VIN reply redacted]".to_string()
    } else {
        redact::scrub_text(resp)
    }
}

fn decode_supported_frames(base: u8, frames: &[Vec<u8>]) -> (BTreeSet<u8>, bool) {
    let mut pids = BTreeSet::new();
    let mut continuation = false;
    for frame in frames {
        if let Some(bitmap) = frame.get(2..6) {
            let (supported, next) = obd::decode_supported(base, bitmap);
            pids.extend(supported);
            continuation |= next;
        }
    }
    (pids, continuation)
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    let h = strip_ws(s);
    if h.is_empty() || h.len() % 2 == 1 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..h.len() / 2).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()).collect()
}

/// Turn reply lines into data frames, handling ELM multi-line CAN replies
/// (`014` length line + `0:`/`1:` numbered lines) and plain per-ECU lines.
pub fn assemble_frames(lines: &[String]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim();
        let compact = strip_ws(l);
        let is_len = compact.len() == 3 && compact.chars().all(|c| c.is_ascii_hexdigit()) && !l.contains(' ');
        if is_len {
            let total = usize::from_str_radix(&compact, 16).unwrap_or(0);
            let mut parts: Vec<(usize, Vec<u8>)> = Vec::new();
            i += 1;
            while i < lines.len() {
                let Some((n, rest)) = lines[i].split_once(':') else { break };
                let Ok(idx) = usize::from_str_radix(n.trim(), 16) else { break };
                if let Some(b) = hex_bytes(rest) {
                    parts.push((idx, b));
                }
                i += 1;
            }
            parts.sort_by_key(|p| p.0);
            let mut data: Vec<u8> = parts.into_iter().flat_map(|p| p.1).collect();
            data.truncate(total);
            frames.push(data);
            continue;
        }
        if let Some(b) = hex_bytes(l) {
            frames.push(b);
        }
        i += 1;
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::mock::MockTransport;

    fn elm(scenario: &str, quirks: &[&str]) -> Elm {
        let q: Vec<String> = quirks.iter().map(|s| s.to_string()).collect();
        let mut e = Elm::new(Box::new(MockTransport::new(scenario, &q).unwrap()));
        e.timings = Timings { atz: Duration::from_millis(800), normal: Duration::from_millis(800), probe: Duration::from_secs(2), fallback_probe: Duration::from_secs(1) };
        e
    }

    async fn full_roundtrip(quirks: &[&str]) {
        let mut e = elm("catalyst", quirks);
        let report = e.init().await.expect("init");
        assert!(report.version.contains("ELM327"));
        assert!(report.protocol.contains("CAN"), "protocol {}", report.protocol);
        let rpm = e.read_pid(obd::pid_by_name("rpm").unwrap()).await.unwrap();
        assert!((700.0..800.0).contains(&rpm), "rpm {rpm}");
        assert_eq!(e.read_dtcs(0x03).await.unwrap(), vec!["P0420"]);
        assert_eq!(e.read_vin().await.unwrap(), crate::transport::mock::MOCK_VIN);
        assert_eq!(e.freeze_frame_dtc().await.unwrap().as_deref(), Some("P0420"));
        assert_eq!(e.freeze_frame_pid(obd::pid_by_name("speed").unwrap()).await.unwrap(), 98.0);
        let sup = e.supported_pids().await.unwrap();
        assert!(sup.contains(&0x0C) && sup.contains(&0x42));
        e.clear_dtcs().await.unwrap();
        assert!(e.read_dtcs(0x03).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn clean_adapter() {
        full_roundtrip(&[]).await;
    }

    #[tokio::test]
    async fn stuck_echo_and_no_spaces() {
        full_roundtrip(&["echo_stuck", "no_spaces"]).await;
    }

    #[tokio::test]
    async fn v15_without_timing_commands() {
        full_roundtrip(&["v15"]).await;
    }

    #[tokio::test]
    async fn slow_boot_dropped_first_noisy() {
        full_roundtrip(&["slow_boot", "drop_first", "noisy_atz"]).await;
    }

    #[tokio::test]
    async fn auto_protocol_failure_falls_back() {
        let mut e = elm("healthy", &["unable_first"]);
        let r = e.init().await.unwrap();
        assert!(r.notes.iter().any(|n| n.contains("one by one")));
    }

    #[tokio::test]
    async fn fake_supported_pid_reports_no_data() {
        let mut e = elm("healthy", &["fake_support"]);
        e.init().await.unwrap();
        assert!(e.supported_pids().await.unwrap().contains(&0x5E));
        assert_eq!(e.read_pid(obd::pid_by_name("fuel_rate").unwrap()).await, Err(ElmError::NoData));
    }

    #[tokio::test]
    async fn raw_log_never_contains_vin() {
        let mut e = elm("catalyst", &[]);
        e.init().await.unwrap();
        e.read_vin().await.unwrap();
        let all: String = e.take_raw().iter().map(|r| format!("{} {}", r.cmd, r.resp)).collect();
        assert!(!all.contains("9BA012345") && !all.contains("57 41 55"));
    }

    #[test]
    fn assembles_multiline_and_legacy_frames() {
        let lines: Vec<String> = ["014", "0: 49 02 01 57 41 55", "1: 5A 5A 5A 38 4B 39 42", "2: 41 30 31 32 33 34 35"].iter().map(|s| s.to_string()).collect();
        let f = assemble_frames(&lines);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].len(), 20);
        let legacy: Vec<String> = ["41 0C 1A F8", "41 0C 1A F8"].iter().map(|s| s.to_string()).collect();
        assert_eq!(assemble_frames(&legacy).len(), 2);
    }

    #[test]
    fn malformed_vin_reply_is_redacted() {
        let encoded_vin = "49 02 01 57 41 55 5A 5A 5A 38 4B 39 42 39 30 31 32 33 34 35";
        assert_eq!(redact_reply("0902", encoded_vin), "[VIN reply redacted]");
        assert!(!redact_reply("0902", encoded_vin).contains("57 41 55"));
    }

    #[test]
    fn supported_pid_frames_are_unioned() {
        let frames = vec![
            vec![0x41, 0x00, 0x80, 0x00, 0x00, 0x00],
            vec![0x41, 0x00, 0x40, 0x00, 0x00, 0x01],
        ];
        let (pids, continuation) = decode_supported_frames(0, &frames);
        assert!(pids.contains(&1) && pids.contains(&2));
        assert!(continuation);
    }
}
