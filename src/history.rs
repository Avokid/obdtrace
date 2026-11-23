//! Persistent observations and deterministic queries. No diagnostic inference.
use crate::store::{now_ms, Db, Sample};
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

fn limit(args: &Value, default: i64) -> Result<i64> {
    let n = args
        .get("limit")
        .map(|v| v.as_i64().context("limit must be an integer"))
        .transpose()?
        .unwrap_or(default);
    if !(1..=1000).contains(&n) {
        bail!("limit must be between 1 and 1000");
    }
    Ok(n)
}

pub fn integer(args: &Value, name: &str) -> Result<i64> {
    args[name]
        .as_i64()
        .with_context(|| format!("missing integer '{name}'"))
}

/// Expands a raw `query_session` page back into per-sample rows ordered by time.
#[cfg(test)]
pub fn flat_samples(page: &Value) -> Vec<Value> {
    let t0 = page["t0_ms"].as_i64().unwrap_or(0);
    let mut out: Vec<Value> = page["series"].as_array().unwrap().iter().flat_map(|s| {
        s["points"].as_array().unwrap().iter().map(move |p| json!({"name": s["pid"], "unit": s["unit"], "source": s["source"],
            "t_ms": t0 + p[0].as_i64().unwrap(), "value": p[1]}))
    }).collect();
    out.sort_by_key(|s| s["t_ms"].as_i64());
    out
}

pub fn validate(args: &Value) -> Result<()> {
    if !args.is_object() {
        bail!("arguments must be an object");
    }
    for key in [
        "session_id",
        "vehicle_id",
        "from_ms",
        "to_ms",
        "limit",
        "before_id",
        "after_id",
        "after_rowid",
        "annotation_id",
        "expected_revision",
        "bucket_ms",
        "min_duration_ms",
        "after_session_id",
        "after_bucket",
    ] {
        if args
            .get(key)
            .is_some_and(|v| !v.is_null() && v.as_i64().is_none())
        {
            bail!("{key} must be an integer");
        }
    }
    for key in [
        "dtc",
        "state",
        "annotation_status",
        "name",
        "title",
        "assessment",
        "problem_tag",
        "status",
    ] {
        if args
            .get(key)
            .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            bail!("{key} must be a string");
        }
    }
    if let Some(pids) = args.get("pids") {
        if pids
            .as_array()
            .map_or(true, |a| a.iter().any(|v| !v.is_string()))
        {
            bail!("pids must be an array of strings");
        }
    }
    if let (Some(a), Some(b)) = (args["from_ms"].as_i64(), args["to_ms"].as_i64()) {
        if a > b {
            bail!("from_ms must not exceed to_ms");
        }
    }
    Ok(())
}

