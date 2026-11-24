//! Minimal MCP server over stdio (JSON-RPC 2.0, newline-delimited).
//! Tools run either in-process (this process owns the adapter) or are forwarded
//! to the authenticated adapter owner over its private TLS bridge.

use crate::hub::{Hub, APP_VERSION};
use crate::prompts;
use crate::tools::{self, Ctx, Origin};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const HISTORY_GUIDE_URI: &str = "obd://history-guide";
const HISTORY_GUIDE: &str = include_str!("../docs/mcp-history.md");

const INSTRUCTIONS: &str = "\
Use the OBDTrace MCP tools directly; do not bypass the client with shell commands, HTTP calls or SQLite queries. Use only the tools exposed by your client. \
Choose the intended real or demo server and start live work with connection_status. Check adapter.connected, target, session_id and storage_error. A mock target is simulation, not vehicle evidence.

If disconnected and the user wants to connect, use list_adapters and connect if both are available; otherwise use the app's Connect page. \
Review discovery warnings, ask the user to choose if ambiguous, and use the exact selected target. Classic Bluetooth needs OS pairing first; some BLE adapters do too. \
Do not replace an active connection without the user's approval: even a failed switch can drop it. With no connection, offer archived findings as historical, not current.

Use get_live_snapshot first for routine values; check each sensor's age_ms and unit. For stale or missing values, read_live_data with only the needed pids, and check individual errors. \
For fault or MIL questions use read_dtcs; only status=ok with codes=[] is an empty result for that category. Check readiness separately within the result. \
Freeze frames are historical ECU data, may be partial, and do not establish fault onset. Never clear codes as a diagnostic step.

History is saved automatically; no start_recording call is needed. When asked to save measurements, check session_id and storage_error, then verify the requested PIDs and interval with query_session before saying they were saved. \
start_recording and stop_recording only mark a named interval. If exposed and requested, use save_diagnostic_log for a recent local bundle, export_recording for a whole-session sample CSV, or save_fault_annotation for a proposal with evidence. \
A server started with OBDTRACE_READ_ONLY=1 does not expose these write operations. For details, read the MCP resource obd://history-guide if the client supports resources.

For archive analysis, use list_vehicles, list_sessions and get_session to select the right session and check sample_coverage. Use vehicle_id rather than masked VIN. \
Use query_session for samples and get_session_events for context; keep PID/time ranges narrow, use UTC epoch milliseconds, and follow returned cursors with the same filters. query_session returns per-PID series with points [dt_ms, value] relative to t0_ms; for long ranges scan first with bucket_ms (points [dt_ms, n, min, avg, max]) and then read raw points for short windows. \
Pin the end time while paging an active session. Check sample sources, gaps and comparable operating conditions; demo_seed is simulated. Missing data or no comparable interval is not evidence that a fault is absent.

Report evidence with timestamps, units and what could not be verified. Codes indicate failed tests, not failed parts. Generic OBD-II does not cover every module. \
Do not ask the user to interact while driving or to reproduce a dangerous condition; refer safety-critical faults to a qualified professional.";

pub enum Backend {
    Local(Arc<Hub>),
    Remote(crate::bridge::Peer),
}

impl Backend {
    async fn call(&self, name: &str, args: Value, confirmed: bool) -> Result<Value> {
        match self {
            Backend::Local(h) => {
                tools::call(
                    h,
                    name,
                    args,
                    Ctx {
                        origin: Origin::Mcp,
                        confirmed,
                        mcp_clear_enabled: tools::clear_enabled_for_mcp(),
                    },
                )
                .await
            }
            Backend::Remote(peer) => {
                peer.request(json!({"op":"call","name":name,"args":args,"confirmed":confirmed}))
                    .await
            }
        }
    }

    async fn hello(&self, name: &str, version: &str) {
        match self {
            Backend::Local(h) => h.note_client(name, version),
            Backend::Remote(peer) => {
                let _ = peer
                    .request(json!({"op":"hello","name":name,"version":version}))
                    .await;
            }
        }
    }
}

pub struct Mcp {
    backend: Backend,
    out: mpsc::UnboundedSender<String>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
    next_id: AtomicU64,
    client_elicits: AtomicBool,
    subscribed: Mutex<HashSet<String>>,
    input_closed: AtomicBool,
}

fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

impl Mcp {
    pub fn new(backend: Backend, out: mpsc::UnboundedSender<String>) -> Arc<Self> {
        Arc::new(Self {
            backend,
            out,
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            client_elicits: AtomicBool::new(false),
            subscribed: Mutex::new(HashSet::new()),
            input_closed: AtomicBool::new(false),
        })
    }

