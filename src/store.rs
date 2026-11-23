//! In-memory ring buffer and SQLite recording store.

use crate::elm::RawEntry;
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

pub fn now_ms() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos().div_euclid(1_000_000) as i64
}

pub fn iso(ms: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000).ok()
        .and_then(|d| d.format(time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z")).ok())
        .unwrap_or_default()
}

pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("OBDTRACE_DATA_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("obdtrace")
}

#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub t_ms: i64,
    pub name: String,
    pub value: f64,
    pub unit: String,
}

pub struct Ring {
    window_ms: i64,
    samples: VecDeque<Sample>,
    raw: VecDeque<RawEntry>,
}

impl Ring {
    pub fn new(window_s: u64) -> Self {
        Self { window_ms: window_s as i64 * 1000, samples: VecDeque::new(), raw: VecDeque::new() }
    }

    pub fn window_s(&self) -> u64 {
        (self.window_ms / 1000) as u64
    }

    pub fn set_window(&mut self, window_s: u64) {
        self.window_ms = window_s as i64 * 1000;
        self.prune();
    }

    fn prune(&mut self) {
        let cutoff = now_ms() - self.window_ms;
        while self.samples.front().is_some_and(|s| s.t_ms < cutoff) {
            self.samples.pop_front();
        }
        while self.raw.front().is_some_and(|r| r.t_ms < cutoff) {
            self.raw.pop_front();
        }
    }

    pub fn push(&mut self, s: Sample) {
        self.samples.push_back(s);
        self.prune();
    }

    pub fn push_raw(&mut self, r: RawEntry) {
        self.raw.push_back(r);
        self.prune();
    }

    pub fn len(&self) -> (usize, usize) {
        (self.samples.len(), self.raw.len())
    }

    pub fn last_seconds(&self, secs: u64) -> (Vec<Sample>, Vec<RawEntry>) {
        let cutoff = now_ms() - secs as i64 * 1000;
        (
            self.samples.iter().filter(|s| s.t_ms >= cutoff).cloned().collect(),
            self.raw.iter().filter(|r| r.t_ms >= cutoff).cloned().collect(),
        )
    }

    pub fn span_ms(&self) -> i64 {
        match (self.samples.front(), self.samples.back()) {
            (Some(a), Some(b)) => b.t_ms - a.t_ms,
            _ => 0,
        }
    }
}

pub struct Db {
    pub(crate) conn: Connection,
}