impl Db {
    pub(crate) fn migrate_history(&mut self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            bail!("database is newer than this application");
        }
        if version == 0 {
            let tx = self.conn.transaction()?;
            tx.execute_batch(
                "ALTER TABLE sessions ADD COLUMN vehicle_id INTEGER;
                 ALTER TABLE sessions ADD COLUMN state TEXT NOT NULL DEFAULT 'legacy';
                 ALTER TABLE sessions ADD COLUMN end_reason TEXT;
                 ALTER TABLE samples ADD COLUMN source TEXT NOT NULL DEFAULT 'sampler';
                 CREATE TABLE vehicles(id INTEGER PRIMARY KEY,name TEXT NOT NULL,vin_hmac TEXT UNIQUE,vin_masked TEXT);
                 CREATE TABLE events(id INTEGER PRIMARY KEY,session_id INTEGER NOT NULL,t_ms INTEGER NOT NULL,kind TEXT NOT NULL,data TEXT NOT NULL);
                 CREATE TABLE dtc_codes(event_id INTEGER NOT NULL,session_id INTEGER NOT NULL,category TEXT NOT NULL,code TEXT NOT NULL);
                 CREATE TABLE annotations(id INTEGER PRIMARY KEY,session_id INTEGER NOT NULL,revision INTEGER NOT NULL,body TEXT NOT NULL);
                 CREATE TABLE investigations(id INTEGER PRIMARY KEY,session_id INTEGER NOT NULL,label TEXT NOT NULL,started_ms INTEGER NOT NULL,ended_ms INTEGER);
                 CREATE INDEX events_session_time ON events(session_id,t_ms,id);
                 CREATE INDEX codes_lookup ON dtc_codes(code,session_id);
                 CREATE INDEX annotations_session ON annotations(session_id,id);
                 CREATE INDEX sessions_vehicle_time ON sessions(vehicle_id,started_ms,id);
                 CREATE INDEX samples_time ON samples(session_id,t_ms,name);
                 PRAGMA user_version=1;"
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn has_vehicle_keys(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM vehicles WHERE vin_hmac IS NOT NULL)",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn resolve_vehicle(&self, hmac: &str, masked: &str) -> Result<i64> {
        self.conn.execute("INSERT INTO vehicles(name,vin_hmac,vin_masked) VALUES('Vehicle',?,?) ON CONFLICT(vin_hmac) DO NOTHING", params![hmac, masked])?;
        Ok(self
            .conn
            .query_row("SELECT id FROM vehicles WHERE vin_hmac=?", [hmac], |r| {
                r.get(0)
            })?)
    }

    pub fn vehicles(&self) -> Result<Value> {
        let mut q = self
            .conn
            .prepare("SELECT id,name,vin_masked FROM vehicles ORDER BY id")?;
        let rows = q.query_map([], |r| Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"vin_masked":r.get::<_,Option<String>>(2)?})))?;
        Ok(Value::Array(rows.collect::<rusqlite::Result<_>>()?))
    }

    pub fn assign_vehicle(
        &mut self,
        session: i64,
        vehicle: Option<i64>,
        name: &str,
    ) -> Result<Value> {
        let meta = self.session(session)?;
        if !meta["vehicle_id"].is_null() {
            bail!("identified sessions cannot be reassigned to another vehicle");
        }
        let tx = self.conn.transaction()?;
        let id = match vehicle {
            Some(id) => {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM vehicles WHERE id=?)",
                    [id],
                    |r| r.get(0),
                )?;
                if !exists {
                    bail!("vehicle not found");
                }
                id
            }
            None => {
                if name.trim().is_empty() || name.len() > 200 {
                    bail!("provide a vehicle name (1-200 bytes)");
                }
                tx.execute(
                    "INSERT INTO vehicles(name) VALUES(?)",
                    [crate::redact::scrub_text(name)],
                )?;
                tx.last_insert_rowid()
            }
        };
        tx.execute(
            "UPDATE sessions SET vehicle_id=? WHERE id=?",
            params![id, session],
        )?;
        tx.execute(
            "INSERT INTO events(session_id,t_ms,kind,data) VALUES(?,?,'vehicle_selected',?)",
            params![session, now_ms(), json!({"vehicle_id":id}).to_string()],
        )?;
        tx.commit()?;
        Ok(json!({"vehicle_id":id,"session_id":session}))
    }

    #[cfg(test)]
    pub fn start_history(
        &self,
        adapter: &str,
        protocol: &str,
        masked: &str,
        vehicle: Option<i64>,
    ) -> Result<i64> {
        self.start_history_with_demo(adapter, protocol, masked, vehicle, None)
    }

    pub fn start_history_with_demo(
        &self,
        adapter: &str,
        protocol: &str,
        masked: &str,
        vehicle: Option<i64>,
        demo: Option<&[Sample]>,
    ) -> Result<i64> {
        let started_ms = demo.and_then(|rows| rows.first()).map_or_else(now_ms, |s| s.t_ms);
        let label = if demo.is_some() { "Audi demo (simulated)" } else { "automatic" };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("INSERT INTO sessions(label,started_ms,adapter,protocol,vin_masked,notes,vehicle_id,state) VALUES(?,?,?,?,?, '',?,'active')",params![label,started_ms,adapter,protocol,masked,vehicle])?;
        let id = tx.last_insert_rowid();
        if let Some(rows) = demo {
            let mut insert = tx.prepare("INSERT INTO samples(session_id,t_ms,name,value,unit,source) VALUES(?,?,?,?,?,'demo_seed')")?;
            for s in rows {
                insert.execute(params![id, s.t_ms, s.name, s.value, s.unit])?;
            }
            if let Some(last) = rows.last() {
                tx.execute("INSERT INTO events(session_id,t_ms,kind,data) VALUES(?,?,'demo_seed',?)",params![id,started_ms,json!({
                    "simulated":true,"from_ms":started_ms,"to_ms":last.t_ms,"description":"Simulated Audi, stationary with throttle blips; not measured vehicle data."
                }).to_string()])?;
            }
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn recover_history(&self) -> Result<()> {
        self.conn.execute_batch(
            "UPDATE sessions SET state='interrupted',end_reason='process_interrupted',
               ended_ms=MAX(started_ms,COALESCE((SELECT MAX(t_ms) FROM samples WHERE session_id=sessions.id),started_ms),
               COALESCE((SELECT MAX(t_ms) FROM events WHERE session_id=sessions.id),started_ms))
             WHERE state='active';
             UPDATE investigations SET ended_ms=(SELECT ended_ms FROM sessions WHERE id=investigations.session_id)
             WHERE ended_ms IS NULL AND session_id IN(SELECT id FROM sessions WHERE state='interrupted');"
        )?;
        Ok(())
    }

    pub fn finish_history(&mut self, session: i64, reason: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE sessions SET ended_ms=?,state='closed',end_reason=? WHERE id=?",
            params![now_ms(), reason, session],
        )?;
        tx.execute(
            "UPDATE investigations SET ended_ms=? WHERE session_id=? AND ended_ms IS NULL",
            params![now_ms(), session],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn session(&self, id: i64) -> Result<Value> {
        let mut meta=self.session_bounds(id)?;
        let mut q=self.conn.prepare("SELECT category,code,MIN(e.t_ms),MAX(e.t_ms) FROM dtc_codes d JOIN events e ON e.id=d.event_id WHERE d.session_id=? GROUP BY category,code ORDER BY category,code")?;
        let codes=q.query_map([id],|r|Ok(json!({"category":r.get::<_,String>(0)?,"code":r.get::<_,String>(1)?,"first_observed_ms":r.get::<_,i64>(2)?,"last_observed_ms":r.get::<_,i64>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        meta["dtc_history"] = json!(codes);
        let mut q=self.conn.prepare("SELECT name,COUNT(*),MIN(t_ms),MAX(t_ms) FROM samples WHERE session_id=? GROUP BY name")?;
        meta["sample_coverage"]=json!(q.query_map([id],|r|Ok(json!({"pid":r.get::<_,String>(0)?,"count":r.get::<_,i64>(1)?,"first_ms":r.get::<_,i64>(2)?,"last_ms":r.get::<_,i64>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?);
        let mut q=self.conn.prepare("SELECT id,label,started_ms,ended_ms FROM investigations WHERE session_id=? ORDER BY id")?;
        meta["investigations"]=json!(q.query_map([id],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"label":r.get::<_,String>(1)?,"from_ms":r.get::<_,i64>(2)?,"to_ms":r.get::<_,Option<i64>>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?);
        Ok(meta)
    }

    fn session_bounds(&self, id: i64) -> Result<Value> {
        self.conn.query_row("SELECT id,label,started_ms,ended_ms,vehicle_id,state,end_reason,vin_masked,notes FROM sessions WHERE id=?",[id],|r| {
            Ok(json!({"id":r.get::<_,i64>(0)?,"label":r.get::<_,String>(1)?,"started_ms":r.get::<_,i64>(2)?,"ended_ms":r.get::<_,Option<i64>>(3)?,
                "vehicle_id":r.get::<_,Option<i64>>(4)?,"state":r.get::<_,String>(5)?,"end_reason":r.get::<_,Option<String>>(6)?,
                "vin_masked":r.get::<_,String>(7)?,"legacy_notes":r.get::<_,String>(8)?}))
        }).optional()?.context("session not found")
    }

    pub fn sessions(&self, args: &Value) -> Result<Value> {
        let n = limit(args, 50)?;
        let mut q = self.conn.prepare(
            "SELECT id FROM sessions s WHERE (?1 IS NULL OR vehicle_id=?1) AND (?2 IS NULL OR id<?2)
             AND (?3 IS NULL OR started_ms>=?3) AND (?4 IS NULL OR started_ms<=?4)
             AND (?5 IS NULL OR EXISTS(SELECT 1 FROM dtc_codes d WHERE d.session_id=s.id AND d.code=?5))
             AND (?6 IS NULL OR state=?6)
             AND (?7 IS NULL OR EXISTS(SELECT 1 FROM annotations a WHERE a.session_id=s.id AND json_extract(a.body,'$.status')=?7))
             AND (?8 IS NULL OR EXISTS(SELECT 1 FROM annotations a WHERE a.session_id=s.id AND json_extract(a.body,'$.problem_tag')=?8))
             ORDER BY id DESC LIMIT ?9")?;
        let ids = q
            .query_map(
                params![
                    args["vehicle_id"].as_i64(),
                    args["before_id"].as_i64(),
                    args["from_ms"].as_i64(),
                    args["to_ms"].as_i64(),
                    args["dtc"].as_str(),
                    args["state"].as_str(),
                    args["annotation_status"].as_str(),
                    args["problem_tag"].as_str(),
                    n + 1
                ],
                |r| r.get::<_, i64>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = ids.len() > n as usize;
        let page = ids
            .iter()
            .take(n as usize)
            .map(|id| self.session(*id))
            .collect::<Result<Vec<_>>>()?;
        Ok(
            json!({"sessions":page,"next_before_id":if more {page.last().map(|v|v["id"].clone())}else{None}}),
        )
    }

    pub fn event(&self, session: i64, kind: &str, data: Value) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO events(session_id,t_ms,kind,data) VALUES(?,?,?,?)",
            params![
                session,
                now_ms(),
                kind,
                crate::redact::scrub_text(&data.to_string())
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn events(&self, args: &Value) -> Result<Value> {
        let session = integer(args, "session_id")?;
        self.session(session)?;
        let n = limit(args, 100)?;
        let mut q = self.conn.prepare("SELECT id,t_ms,kind,data FROM events WHERE session_id=?1 AND id>?2 AND (?3 IS NULL OR t_ms>=?3) AND (?4 IS NULL OR t_ms<=?4) ORDER BY id LIMIT ?5")?;
        let rows = q.query_map(
            params![
                session,
                args["after_id"].as_i64().unwrap_or(0),
                args["from_ms"].as_i64(),
                args["to_ms"].as_i64(),
                n + 1
            ],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )?;
        let mut events = Vec::new();
        for row in rows {
            let (id, t, kind, data) = row?;
            events.push(
                json!({"id":id,"t_ms":t,"kind":kind,"data":serde_json::from_str::<Value>(&data)?}),
            );
        }
        let more = events.len() > n as usize;
        events.truncate(n as usize);
        Ok(
            json!({"events":events,"next_after_id":if more {events.last().map(|e|e["id"].clone())}else{None}}),
        )
    }

    pub fn dtc_scan(&mut self, session: i64, scan: &Value) -> Result<i64> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO events(session_id,t_ms,kind,data) VALUES(?,?,'dtc_scan',?)",
            params![session, now_ms(), scan.to_string()],
        )?;
        let event = tx.last_insert_rowid();
        for category in ["stored", "pending", "permanent"] {
            if scan[category]["status"] == "ok" {
                if let Some(codes) = scan[category]["codes"].as_array() {
                    for code in codes {
                        tx.execute("INSERT INTO dtc_codes(event_id,session_id,category,code) VALUES(?,?,?,?)",params![event,session,category,code.as_str().context("invalid DTC")?])?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(event)
    }

    pub fn insert_direct(&self, session: i64, sample: &Sample) -> Result<()> {
        self.conn.execute("INSERT INTO samples(session_id,t_ms,name,value,unit,source) VALUES(?,?,?,?,?,'direct')",params![session,sample.t_ms,sample.name,sample.value,sample.unit])?;
        Ok(())
    }

    pub fn query_samples(&self, args: &Value) -> Result<Value> {
        let session = integer(args, "session_id")?;
        let meta = self.session_bounds(session)?;
        let n = limit(args, 500)?;
        let pids = args.get("pids").cloned().unwrap_or(json!([]));
        if !pids.is_array() {
            bail!("pids must be an array");
        }
        let from = args["from_ms"]
            .as_i64()
            .unwrap_or(meta["started_ms"].as_i64().unwrap());
        let to = args["to_ms"].as_i64().unwrap_or(i64::MAX);
        if from > to {
            bail!("from_ms must not exceed to_ms");
        }
        if args.get("bucket_ms").is_some_and(|v| !v.is_null()) {
            return self.query_buckets(args, meta, session, from, to, &pids, n);
        }
        let mut q = self.conn.prepare("SELECT rowid,t_ms,name,value,unit,source FROM samples WHERE session_id=?1 AND t_ms>=?2 AND t_ms<=?3
            AND rowid>?4 AND (json_array_length(?5)=0 OR name IN(SELECT value FROM json_each(?5))) ORDER BY rowid LIMIT ?6")?;
        let rows = q.query_map(
            params![session, from, to, args["after_rowid"].as_i64().unwrap_or(0), pids.to_string(), n + 1],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, f64>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?)),
        )?;
        let mut samples = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let more = samples.len() > n as usize;
        samples.truncate(n as usize);
        let next = if more { samples.last().map(|s| s.0) } else { None };
        let t0 = samples.iter().map(|s| s.1).min();
        // One series per (pid, unit, source) keeps pages lossless without repeating keys per sample.
        let mut groups: std::collections::BTreeMap<(String, String, String), Vec<(i64, i64, f64)>> = Default::default();
        for (rowid, t, name, value, unit, source) in samples.iter().cloned() {
            groups.entry((name, unit, source)).or_default().push((t, rowid, value));
        }
        let series: Vec<Value> = groups
            .into_iter()
            .map(|((pid, unit, source), mut pts)| {
                pts.sort_by_key(|p| (p.0, p.1));
                let points: Vec<Value> = pts.iter().map(|(t, _, v)| json!([t - t0.unwrap_or(0), v])).collect();
                json!({"pid": pid, "unit": unit, "source": source, "points": points})
            })
            .collect();
        Ok(json!({"session": meta, "t0_ms": t0, "rows": samples.len(), "series": series,
            "downsampled": false, "next_after_rowid": next}))
    }

    #[allow(clippy::too_many_arguments)]
    fn query_buckets(&self, args: &Value, meta: Value, session: i64, from: i64, to: i64, pids: &Value, n: i64) -> Result<Value> {
        let width = integer(args, "bucket_ms")?;
        if !(100..=3_600_000).contains(&width) {
            bail!("bucket_ms must be between 100 and 3600000");
        }
        if args.get("after_rowid").is_some_and(|v| !v.is_null()) {
            bail!("use after_bucket, not after_rowid, with bucket_ms");
        }
        let after = args["after_bucket"].as_i64().unwrap_or(-1).max(-1);
        let start = from.saturating_add((after + 1).saturating_mul(width));
        let mut q = self.conn.prepare(
            "SELECT CAST((t_ms-?2)/?4 AS INTEGER) AS b,name,unit,COUNT(*),MIN(value),AVG(value),MAX(value),GROUP_CONCAT(DISTINCT source)
             FROM samples WHERE session_id=?1 AND t_ms>=?5 AND t_ms<=?3
             AND (json_array_length(?6)=0 OR name IN(SELECT value FROM json_each(?6)))
             GROUP BY b,name,unit ORDER BY b,name,unit LIMIT ?7",
        )?;
        let rows = q.query_map(params![session, from, to, width, start, pids.to_string(), n + 1], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?,
                r.get::<_, f64>(4)?, r.get::<_, f64>(5)?, r.get::<_, f64>(6)?, r.get::<_, String>(7)?))
        })?;
        let mut buckets = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut next = None;
        if buckets.len() > n as usize {
            // Pages end on whole buckets so a cursor never splits one time slot.
            let cut = buckets[n as usize].0;
            buckets.retain(|b| b.0 < cut);
            if buckets.is_empty() {
                bail!("limit is smaller than the number of PIDs in one bucket; raise limit or select fewer pids");
            }
            next = buckets.last().map(|b| b.0);
        }
        let mut groups: std::collections::BTreeMap<(String, String), (std::collections::BTreeSet<String>, Vec<Value>)> = Default::default();
        for (b, name, unit, count, min, avg, max, sources) in &buckets {
            let g = groups.entry((name.clone(), unit.clone())).or_default();
            g.0.extend(sources.split(',').map(str::to_string));
            g.1.push(json!([b * width, count, min, (avg * 1000.0).round() / 1000.0, max]));
        }
        let series: Vec<Value> = groups
            .into_iter()
            .map(|((pid, unit), (sources, points))| json!({"pid": pid, "unit": unit, "sources": sources, "points": points}))
            .collect();
        Ok(json!({"session": meta, "t0_ms": from, "bucket_ms": width, "point_fields": ["dt_ms", "n", "min", "avg", "max"],
            "rows": buckets.len(), "series": series, "downsampled": true, "next_after_bucket": next}))
    }

    pub fn start_investigation(&self, session: i64, label: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO investigations(session_id,label,started_ms) VALUES(?,?,?)",
            params![session, crate::redact::scrub_text(label), now_ms()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn stop_investigation(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE investigations SET ended_ms=? WHERE id=?",
            params![now_ms(), id],
        )?;
        Ok(())
    }

    pub fn annotations(&self, session: i64) -> Result<Value> {
        self.session(session)?;
        let mut q = self
            .conn
            .prepare("SELECT id,revision,body FROM annotations WHERE session_id=? ORDER BY id")?;
        let rows = q.query_map([session], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, revision, body) = row?;
            out.push(json!({"id":id,"revision":revision,"annotation":serde_json::from_str::<Value>(&body)?}));
        }
        Ok(json!(out))
    }

    pub fn annotate(&mut self, args: &Value, origin: &str) -> Result<Value> {
        let session = integer(args, "session_id")?;
        let meta = self.session(session)?;
        let from = integer(args, "from_ms")?;
        let to = integer(args, "to_ms")?;
        if from > to
            || from < meta["started_ms"].as_i64().unwrap()
            || to > meta["ended_ms"].as_i64().unwrap_or(now_ms())
        {
            bail!("annotation range is outside session");
        }
        for key in ["title", "assessment", "problem_tag"] {
            if args[key]
                .as_str()
                .map_or(true, |s| s.trim().is_empty() || s.len() > 8000)
            {
                bail!("provide non-empty {key} (max 8000 bytes)");
            }
        }
        let status = args["status"].as_str().unwrap_or("proposed");
        if !["proposed", "confirmed", "rejected"].contains(&status) {
            bail!("invalid annotation status");
        }
        if origin == "ai" && status != "proposed" {
            bail!("only a user can confirm or reject an AI assessment");
        }
        let evidence = args["evidence"]
            .as_array()
            .context("provide evidence references")?;
        if evidence.is_empty() || evidence.len() > 100 {
            bail!("provide 1-100 evidence references");
        }
        for e in evidence {
            if integer(e, "session_id")? != session {
                bail!("evidence must refer to the annotation session");
            }
            if let Some(id) = e["event_id"].as_i64() {
                let exists:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE id=? AND session_id=? AND t_ms BETWEEN ? AND ?)",params![id,session,from,to],|r|r.get(0))?;
                if !exists {
                    bail!("event evidence is outside annotation range");
                }
            } else {
                let pid = e["pid"].as_str().context("sample evidence needs a pid")?;
                let exists:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM samples WHERE session_id=? AND name=? AND t_ms BETWEEN ? AND ?)",params![session,pid,from,to],|r|r.get(0))?;
                if !exists {
                    bail!("sample evidence is not available in annotation range");
                }
            }
        }
        let mut body = args.clone();
        body["origin"] = json!(origin);
        body["updated_by"] = json!(origin);
        body["status"] = json!(status);
        body["updated_ms"] = json!(now_ms());
        body = serde_json::from_str(&crate::redact::scrub_text(&body.to_string()))?;
        let tx = self.conn.transaction()?;
        let (id, revision) = if let Some(id) = args["annotation_id"].as_i64() {
            let (sid, revision, previous): (i64, i64, String) = tx.query_row(
                "SELECT session_id,revision,body FROM annotations WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            if sid != session {
                bail!("annotation belongs to another session");
            }
            if args["expected_revision"].as_i64() != Some(revision) {
                bail!("annotation changed; read the current revision first");
            }
            let previous: Value = serde_json::from_str(&previous)?;
            if origin == "ai" && previous["status"] != "proposed" {
                bail!("AI cannot overwrite a reviewed annotation");
            }
            body["created_ms"] = previous["created_ms"].clone();
            body["origin"] = previous["origin"].clone();
            tx.execute(
                "UPDATE annotations SET revision=?,body=? WHERE id=?",
                params![revision + 1, body.to_string(), id],
            )?;
            (id, revision + 1)
        } else {
            body["created_ms"] = json!(now_ms());
            tx.execute(
                "INSERT INTO annotations(session_id,revision,body) VALUES(?,1,?)",
                params![session, body.to_string()],
            )?;
            (tx.last_insert_rowid(), 1)
        };
        let revision_data = json!({"id":id,"revision":revision,"annotation":body});
        tx.execute(
            "INSERT INTO events(session_id,t_ms,kind,data) VALUES(?,?,'annotation_revision',?)",
            params![session, now_ms(), revision_data.to_string()],
        )?;
        tx.commit()?;
        Ok(revision_data)
    }
}

#[derive(Deserialize)]
struct Condition {
    pid: String,
    min: f64,
    max: f64,
}

impl Db {
    pub fn conditions(&self, args: &Value) -> Result<Value> {
        let vehicle = integer(args, "vehicle_id")?;
        let filters: Vec<Condition> = serde_json::from_value(args["conditions"].clone())?;
        let width = integer(args, "bucket_ms")?;
        let duration = integer(args, "min_duration_ms")?;
        if filters.is_empty()
            || filters.len() > 20
            || !(100..=60_000).contains(&width)
            || !(0..=86_400_000).contains(&duration)
        {
            bail!("use 1-20 conditions, bucket_ms 100-60000 and min_duration_ms 0-86400000");
        }
        let mut seen = std::collections::HashSet::new();
        for c in &filters {
            if crate::obd::pid_by_name(&c.pid).is_none()
                || !seen.insert(&c.pid)
                || !c.min.is_finite()
                || !c.max.is_finite()
                || c.min > c.max
            {
                bail!("invalid or repeated condition");
            }
        }
        let n = limit(args, 100)?;
        // Per-bucket min/max requires every recorded value to satisfy the caller's
        // ranges. Different PIDs are never claimed to have been sampled simultaneously.
        let mut q=self.conn.prepare(
            "WITH filtered AS (
               SELECT s.session_id,CAST((s.t_ms-ss.started_ms)/?1 AS INTEGER) AS bucket,s.name,
                 MIN(s.value) AS lo,MAX(s.value) AS hi,MIN(s.t_ms) AS first_ms,MAX(s.t_ms) AS last_ms,COUNT(*) AS n
               FROM samples s JOIN sessions ss ON ss.id=s.session_id
               WHERE ss.vehicle_id=?2 AND s.t_ms>=?3 AND s.t_ms<=?4 AND
                 s.name IN(SELECT json_extract(value,'$.pid') FROM json_each(?5))
               GROUP BY s.session_id,bucket,s.name
             ), matched AS (
               SELECT f.* FROM filtered f JOIN json_each(?5) c ON json_extract(c.value,'$.pid')=f.name
               WHERE f.lo>=json_extract(c.value,'$.min') AND f.hi<=json_extract(c.value,'$.max')
             ), buckets AS (
               SELECT session_id,bucket,MIN(first_ms) AS first_ms,MAX(last_ms) AS last_ms,SUM(n) AS n
               FROM matched GROUP BY session_id,bucket HAVING COUNT(*)=?6
             ), islands AS (
               SELECT *,bucket-ROW_NUMBER() OVER(PARTITION BY session_id ORDER BY bucket) AS island FROM buckets
             ), intervals AS (
               SELECT session_id,MIN(bucket) AS first_bucket,MAX(bucket) AS last_bucket,
                 MIN(first_ms) AS first_ms,MAX(last_ms) AS last_ms,SUM(n) AS n
               FROM islands GROUP BY session_id,island
             )
             SELECT session_id,first_bucket,first_ms,last_ms,n,last_bucket FROM intervals
             WHERE (session_id>?7 OR (session_id=?7 AND first_bucket>?8)) AND last_ms-first_ms>=?10
             ORDER BY session_id,first_bucket LIMIT ?9")?;
        let rows = q.query_map(
            params![
                width,
                vehicle,
                args["from_ms"].as_i64().unwrap_or(0),
                args["to_ms"].as_i64().unwrap_or(i64::MAX),
                args["conditions"].to_string(),
                filters.len() as i64,
                args["after_session_id"].as_i64().unwrap_or(0),
                args["after_bucket"].as_i64().unwrap_or(-1),
                n + 1,
                duration
            ],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            },
        )?;
        let mut buckets = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let more = buckets.len() > n as usize;
        buckets.truncate(n as usize);
        let cursor = if more {
            buckets
                .last()
                .map(|r| json!({"after_session_id":r.0,"after_bucket":r.1}))
        } else {
            None
        };
        let mut intervals: Vec<Value> = Vec::new();
        for (sid, bucket, first, last, count, last_bucket) in buckets {
            let mut coverage = Vec::new();
            for c in &filters {
                let info=self.conn.query_row(
                    "WITH readings AS (SELECT t_ms,value,t_ms-LAG(t_ms) OVER(ORDER BY t_ms) AS gap FROM samples
                      WHERE session_id=? AND name=? AND t_ms BETWEEN ? AND ?)
                     SELECT MIN(value),MAX(value),COUNT(*),MIN(t_ms),MAX(t_ms),MAX(gap) FROM readings",
                    params![sid,c.pid,first,last],|r|Ok(json!({"pid":c.pid,"min":r.get::<_,Option<f64>>(0)?,"max":r.get::<_,Option<f64>>(1)?,
                        "count":r.get::<_,i64>(2)?,"first_ms":r.get::<_,Option<i64>>(3)?,"last_ms":r.get::<_,Option<i64>>(4)?,"max_gap_ms":r.get::<_,Option<i64>>(5)?})))?;
                coverage.push(info);
            }
            intervals.push(json!({"session_id":sid,"from_ms":first,"to_ms":last,"first_bucket":bucket,"last_bucket":last_bucket,"samples":count,"coverage":coverage}));
        }
        for v in &mut intervals {
            let events=self.events(&json!({"session_id":v["session_id"],"from_ms":v["from_ms"],"to_ms":v["to_ms"],"limit":100}))?;
            v["events"] = events;
            v["observed_span_ms"] =
                json!(v["to_ms"].as_i64().unwrap() - v["from_ms"].as_i64().unwrap());
        }
        Ok(
            json!({"intervals":intervals,"next_cursor":cursor,"criteria":args,
            "coverage_note":"Only recorded samples within matching buckets; no interpolation or proof of continuous conditions. min_duration_ms filters observed spans, not proven continuous duration. max_gap_ms=null means fewer than two samples for a PID. Page by next_cursor; query original samples and events for full evidence. No diagnosis or recurrence conclusion is generated."}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t: i64, pid: &str, value: f64) -> Sample {
        Sample {
            t_ms: t,
            name: pid.into(),
            value,
            unit: "test".into(),
        }
    }
    fn session(db: &Db, vehicle: i64) -> i64 {
        let id = db
            .start_history("mock", "CAN", "WAUZZZ8KXXXXXXXXX", Some(vehicle))
            .unwrap();
        db.conn
            .execute("UPDATE sessions SET started_ms=1000 WHERE id=?", [id])
            .unwrap();
        id
    }

    #[test]
    fn migrates_old_database_without_rewriting_history() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE sessions(id INTEGER PRIMARY KEY,label TEXT,started_ms INTEGER,ended_ms INTEGER,adapter TEXT,protocol TEXT,vin_masked TEXT,notes TEXT);
            CREATE TABLE samples(session_id INTEGER,t_ms INTEGER,name TEXT,value REAL,unit TEXT);
            INSERT INTO sessions VALUES(42,'old',1000,2000,'mock','CAN','WAUZZZ8KXXXXXXXXX','P0420 at unknown time');
            INSERT INTO samples VALUES(42,1500,'rpm',700,'rpm');").unwrap();
        let mut db = Db { conn };
        db.migrate_history().unwrap();
        db.migrate_history().unwrap();
        let meta = db.session(42).unwrap();
        assert_eq!(meta["state"], "legacy");
        assert_eq!(meta["vehicle_id"], Value::Null);
        assert_eq!(meta["legacy_notes"], "P0420 at unknown time");
        assert_eq!(
            flat_samples(&db.query_samples(&json!({"session_id":42})).unwrap())[0]["value"],
            700.0
        );
    }

    #[test]
    fn paginates_all_sessions_and_filters_structured_observations() {
        let mut db = Db::memory().unwrap();
        let car = db
            .resolve_vehicle("private-hmac", "WAUZZZ8KXXXXXXXXX")
            .unwrap();
        for _ in 0..205 {
            session(&db, car);
        }
        let first = db.sessions(&json!({"vehicle_id":car,"limit":100})).unwrap();
        let second = db
            .sessions(&json!({"vehicle_id":car,"limit":100,"before_id":first["next_before_id"]}))
            .unwrap();
        let third = db
            .sessions(&json!({"vehicle_id":car,"limit":100,"before_id":second["next_before_id"]}))
            .unwrap();
        assert_eq!(
            first["sessions"].as_array().unwrap().len()
                + second["sessions"].as_array().unwrap().len()
                + third["sessions"].as_array().unwrap().len(),
            205
        );
        assert!(third["next_before_id"].is_null());
        db.dtc_scan(1,&json!({"stored":{"status":"ok","codes":["P0420"]},"pending":{"status":"no_data"},"permanent":{"status":"error"}})).unwrap();
        db.dtc_scan(1, &json!({"stored":{"status":"ok","codes":[]}}))
            .unwrap();
        let found = db.sessions(&json!({"dtc":"P0420"})).unwrap();
        assert_eq!(found["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(found["sessions"][0]["dtc_history"][0]["code"], "P0420");
        let events = db.events(&json!({"session_id":1})).unwrap();
        assert_eq!(events["events"][0]["data"]["pending"]["status"], "no_data");
        assert!(!db.vehicles().unwrap().to_string().contains("private-hmac"));
    }

    #[test]
    fn annotations_validate_evidence_and_preserve_reviewed_revisions() {
        let mut db = Db::memory().unwrap();
        let sid = session(&db, 1);
        db.insert(sid, &[sample(1100, "rpm", 750.0)]).unwrap();
        let body = json!({"session_id":sid,"from_ms":1000,"to_ms":1200,"title":"Idle","assessment":"Hypothesis","problem_tag":"rough-idle",
            "evidence":[{"session_id":sid,"pid":"rpm"}]});
        let proposal = db.annotate(&body, "ai").unwrap();
        let mut review = body.clone();
        review["annotation_id"] = proposal["id"].clone();
        review["expected_revision"] = proposal["revision"].clone();
        review["status"] = json!("confirmed");
        assert!(db.annotate(&review, "ai").is_err());
        let confirmed = db.annotate(&review, "user").unwrap();
        assert_eq!(confirmed["annotation"]["origin"], "ai");
        assert_eq!(confirmed["annotation"]["updated_by"], "user");
        review["expected_revision"] = confirmed["revision"].clone();
        review["status"] = json!("proposed");
        assert!(db.annotate(&review, "ai").is_err());
        let mut missing = body.clone();
        missing["evidence"] = json!([{"session_id":sid,"pid":"maf"}]);
        assert!(db.annotate(&missing, "ai").is_err());
        missing["evidence"] = json!([{"session_id":sid+1,"pid":"rpm"}]);
        assert!(db.annotate(&missing, "ai").is_err());
        assert_eq!(
            db.events(&json!({"session_id":sid})).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn original_sample_pages_and_condition_intervals_keep_transients_and_gaps() {
        let mut db = Db::memory().unwrap();
        let sid = session(&db, 1);
        let other = session(&db, 2);
        let values = [
            sample(1000, "rpm", 750.0),
            sample(1100, "rpm", 760.0),
            sample(1200, "rpm", 2000.0),
            sample(2000, "rpm", 750.0),
            sample(2100, "rpm", 750.0),
            sample(3000, "rpm", 750.0),
            sample(3100, "rpm", 750.0),
            sample(5000, "rpm", 750.0),
            sample(5100, "rpm", 750.0),
        ];
        db.insert(sid, &values).unwrap();
        db.insert(other, &values).unwrap();
        let page = db
            .query_samples(&json!({"session_id":sid,"limit":2}))
            .unwrap();
        let next = db
            .query_samples(
                &json!({"session_id":sid,"limit":2,"after_rowid":page["next_after_rowid"]}),
            )
            .unwrap();
        assert_eq!(
            flat_samples(&next)[0]["value"], 2000.0,
            "no striding past a transient"
        );
        let mut args = json!({"session_id":sid,"from_ms":1000,"bucket_ms":1000,"limit":2});
        let first = db.query_samples(&args).unwrap();
        assert_eq!(first["downsampled"], true);
        assert_eq!(first["series"][0]["points"], json!([[0, 3, 750.0, 1170.0, 2000.0], [1000, 2, 750.0, 750.0, 750.0]]), "transient max survives aggregation");
        args["after_bucket"] = first["next_after_bucket"].clone();
        let rest = db.query_samples(&args).unwrap();
        assert_eq!(rest["series"][0]["points"], json!([[2000, 2, 750.0, 750.0, 750.0], [4000, 2, 750.0, 750.0, 750.0]]));
        assert!(rest["next_after_bucket"].is_null());
        assert!(db.query_samples(&json!({"session_id":sid,"bucket_ms":1000,"after_rowid":1})).is_err());
        let args = json!({"vehicle_id":1,"conditions":[{"pid":"rpm","min":700,"max":800}],"bucket_ms":1000,"min_duration_ms":1000,"limit":1});
        let found = db.conditions(&args).unwrap();
        assert_eq!(
            found["intervals"].as_array().unwrap().len(),
            1,
            "minimum duration must be applied after joining consecutive buckets"
        );
        assert_eq!(found["intervals"][0]["from_ms"], 2000);
        assert_eq!(found["intervals"][0]["to_ms"], 3100);
        assert_eq!(found["intervals"][0]["session_id"], sid);
        assert_eq!(found["intervals"][0]["coverage"][0]["max_gap_ms"], 900);
        assert!(found.get("diagnosis").is_none());
        let mut missing = args.clone();
        missing["conditions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"pid":"speed","min":0,"max":5}));
        assert_eq!(db.conditions(&missing).unwrap()["intervals"], json!([]));
        let mut pages = args;
        pages["min_duration_ms"] = json!(0);
        let p = db.conditions(&pages).unwrap();
        pages["after_session_id"] = p["next_cursor"]["after_session_id"].clone();
        pages["after_bucket"] = p["next_cursor"]["after_bucket"].clone();
        let p2 = db.conditions(&pages).unwrap();
        assert_eq!(p2["intervals"][0]["from_ms"], 5000);
    }

    #[test]
    fn interrupted_session_ends_at_last_persisted_evidence() {
        let mut db = Db::memory().unwrap();
        let sid = session(&db, 1);
        db.insert(sid, &[sample(1250, "rpm", 750.0)]).unwrap();
        db.recover_history().unwrap();
        assert_eq!(db.session(sid).unwrap()["ended_ms"], 1250);
        assert_eq!(db.session(sid).unwrap()["state"], "interrupted");
    }
}
