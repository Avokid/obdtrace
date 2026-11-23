//! Simulated ELM327 adapter + vehicle, speaking the real wire protocol.
//! Used for the demo mode and tests. Quirk profiles emulate misbehaving clones.

use super::Transport;
use crate::obd::{decode_pid, pid_by_code, PIDS};
use crate::store::Sample;
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const AUDI_DEMO: &str = "audi_demo";
pub const DEMO_HISTORY_MS: i64 = 32_000;
pub const SCENARIOS: &[&str] = &["catalyst", "healthy", "misfire", "lean", AUDI_DEMO];
pub const QUIRKS: &[&str] = &["echo_stuck", "v15", "no_spaces", "slow_boot", "drop_first", "fake_support", "unable_first", "noisy_atz", "no_vin"];
pub const MOCK_VIN: &str = "WAUZZZ8K9BA012345";

pub struct MockTransport {
    scenario: String,
    quirks: Vec<String>,
    inbuf: String,
    out: VecDeque<Vec<u8>>,
    ready_at: Instant,
    t0: Instant,
    echo: bool,
    spaces: bool,
    protocol_set: bool,
    dropped_first: bool,
    dtcs: Vec<(u8, u8)>,
    pending: Vec<(u8, u8)>,
    mil: bool,
}

impl MockTransport {
    pub fn new(scenario: &str, quirks: &[String]) -> Result<Self> {
        if !SCENARIOS.contains(&scenario) {
            bail!("unknown mock scenario '{scenario}' (available: {})", SCENARIOS.join(", "));
        }
        for q in quirks {
            if !QUIRKS.contains(&q.as_str()) {
                bail!("unknown mock quirk '{q}' (available: {})", QUIRKS.join(", "));
            }
        }
        let (dtcs, pending): (Vec<(u8, u8)>, Vec<(u8, u8)>) = match scenario {
            "catalyst" | AUDI_DEMO => (vec![(0x04, 0x20)], vec![]),
            "misfire" => (vec![(0x03, 0x01)], vec![]),
            "lean" => (vec![(0x01, 0x71)], vec![(0x01, 0x71)]),
            _ => (vec![], vec![]),
        };
        let mil = !dtcs.is_empty();
        let now = Instant::now();
        Ok(Self {
            scenario: scenario.to_string(),
            quirks: quirks.to_vec(),
            inbuf: String::new(),
            out: VecDeque::new(),
            ready_at: now,
            t0: now,
            echo: true,
            spaces: !quirks.iter().any(|q| q == "no_spaces"),
            protocol_set: false,
            dropped_first: false,
            dtcs,
            pending,
            mil,
        })
    }

    fn q(&self, name: &str) -> bool {
        self.quirks.iter().any(|q| q == name)
    }