    fn send(&self, v: Value) {
        let _ = self.out.send(v.to_string());
    }

    async fn elicit_confirmation(&self, message: &str) -> bool {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if self.input_closed.load(Ordering::SeqCst) {
                return false;
            }
            pending.insert(id, tx);
        }
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": "elicitation/create", "params": {
            "message": message,
            "requestedSchema": {"type": "object", "properties": {"confirm": {"type": "boolean", "title": "Yes, clear the trouble codes", "default": false}}, "required": ["confirm"]}
        }}));
        match tokio::time::timeout(Duration::from_secs(120), rx).await {
            Ok(Ok(v)) => {
                v["result"]["action"] == "accept" && v["result"]["content"]["confirm"] == true
            }
            _ => false,
        }
    }

    pub fn resolve(&self, msg: Value) {
        if let Some(id) = msg["id"].as_u64() {
            if let Some(tx) = self.pending.lock().unwrap().remove(&id) {
                let _ = tx.send(msg);
            }
        }
    }

    pub async fn tick(&self) {
        let uris: Vec<String> = self.subscribed.lock().unwrap().iter().cloned().collect();
        for uri in uris {
            self.send(json!({"jsonrpc": "2.0", "method": "notifications/resources/updated", "params": {"uri": uri}}));
        }
    }

    async fn tool_call(&self, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or("");
        let mut args = if params["arguments"].is_null() {
            json!({})
        } else {
            params["arguments"].clone()
        };
        let text = |t: String, is_err: bool| json!({"content": [{"type": "text", "text": t}], "isError": is_err});
        let mut confirmed = false;
        if !tools::allowed_for_mcp(name) {
            return text("Tool disabled in this MCP profile".into(), true);
        }
        if name == "clear_dtcs"
            && args["confirm"].as_bool() == Some(true)
            && tools::clear_enabled_for_mcp()
        {
            if !self.client_elicits.load(Ordering::Relaxed) {
                return text("This AI client cannot show a confirmation dialog, so codes are not cleared. Clear them from the OBD2 app instead.".into(), true);
            }
            let Ok(status) = self
                .backend
                .call("connection_status", json!({}), false)
                .await
            else {
                return text("Cannot verify vehicle session".into(), true);
            };
            let Some(session) = status["session_id"].as_i64() else {
                return text("No active vehicle session".into(), true);
            };
            args["session_id"] = json!(session);
            confirmed = self.elicit_confirmation(&format!("Clear all trouble codes for vehicle session {session}? This erases freeze-frame data and resets emissions readiness monitors. A backup log is saved first.")).await;
        }
        match self.backend.call(name, args, confirmed).await {
            Ok(v) => text(serde_json::to_string(&v).unwrap_or_default(), false),
            Err(e) => text(e.to_string(), true),
        }
    }

    pub async fn handle(self: &Arc<Self>, msg: Value) -> Option<Value> {
        let method = msg["method"].as_str()?.to_string();
        let id = msg.get("id").cloned();
        let params = &msg["params"];
        let Some(id) = id else {
            return None; // notifications need no reply
        };
        Some(match method.as_str() {
            "initialize" => {
                let want = params["protocolVersion"].as_str().unwrap_or("");
                let version = if VERSIONS.contains(&want) {
                    want
                } else {
                    VERSIONS[0]
                };
                self.client_elicits.store(
                    params["capabilities"].get("elicitation").is_some(),
                    Ordering::Relaxed,
                );
                let ci = &params["clientInfo"];
                self.backend
                    .hello(
                        ci["name"].as_str().unwrap_or("unknown client"),
                        ci["version"].as_str().unwrap_or(""),
                    )
                    .await;
                ok(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": {"tools": {}, "prompts": {}, "resources": {"subscribe": true}},
                        "serverInfo": {"name": "obdtrace", "version": APP_VERSION},
                        "instructions": INSTRUCTIONS
                    }),
                )
            }
            "ping" => ok(id, json!({})),
            "tools/list" => ok(
                id,
                json!({"tools": tools::definitions().into_iter().filter(|v| tools::allowed_for_mcp(v["name"].as_str().unwrap_or(""))).collect::<Vec<_>>()}),
            ),
            "tools/call" => ok(id, self.tool_call(params).await),
            "prompts/list" => ok(id, json!({"prompts": prompts::mcp_list()})),
            "prompts/get" => match prompts::mcp_get(
                params["name"].as_str().unwrap_or(""),
                params["arguments"]["language"].as_str().unwrap_or("en"),
                &params["arguments"],
            ) {
                Some(p) => ok(id, p),
                None => err(id, -32602, "unknown prompt"),
            },
            "resources/list" => ok(
                id,
                json!({"resources": [
                {"uri": "obd://live", "name": "Live sensor snapshot", "mimeType": "application/json", "description": "Latest continuously sampled sensor values"},
                {"uri": "obd://status", "name": "Connection status", "mimeType": "application/json"},
                {"uri": HISTORY_GUIDE_URI, "name": "History and saving playbook", "mimeType": "text/markdown", "description": "Verify automatic capture, investigate an archived symptom, export evidence, and save a proposed assessment."}]}),
            ),
            "resources/read" => {
                let uri = params["uri"].as_str().unwrap_or("");
                let name = match uri {
                    "obd://live" => "get_live_snapshot",
                    "obd://status" => "connection_status",
                    HISTORY_GUIDE_URI => {
                        return Some(ok(
                            id,
                            json!({"contents": [
                                {"uri": uri, "mimeType": "text/markdown", "text": HISTORY_GUIDE}
                            ]}),
                        ))
                    }
                    _ => return Some(err(id, -32602, "unknown resource")),
                };
                let text = match self.backend.call(name, json!({}), false).await {
                    Ok(v) => v.to_string(),
                    Err(e) => json!({"error": e.to_string()}).to_string(),
                };
                ok(
                    id,
                    json!({"contents": [{"uri": uri, "mimeType": "application/json", "text": text}]}),
                )
            }
            "resources/subscribe" => match params["uri"].as_str() {
                Some(uri @ ("obd://live" | "obd://status")) => {
                    self.subscribed.lock().unwrap().insert(uri.to_string());
                    ok(id, json!({}))
                }
                _ => err(id, -32602, "unknown resource"),
            },
            "resources/unsubscribe" => match params["uri"].as_str() {
                Some(uri @ ("obd://live" | "obd://status")) => {
                    self.subscribed.lock().unwrap().remove(uri);
                    ok(id, json!({}))
                }
                _ => err(id, -32602, "unknown resource"),
            },
            _ => err(id, -32601, "method not found"),
        })
    }
}