impl Db {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions(id INTEGER PRIMARY KEY, label TEXT, started_ms INTEGER, ended_ms INTEGER, adapter TEXT, protocol TEXT, vin_masked TEXT, notes TEXT);
             CREATE TABLE IF NOT EXISTS samples(session_id INTEGER, t_ms INTEGER, name TEXT, value REAL, unit TEXT);
             CREATE INDEX IF NOT EXISTS samples_session ON samples(session_id, name, t_ms);",
        )?;
        let mut db = Self { conn };
        db.migrate_history()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn start_session(&self, label: &str, adapter: &str, protocol: &str, vin_masked: &str, notes: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions(label,started_ms,adapter,protocol,vin_masked,notes) VALUES(?,?,?,?,?,?)",
            params![label, now_ms(), adapter, protocol, vin_masked, notes],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert(&mut self, id: i64, batch: &[Sample]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare_cached("INSERT INTO samples(session_id,t_ms,name,value,unit) VALUES(?,?,?,?,?)")?;
            for s in batch {
                st.execute(params![id, s.t_ms, s.name, s.value, s.unit])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list(&self) -> Result<Value> {
        let mut st = self.conn.prepare(
            "SELECT s.id,s.label,s.started_ms,s.ended_ms,s.adapter,s.protocol,s.vin_masked,(SELECT COUNT(*) FROM samples WHERE session_id=s.id) FROM sessions s ORDER BY s.id DESC LIMIT 200",
        )?;
        let rows = st.query_map([], |r| {
            let started: i64 = r.get(2)?;
            let ended: Option<i64> = r.get(3)?;
            Ok(json!({
                "id": r.get::<_, i64>(0)?, "label": r.get::<_, String>(1)?,
                "started": iso(started), "ended": ended.map(iso),
                "duration_s": ended.map(|e| (e - started) / 1000),
                "adapter": r.get::<_, String>(4)?, "protocol": r.get::<_, String>(5)?,
                "vin_masked": r.get::<_, String>(6)?, "samples": r.get::<_, i64>(7)?,
            }))
        })?;
        Ok(Value::Array(rows.collect::<std::result::Result<_, _>>()?))
    }

    fn session_meta(&self, id: i64) -> Result<Value> {
        self.conn
            .query_row("SELECT label,started_ms,ended_ms,adapter,protocol,vin_masked,notes FROM sessions WHERE id=?", [id], |r| {
                let started: i64 = r.get(1)?;
                let ended: Option<i64> = r.get(2)?;
                Ok(json!({"id": id, "label": r.get::<_, String>(0)?, "started": iso(started), "ended": ended.map(iso),
                    "adapter": r.get::<_, String>(3)?, "protocol": r.get::<_, String>(4)?, "vin_masked": r.get::<_, String>(5)?, "notes": r.get::<_, String>(6)?}))
            })
            .map_err(|_| anyhow!("recording {id} not found"))
    }

    fn series(&self, id: i64) -> Result<HashMap<String, (String, Vec<(i64, f64)>)>> {
        let mut st = self.conn.prepare("SELECT name,unit,t_ms,value FROM samples WHERE session_id=? ORDER BY t_ms")?;
        let mut out: HashMap<String, (String, Vec<(i64, f64)>)> = HashMap::new();
        let rows = st.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, f64>(3)?)))?;
        for row in rows {
            let (n, u, t, v) = row?;
            out.entry(n).or_insert_with(|| (u, Vec::new())).1.push((t, v));
        }
        Ok(out)
    }

    pub fn summary(&self, id: i64) -> Result<Value> {
        let meta = self.session_meta(id)?;
        let series = self.series(id)?;
        let mut sensors = serde_json::Map::new();
        let mut names: Vec<&String> = series.keys().collect();
        names.sort();
        for n in names {
            let (unit, pts) = &series[n];
            sensors.insert(n.clone(), stats(unit, pts));
        }
        Ok(json!({"recording": meta, "sensors": sensors}))
    }

    pub fn query(&self, id: i64, pids: &[String], from_s: f64, to_s: Option<f64>, max_points: usize) -> Result<Value> {
        let meta = self.session_meta(id)?;
        let series = self.series(id)?;
        let t0 = series.values().filter_map(|(_, p)| p.first().map(|x| x.0)).min().unwrap_or(0);
        let mut out = serde_json::Map::new();
        let mut names: Vec<&String> = series.keys().filter(|n| pids.is_empty() || pids.contains(n)).collect();
        names.sort();
        for n in names {
            let (unit, pts) = &series[n];
            let sel: Vec<&(i64, f64)> = pts
                .iter()
                .filter(|(t, _)| {
                    let rel = (*t - t0) as f64 / 1000.0;
                    rel >= from_s && to_s.map_or(true, |e| rel <= e)
                })
                .collect();
            let step = sel.len().div_ceil(max_points.max(1)).max(1);
            let points: Vec<Value> = sel.iter().step_by(step).map(|(t, v)| json!([((*t - t0) as f64 / 1000.0 * 100.0).round() / 100.0, v])).collect();
            out.insert(n.clone(), json!({"unit": unit, "points_t_s_value": points, "total_samples_in_range": sel.len(), "downsample_step": step}));
        }
        Ok(json!({"recording": meta, "sensors": out}))
    }

    pub fn compare(&self, a: i64, b: i64) -> Result<Value> {
        let (sa, sb) = (self.series(a)?, self.series(b)?);
        let mut out = serde_json::Map::new();
        let mut names: Vec<&String> = sa.keys().filter(|k| sb.contains_key(*k)).collect();
        names.sort();
        for n in names {
            let (ua, pa) = &sa[n];
            let ma = stats(ua, pa);
            let mb = stats(ua, &sb[n].1);
            let d = ma["avg"].as_f64().zip(mb["avg"].as_f64()).map(|(x, y)| ((y - x) * 100.0).round() / 100.0);
            out.insert(n.clone(), json!({"a": ma, "b": mb, "avg_delta_b_minus_a": d}));
        }
        Ok(json!({"a": self.session_meta(a)?, "b": self.session_meta(b)?, "sensors": out}))
    }

    pub fn export_csv(&self, id: i64, path: &std::path::Path) -> Result<usize> {
        let series = self.series(id)?;
        let mut rows: Vec<(i64, &String, &String, f64)> = Vec::new();
        for (n, (u, pts)) in &series {
            for (t, v) in pts {
                rows.push((*t, n, u, *v));
            }
        }
        rows.sort_by_key(|r| r.0);
        let mut s = String::from("timestamp,sensor,value,unit\n");
        for (t, n, u, v) in &rows {
            s.push_str(&format!("{},{},{},{}\n", iso(*t), n, v, u));
        }
        std::fs::write(path, s)?;
        Ok(rows.len())
    }
}