    fn t(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    pub fn demo_history(end_ms: i64) -> Result<Vec<Sample>> {
        let demo = Self::new(AUDI_DEMO, &[])?;
        Ok((0..=DEMO_HISTORY_MS).step_by(500).flat_map(|offset| {
            let demo = &demo;
            PIDS.iter().map(move |pid| Sample {
                t_ms: end_ms - DEMO_HISTORY_MS + offset,
                name: pid.name.into(),
                value: decode_pid(pid, &demo.encode(pid.pid, demo.value_at(pid.name, offset as f64 / 1000.0))).unwrap(),
                unit: pid.unit.into(),
            })
        }).collect())
    }

    fn hex(&self, bytes: &[u8]) -> String {
        let sep = if self.spaces { " " } else { "" };
        let parts: Vec<String> = bytes.iter().map(|b| format!("{b:02X}")).collect();
        let mut s = parts.join(sep);
        if self.spaces {
            s.push(' ');
        }
        s
    }

    fn supported(&self) -> Vec<u8> {
        if self.scenario == AUDI_DEMO {
            return std::iter::once(0x01).chain(PIDS.iter().map(|p| p.pid)).collect();
        }
        let mut v: Vec<u8> = PIDS.iter().map(|p| p.pid).filter(|p| ![0x08, 0x09, 0x5C, 0x5E].contains(p)).collect();
        v.push(0x01);
        if self.q("fake_support") {
            v.push(0x5E);
        }
        v
    }

    fn bitmap(&self, base: u8) -> Vec<u8> {
        let sup = self.supported();
        let mut bits = [0u8; 4];
        for p in &sup {
            if *p > base && *p <= base.saturating_add(0x20) && *p as u16 <= base as u16 + 0x20 {
                let i = (*p - base - 1) as usize;
                bits[i / 8] |= 0x80 >> (i % 8);
            }
        }
        if sup.iter().any(|p| *p as u16 > base as u16 + 0x20) {
            bits[3] |= 1;
        }
        bits.to_vec()
    }

    fn value(&self, name: &str) -> f64 {
        let offset = if self.scenario == AUDI_DEMO { DEMO_HISTORY_MS as f64 / 1000.0 } else { 0.0 };
        self.value_at(name, self.t() + offset)
    }

    fn value_at(&self, name: &str, t: f64) -> f64 {
        let gas = if self.scenario == AUDI_DEMO {
            let phase = t % 24.0;
            let pulse = |start: f64, peak: f64, end: f64| {
                ((phase - start) / (peak - start)).min((end - phase) / (end - peak)).clamp(0.0, 1.0)
            };
            0.65 * pulse(5.0, 7.0, 11.0) + pulse(15.0, 18.0, 22.0)
        } else { 0.0 };
        let rpm_jitter = if self.scenario == "misfire" { 120.0 * (t * 13.0).sin() } else { 0.0 };
        match name {
            "rpm" => 740.0 + 25.0 * (t * 0.7).sin() + rpm_jitter + 2500.0 * gas,
            "speed" => 0.0,
            "engine_load" => 22.0 + 2.0 * (t * 0.5).sin() + 46.0 * gas,
            "coolant_temp" => 88.0,
            "short_fuel_trim_b1" => 1.5 * (t * 0.9).sin() + if self.scenario == "lean" { 9.0 } else { 0.0 },
            "long_fuel_trim_b1" => if self.scenario == "lean" { 18.0 } else { 1.0 },
            "short_fuel_trim_b2" => 1.8 * (t * 0.8).sin(),
            "long_fuel_trim_b2" => 2.3,
            "intake_manifold_pressure" => 33.0 + 51.0 * gas,
            "timing_advance" => 12.0 + 16.0 * gas,
            "intake_air_temp" => 31.0,
            "maf" => 3.8 + 33.0 * gas,
            "throttle" => 14.0 + 49.0 * gas,
            "o2_b1s1_voltage" => 0.45 + 0.35 * (t * 2.0).sin() + 0.06 * gas,
            "o2_b1s2_voltage" => if matches!(self.scenario.as_str(), "catalyst" | AUDI_DEMO) { 0.45 + 0.3 * (t * 2.0 - 0.4).sin() + 0.06 * gas } else { 0.65 },
            "run_time" => 300.0 + t,
            "fuel_level" => 61.0,
            "distance_since_clear" => 12.0,
            "barometric_pressure" => 101.0,
            "control_module_voltage" => 14.1,
            "ambient_temp" => 18.0,
            "oil_temp" => 91.0,
            "fuel_rate" => 0.9 + 12.0 * gas,
            _ => 0.0,
        }
    }

    fn encode(&self, pid: u8, v: f64) -> Vec<u8> {
        let u16b = |x: f64| {
            let n = x.max(0.0).min(65535.0) as u16;
            vec![(n >> 8) as u8, n as u8]
        };
        let u8b = |x: f64| vec![x.max(0.0).min(255.0) as u8];
        match pid {
            0x04 | 0x11 | 0x2F => u8b(v * 255.0 / 100.0),
            0x05 | 0x0F | 0x46 | 0x5C => u8b(v + 40.0),
            0x06..=0x09 => u8b((v + 100.0) * 128.0 / 100.0),
            0x0B | 0x0D | 0x33 => u8b(v),
            0x0C => u16b(v * 4.0),
            0x0E => u8b((v + 64.0) * 2.0),
            0x10 => u16b(v * 100.0),
            0x14 | 0x15 => vec![(v * 200.0).clamp(0.0, 255.0) as u8, 0xFF],
            0x1F | 0x31 => u16b(v),
            0x42 => u16b(v * 1000.0),
            0x5E => u16b(v * 20.0),
            _ => vec![0],
        }
    }

    fn vin_reply(&self) -> String {
        let mut data = vec![0x49, 0x02, 0x01];
        data.extend(MOCK_VIN.bytes());
        let mut lines = vec![format!("{:03X}", data.len())];
        lines.push(format!("0: {}", self.hex(&data[0..6])));
        lines.push(format!("1: {}", self.hex(&data[6..13])));
        lines.push(format!("2: {}", self.hex(&data[13..20])));
        lines.join("\r")
    }

    fn dtc_reply(&self, mode: u8, list: &[(u8, u8)]) -> String {
        let mut d = vec![mode + 0x40, list.len() as u8];
        for (a, b) in list {
            d.push(*a);
            d.push(*b);
        }
        self.hex(&d)
    }

    fn obd(&mut self, req: &str) -> String {
        let Ok(bytes) = (0..req.len() / 2).map(|i| u8::from_str_radix(&req[i * 2..i * 2 + 2], 16)).collect::<Result<Vec<u8>, _>>() else {
            return "?".into();
        };
        if bytes.is_empty() || req.len() % 2 == 1 {
            return "?".into();
        }
        if self.q("unable_first") && !self.protocol_set {
            return "SEARCHING...\rUNABLE TO CONNECT".into();
        }
        let mode = bytes[0];
        match (mode, bytes.get(1).copied()) {
            (0x01, Some(p)) if p % 0x20 == 0 && p <= 0x60 => self.hex(&[&[0x41, p][..], &self.bitmap(p)].concat()),
            (0x01, Some(0x01)) => {
                let n = self.dtcs.len() as u8;
                let incomplete = if matches!(self.scenario.as_str(), "catalyst" | AUDI_DEMO) { 0x01 } else { 0x00 };
                self.hex(&[0x41, 0x01, if self.mil { 0x80 | n } else { 0 }, 0x07, 0xE5, incomplete])
            }
            (0x01, Some(p)) => {
                if p == 0x5E {
                    return "NO DATA".into();
                }
                match pid_by_code(p).filter(|_| self.supported().contains(&p)) {
                    Some(def) => {
                        let v = self.value(def.name);
                        self.hex(&[&[0x41, p][..], &self.encode(p, v)].concat())
                    }
                    None => "NO DATA".into(),
                }
            }
            (0x02, Some(p)) if bytes.len() >= 3 => {
                if self.dtcs.is_empty() {
                    return "NO DATA".into();
                }
                if p == 0x02 {
                    let (a, b) = self.dtcs[0];
                    return self.hex(&[0x42, 0x02, 0x00, a, b]);
                }
                let v = match p {
                    0x0C => 2210.0,
                    0x0D => 98.0,
                    0x05 => 90.0,
                    other => pid_by_code(other).map(|d| self.value(d.name)).unwrap_or(0.0),
                };
                self.hex(&[&[0x42, p, 0x00][..], &self.encode(p, v)].concat())
            }
            (0x03, None) => self.dtc_reply(0x03, &self.dtcs.clone()),
            (0x07, None) => self.dtc_reply(0x07, &self.pending.clone()),
            (0x0A, None) => self.dtc_reply(0x0A, &[]),
            (0x04, None) => {
                self.dtcs.clear();
                self.pending.clear();
                self.mil = false;
                self.hex(&[0x44])
            }
            (0x09, Some(0x00)) => self.hex(&[0x49, 0x00, 0x40, 0x00, 0x00, 0x00]),
            (0x09, Some(0x02)) => if self.q("no_vin"){"NO DATA".into()}else{self.vin_reply()},
            _ => "NO DATA".into(),
        }
    }

    fn respond(&mut self, cmd: &str) -> Option<String> {
        if self.q("drop_first") && !self.dropped_first {
            self.dropped_first = true;
            return None;
        }
        let c = cmd.trim().to_uppercase().replace(' ', "");
        let echo = self.echo || self.q("echo_stuck");
        let body: String = if let Some(at) = c.strip_prefix("AT") {
            let v15_unsupported = ["AT", "ST", "CAF", "DPN", "CRA", "SH"].iter().any(|p| at.starts_with(p));
            if self.q("v15") && v15_unsupported {
                "?".into()
            } else if at == "Z" {
                self.echo = true;
                self.protocol_set = false;
                if self.q("slow_boot") {
                    self.ready_at = Instant::now() + Duration::from_millis(1500);
                }
                let noise = if self.q("noisy_atz") { "\u{1}\u{2}zz\r" } else { "" };
                format!("{noise}\rELM327 v1.5")
            } else if at == "E0" {
                self.echo = false;
                "OK".into()
            } else if at == "E1" {
                self.echo = true;
                "OK".into()
            } else if at == "S0" || at == "S1" {
                if !self.q("no_spaces") {
                    self.spaces = at == "S1";
                }
                "OK".into()
            } else if at.starts_with("SP") {
                self.protocol_set = at != "SP0" || !self.q("unable_first");
                "OK".into()
            } else if at == "DPN" {
                "A6".into()
            } else if at == "DP" {
                "ISO 15765-4 (CAN 11/500)".into()
            } else if at == "I" {
                "ELM327 v1.5".into()
            } else if at == "@1" {
                "OBDII to RS232 Interpreter".into()
            } else if at == "RV" {
                "14.1V".into()
            } else {
                "OK".into()
            }
        } else {
            self.obd(&c)
        };
        let mut s = String::new();
        if echo {
            s.push_str(cmd.trim());
            s.push('\r');
        }
        s.push_str(&body);
        s.push_str("\r\r>");
        Some(s)
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn write(&mut self, data: &[u8]) -> Result<()> {
        self.inbuf.push_str(&String::from_utf8_lossy(data));
        while let Some(i) = self.inbuf.find('\r') {
            let cmd: String = self.inbuf.drain(..=i).collect();
            if let Some(resp) = self.respond(&cmd) {
                for chunk in resp.as_bytes().chunks(20) {
                    self.out.push_back(chunk.to_vec());
                }
            }
        }
        Ok(())
    }

    async fn read_chunk(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if Instant::now() >= self.ready_at {
                if let Some(c) = self.out.pop_front() {
                    return Ok(Some(c));
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    async fn close(&mut self) {}

    fn describe(&self) -> String {
        format!("mock:{}", self.scenario)
    }
}