pub async fn run_stdio(backend: Backend) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let mcp = Mcp::new(backend, tx);
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if out.write_all(format!("{line}\n").as_bytes()).await.is_err()
                || out.flush().await.is_err()
            {
                break;
            }
        }
    });
    let ticker = mcp.clone();
    let ticker_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            ticker.tick().await;
        }
    });
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut tasks = tokio::task::JoinSet::new();
    while let Some(line) = lines.next_line().await? {
        while let Some(result) = tasks.try_join_next() {
            result?;
        }
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            mcp.send(err(Value::Null, -32700, "parse error"));
            continue;
        };
        if msg.get("method").is_none() {
            mcp.resolve(msg);
            continue;
        }
        let m = mcp.clone();
        tasks.spawn(async move {
            if let Some(resp) = m.handle(msg).await {
                m.send(resp);
            }
        });
    }
    mcp.input_closed.store(true, Ordering::SeqCst);
    mcp.pending.lock().unwrap().clear();
    while let Some(result) = tasks.join_next().await {
        result?;
    }
    ticker_task.abort();
    let _ = ticker_task.await;
    drop(mcp);
    writer.await?;
    Ok(())
}

/// In-process handshake used by `doctor`: initialize, tools/list, ping and a status call.
pub async fn selftest(hub: Arc<Hub>) -> Result<Vec<(String, bool)>> {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mcp = Mcp::new(Backend::Local(hub), tx);
    let mut results = Vec::new();
    let init = mcp.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "obdtrace selftest", "version": "0"}}})).await;
    results.push((
        "initialize".into(),
        init.is_some_and(|r| r["result"]["serverInfo"]["name"] == "obdtrace"),
    ));
    let list = mcp
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .await;
    results.push((
        "tools/list".into(),
        list.is_some_and(|r| {
            r["result"]["tools"]
                .as_array()
                .is_some_and(|t| t.len() > 10)
        }),
    ));
    let ping = mcp
        .handle(json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}))
        .await;
    results.push(("ping".into(), ping.is_some()));
    let call = mcp.handle(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "connection_status", "arguments": {}}})).await;
    results.push((
        "tools/call connection_status".into(),
        call.is_some_and(|r| r["result"]["isError"] == false),
    ));
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Db;

    fn hub() -> Arc<Hub> {
        Hub::new(
            std::env::temp_dir().join(format!("obdtrace-test-{}", uuid::Uuid::new_v4())),
            Db::memory().unwrap(),
        )
        .unwrap()
    }

    async fn rpc(m: &Arc<Mcp>, v: Value) -> Value {
        m.handle(v).await.unwrap()
    }

    #[tokio::test]
    async fn full_mcp_flow_against_mock_car() {
        let h = hub();
        let (tx, _rx) = mpsc::unbounded_channel();
        let m = Mcp::new(Backend::Local(h.clone()), tx);
        let init = rpc(&m, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).await;
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        let instructions = init["result"]["instructions"].as_str().unwrap();
        assert!(
            instructions.contains("tools exposed by your client")
                && instructions.contains("Connect page")
        );
        let list = rpc(&m, json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).await;
        let schema = |name| {
            &list["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap()["inputSchema"]
        };
        assert_eq!(schema("list_vehicles")["properties"], json!({}));
        assert_eq!(schema("get_session")["required"], json!(["session_id"]));
        assert_eq!(
            schema("get_session")["properties"]
                .as_object()
                .unwrap()
                .len(),
            1
        );
        for (name, cursor, default) in [
            ("list_sessions", "before_id", 50),
            ("query_session", "after_rowid", 500),
            ("get_session_events", "after_id", 100),
        ] {
            let props = &schema(name)["properties"];
            assert_eq!(props[cursor]["type"], "integer");
            assert_eq!(
                props["limit"],
                json!({"type":"integer","minimum":1,"maximum":1000,"default":default})
            );
            assert_eq!(
                ["before_id", "after_rowid", "after_id"]
                    .iter()
                    .filter(|key| props.get(**key).is_some())
                    .count(),
                1
            );
        }
        let call = |name: &str, args: Value| json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":name,"arguments":args}});
        let r = rpc(&m, call("connection_status", json!({}))).await;
        let status: Value =
            serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(status["adapter"]["connected"], false);
        let hint = status["next_step"].as_str().unwrap();
        assert!(hint.contains("If your client exposes") && hint.contains("Connect page"));
        let r = rpc(&m, call("read_dtcs", json!({}))).await;
        assert_eq!(
            r["result"]["isError"], true,
            "must fail cleanly when not connected"
        );
        let r = rpc(&m, call("connect", json!({"target":"mock"}))).await;
        assert_eq!(r["result"]["isError"], false, "{r}");
        let r = rpc(&m, call("read_dtcs", json!({}))).await;
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("P0420"));
        let r = rpc(&m, call("read_vin", json!({}))).await;
        let t = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(t.contains("WAUZZZ8K") && !t.contains("9BA012345"));
        let r = rpc(&m, call("clear_dtcs", json!({"confirm": true}))).await;
        assert_eq!(
            r["result"]["isError"], true,
            "clear must be refused over MCP by default"
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let r = rpc(&m, call("get_live_snapshot", json!({}))).await;
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("rpm"));
        let pids = ["o2_b1s1_voltage", "o2_b1s2_voltage"];
        let r = rpc(
            &m,
            call("query_session", json!({"session_id":1,"pids":pids})),
        )
        .await;
        assert_eq!(r["result"]["isError"], false, "{r}");
        let history: Value =
            serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        let samples = crate::history::flat_samples(&history);
        for pid in pids {
            assert!(
                samples
                    .iter()
                    .filter(|s| s["name"] == pid && s["unit"] == "V" && s["source"] == "sampler")
                    .count()
                    >= 2,
                "missing automatic voltage history for {pid}"
            );
        }
        let r = rpc(&m, call("start_recording", json!({"label":"t"}))).await;
        assert_eq!(r["result"]["isError"], false, "{r}");
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let r = rpc(&m, call("stop_recording", json!({}))).await;
        assert_eq!(r["result"]["isError"], false);
        let r = rpc(
            &m,
            call("get_recording_summary", json!({"recording_id": 1})),
        )
        .await;
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("rpm"),
            "{r}"
        );
        let r = rpc(
            &m,
            call(
                "save_diagnostic_log",
                json!({"duration_s": 60, "label": "unit test"}),
            ),
        )
        .await;
        assert_eq!(r["result"]["isError"], false, "{r}");
        let r = rpc(&m, json!({"jsonrpc":"2.0","id":3,"method":"prompts/get","params":{"name":"check_engine_light"}})).await;
        assert!(r["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("read_dtcs"));
        let dir = std::path::PathBuf::from(h.status()["data_dir"].as_str().unwrap());
        h.disconnect().await.unwrap();
        drop(m);
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn audi_demo_exposes_seeded_throttle_history_for_every_sensor() {
        let h = hub();
        let dir = std::path::PathBuf::from(h.status()["data_dir"].as_str().unwrap());
        let status = h.connect("mock:audi_demo").await.unwrap();
        let session = status["session_id"].as_i64().unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();
        let m = Mcp::new(Backend::Local(h.clone()), tx);
        let query = |name, args| json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}});
        let decode = |r: Value| {
            assert_eq!(r["result"]["isError"], false, "{r}");
            serde_json::from_str::<Value>(r["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap()
        };
        let pids = crate::obd::PIDS;
        let supported = decode(rpc(&m, query("list_supported_sensors", json!({}))).await);
        assert_eq!(
            supported["supported_sensor_names"]
                .as_array()
                .unwrap()
                .len(),
            pids.len()
        );
        let events = decode(
            rpc(
                &m,
                query("get_session_events", json!({"session_id":session})),
            )
            .await,
        );
        let demo = events["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "demo_seed")
            .unwrap();
        assert_eq!(demo["data"]["simulated"], true);
        let end = demo["data"]["to_ms"].as_i64().unwrap();
        let mut args = json!({"session_id":session,"from_ms":end-30_000,"to_ms":end,"limit":1000});
        let mut samples = Vec::new();
        loop {
            let result = decode(rpc(&m, query("query_session", args.clone())).await);
            samples.extend(crate::history::flat_samples(&result));
            if result["next_after_rowid"].is_null() {
                break;
            }
            args["after_rowid"] = result["next_after_rowid"].clone();
        }
        for pid in pids {
            let rows: Vec<_> = samples.iter().filter(|s| s["name"] == pid.name).collect();
            assert!(rows.len() >= 61, "{} has incomplete coverage", pid.name);
            assert!(rows
                .iter()
                .all(|s| s["source"] == "demo_seed" && s["unit"] == pid.unit));
        }
        let peak = samples
            .iter()
            .filter(|s| s["name"] == "throttle")
            .max_by(|a, b| {
                a["value"]
                    .as_f64()
                    .unwrap()
                    .total_cmp(&b["value"].as_f64().unwrap())
            })
            .unwrap();
        assert!(peak["value"].as_f64().unwrap() > 50.0);
        assert!(samples
            .iter()
            .any(|s| s["name"] == "throttle" && s["value"].as_f64().unwrap() < 20.0));
        assert!(samples.iter().any(|s| s["name"] == "rpm"
            && s["t_ms"] == peak["t_ms"]
            && s["value"].as_f64().unwrap() > 3000.0));
        assert!(samples
            .iter()
            .filter(|s| s["name"] == "speed")
            .all(|s| s["value"] == 0.0));
        h.disconnect().await.unwrap();
        drop(m);
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn history_guide_is_discoverable_without_a_running_app() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let m = Mcp::new(
            Backend::Remote(
                serde_json::from_value(json!({"port":0,"cert":[],"token":"","data":""})).unwrap(),
            ),
            tx,
        );
        let list = rpc(
            &m,
            json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}),
        )
        .await;
        let guide = list["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["uri"] == "obd://history-guide")
            .unwrap();
        assert_eq!(guide["mimeType"], "text/markdown");
        let read = rpc(
            &m,
            json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":guide["uri"]}}),
        )
        .await;
        let content = &read["result"]["contents"][0];
        assert_eq!(content["uri"], guide["uri"]);
        assert_eq!(content["mimeType"], guide["mimeType"]);
        let text = content["text"].as_str().unwrap();
        for keyword in [
            "storage_error",
            "query_session",
            "export_recording",
            "save_fault_annotation",
        ] {
            assert!(text.contains(keyword), "guide is missing {keyword}");
        }
    }

    #[tokio::test]
    async fn resource_subscriptions_are_uri_specific() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let h = hub();
        let dir = std::path::PathBuf::from(h.status()["data_dir"].as_str().unwrap());
        let m = Mcp::new(Backend::Local(h.clone()), tx);
        for (id, method, uri) in [
            (1, "resources/subscribe", "obd://status"),
            (2, "resources/subscribe", "obd://live"),
            (3, "resources/unsubscribe", "obd://status"),
        ] {
            let result = rpc(
                &m,
                json!({"jsonrpc":"2.0","id":id,"method":method,"params":{"uri":uri}}),
            )
            .await;
            assert!(result.get("result").is_some(), "{result}");
        }
        let invalid = rpc(&m, json!({"jsonrpc":"2.0","id":4,"method":"resources/subscribe","params":{"uri":"obd://unknown"}})).await;
        assert_eq!(invalid["error"]["code"], -32602);
        m.tick().await;
        let notification: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(notification["params"]["uri"], "obd://live");
        assert!(rx.try_recv().is_err());
        drop(m);
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