fn stats(unit: &str, pts: &[(i64, f64)]) -> Value {
    let n = pts.len() as f64;
    if pts.is_empty() {
        return json!({"unit": unit, "count": 0});
    }
    let vals: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let mean = vals.iter().sum::<f64>() / n;
    let sd = (vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
    let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let spikes = if sd > 0.0 { vals.iter().filter(|v| (**v - mean).abs() > 3.0 * sd).count() } else { 0 };
    let r = |x: f64| (x * 100.0).round() / 100.0;
    json!({"unit": unit, "count": pts.len(), "min": r(min), "max": r(max), "avg": r(mean), "stddev": r(sd),
        "first": r(vals[0]), "last": r(*vals.last().unwrap()), "outliers_over_3_sigma": spikes})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_window_and_last_seconds() {
        let mut r = Ring::new(2);
        let now = now_ms();
        r.push(Sample { t_ms: now - 5000, name: "rpm".into(), value: 1.0, unit: "".into() });
        r.push(Sample { t_ms: now, name: "rpm".into(), value: 2.0, unit: "".into() });
        assert_eq!(r.len().0, 1);
        r.set_window(600);
        r.push(Sample { t_ms: now - 5000, name: "rpm".into(), value: 3.0, unit: "".into() });
        assert_eq!(r.last_seconds(2).0.len(), 1);
        assert_eq!(r.last_seconds(60).0.len(), 2);
    }

    #[test]
    fn raw_entries_are_pruned_on_insert() {
        let mut r = Ring::new(2);
        r.push_raw(RawEntry { t_ms: now_ms() - 5000, cmd: "010C".into(), resp: "410C1AF8".into() });
        assert_eq!(r.len().1, 0);
    }

    #[test]
    fn db_summary_query_compare() {
        let mut db = Db::memory().unwrap();
        let a = db.start_session("a", "mock", "CAN", "WAUZZZ8KXXXXXXXXX", "").unwrap();
        let b = db.start_session("b", "mock", "CAN", "WAUZZZ8KXXXXXXXXX", "").unwrap();
        let mk = |v: f64, t: i64| Sample { t_ms: t, name: "rpm".into(), value: v, unit: "rpm".into() };
        db.insert(a, &(0..100).map(|i| mk(700.0 + i as f64, 1000 + i * 100)).collect::<Vec<_>>()).unwrap();
        db.insert(b, &[mk(900.0, 1000), mk(1100.0, 1100)]).unwrap();
        let s = db.summary(a).unwrap();
        assert_eq!(s["sensors"]["rpm"]["count"], 100);
        assert_eq!(s["sensors"]["rpm"]["min"], 700.0);
        let q = db.query(a, &["rpm".into()], 0.0, Some(5.0), 10).unwrap();
        assert!(q["sensors"]["rpm"]["points_t_s_value"].as_array().unwrap().len() <= 10);
        let c = db.compare(a, b).unwrap();
        assert!(c["sensors"]["rpm"]["avg_delta_b_minus_a"].as_f64().unwrap() > 200.0);
        assert!(db.summary(99).is_err());
    }
}
