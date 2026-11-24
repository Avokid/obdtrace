//! Tool catalogue shared by the MCP server and the web UI.

use crate::hub::Hub;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Origin {
    Mcp,
    Ui,
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required})
}

fn tool(name: &str, description: &str, props: Value, required: &[&str]) -> Value {
    json!({"name": name, "description": description, "inputSchema": obj(props, required)})
}

fn page_limit(default: u16) -> Value {
    json!({"type": "integer", "minimum": 1, "maximum": 1000, "default": default})
}

pub fn definitions() -> Vec<Value> {
    let pids = json!({"type": "array", "items": {"type": "string"},
        "description": "Canonical sensor names (e.g. rpm, coolant_temp), not raw PID numbers."});
    let int = json!({"type": "integer"});
    let string = json!({"type": "string"});
    let session_id = json!({"type": "integer", "description": "Session ID from connection_status.session_id or list_sessions.sessions[].id."});
    let recording_id = json!({"type": "integer", "description": "Legacy name for session_id, not investigation_id."});
    let from_ms =
        json!({"type": "integer", "description": "Inclusive lower UTC epoch millisecond bound."});
    let to_ms =
        json!({"type": "integer", "description": "Inclusive upper UTC epoch millisecond bound."});
    let interval_ms = json!({"type": "integer", "minimum": 0, "description": "Background cycle interval in ms; capped at 60000. Actual cycles may take longer."});
    vec![
        tool("connection_status", "Start live diagnostics here. Check adapter.connected, target (mock means simulated), session_id and storage_error. last_dtc_scan is cached; its category timestamps describe when reads occurred, not when a fault began.", json!({}), &[]),
        tool("list_adapters", "Discover targets without connecting. Returns adapters (including the mock option) and warnings; partial discovery can still return usable targets. Ask the user to choose if ambiguous and pass the exact selected target to connect.", json!({
            "scan_bluetooth": {"type": "boolean", "default": true, "description": "Scan BLE and list paired classic Bluetooth SPP on Linux. Serial ports are listed either way."}
        }), &[]),
        tool("connect", "Open an ELM-compatible adapter connection and start an automatic session. Classic Bluetooth needs OS pairing; some BLE adapters do too. This does not pair devices. A valid target closes the existing session before opening the new adapter, even if opening ultimately fails; do not switch without user approval.", json!({
            "target": {"type": "string", "description": "Use the selected discovery target or a known manual target: ble:<name or id>, serial:<port>[@baud], Linux rfcomm:<MAC>[@channel] (default 1), tcp:<host>:<port>, or mock:audi_demo (simulation). Use bare ble only when automatic selection is intended."}
        }), &["target"]),
        tool("disconnect", "Stop sampling, flush buffered samples, and close the active adapter connection, session and any named investigation.", json!({}), &[]),
        tool("get_live_snapshot", "First choice for routine live values. No ECU traffic. Returns cached sensors with value, unit and age_ms; check freshness per sensor. Missing sensors are not zero. Requires an active connection.", json!({}), &[]),
        tool("read_live_data", "Request fresh ECU values; omit pids or pass [] to use the current sampling selection. Prefer a short explicit PID list. Check each sensors entry for value/unit or error even on MCP success. Successful reads are archived with source=direct.", json!({"pids": pids}), &[]),
        tool("list_supported_sensors", "Query the Mode 01 support bitmaps. Use supported_sensor_names for canonical sensor names. Advertised support does not guarantee a current reading.", json!({}), &[]),
        tool("read_dtcs", "Fresh ECU scan of stored, pending and permanent codes, plus readiness/MIL if readable. Check each category's status and read_at_ms; only status=ok with codes=[] confirms no reported codes for that category. no_data, unsupported or error are inconclusive; last_success_ms is not a fresh result. Archives the scan and may capture a frame for newly observed codes.", json!({}), &[]),
        tool("read_freeze_frame", "Read and archive historical ECU freeze-frame data. Check status, dtc_that_set_freeze_frame and per-sensor errors. The frame may be absent, partial or related to a different DTC; read_at_ms is collection time, not fault onset.", json!({}), &[]),
        tool("read_readiness", "Read current emissions monitor completion and ECU-requested MIL state. Not a complete inspection or pass/fail verdict. Avoid a duplicate read if read_dtcs already returned successful readiness.", json!({}), &[]),
        tool("read_vin", "Read masked vehicle identification (only the first 8 VIN characters are retained in output). Not a unique vehicle ID; use vehicle_id for archive matching.", json!({}), &[]),
        tool("clear_dtcs", "Destructive: clears codes/freeze frames and resets readiness. Never use as a diagnostic step. Requires OBDTRACE_ENABLE_CLEAR=1 on both the adapter owner and stdio process, confirm=true and human approval for the current vehicle session through client elicitation. Evidence is backed up before the command.", json!({"confirm": {"type": "boolean", "const": true}}), &["confirm"]),
        tool("set_sampling", "Change background sampling, not automatic session recording. Omitted options or an empty PID list retain the current settings for those fields; auto_record=false is rejected.", json!({
            "pids": pids, "interval_ms": interval_ms, "auto_record": {"type": "boolean", "const": true}
        }), &[]),
        tool("set_ring_buffer", "Set the in-memory window for recent samples and raw adapter traffic (initially 300 s). Does not limit persistent session history.", json!({
            "seconds": {"type": "integer", "minimum": 10, "maximum": 3600}
        }), &["seconds"]),
        tool("save_diagnostic_log", "Export recent ring-buffer evidence to local files (CSV/JSON and optional raw log), not the full archive. Returns file paths, sample count and seconds covered. Use query_session for analysis without writing export files.", json!({
            "duration_s": {"type": "integer", "minimum": 0, "default": 120, "description": "Requested recent seconds, capped by the ring window."},
            "label": {"type": "string", "description": "Legacy argument; not included in filenames or exported metadata."}, "include_raw": {"type": "boolean", "default": false, "description": "Explicitly include private raw adapter traffic for local troubleshooting. Do not share it without review."}
        }), &[]),
        tool("start_recording", "Open a named investigation inside the already recorded session. Returns session_id/recording_id and investigation_id; no duplicate recording. Optional PIDs and interval change ongoing background sampling, which is not restored by stop_recording.", json!({
            "label": {"type": "string", "default": "recording"}, "pids": pids, "interval_ms": interval_ms
        }), &[]),
        tool("stop_recording", "End the named investigation, not automatic history. Use the returned session_id, from_ms and to_ms with query_session to analyze the segment.", json!({}), &[]),
        tool("list_recordings", "Legacy list of the latest 200 sessions. Use list_sessions to page through all history.", json!({}), &[]),
        tool("get_recording_summary", "Per-sensor statistics for a whole session, not a named segment. Use query_session when transients or timing matter.", json!({"recording_id": recording_id}), &["recording_id"]),
        tool("query_recording", "Downsampled time series for quick plots, not evidence that a short transient was absent. Omit pids or pass [] for all recorded sensors. Use query_session for original rows.", json!({
            "recording_id": recording_id, "pids": pids,
            "from_s": {"type": "number", "default": 0, "description": "Inclusive lower bound in seconds relative to the first recorded sample."},
            "to_s": {"type": "number", "description": "Inclusive upper bound in seconds relative to the first recorded sample. Omit for no upper bound."},
            "max_points": {"type": "integer", "minimum": 0, "default": 200, "description": "Point cap per sensor; values below 1 are treated as 1."}
        }), &["recording_id"]),
        tool("compare_recordings", "Compare whole-session statistics for shared sensors. avg_delta_b_minus_a is the mean in B minus the mean in A; check matching operating conditions before interpreting it.", json!({
            "a": {"type": "integer", "description": "Baseline session ID."}, "b": {"type": "integer", "description": "Comparison session ID."}
        }), &["a", "b"]),
        tool("export_recording", "Write all samples of a session to a local CSV and return its path and row count.", json!({"recording_id": recording_id}), &["recording_id"]),
        tool("list_sessions", "List archived sessions by descending ID. Time filters apply to session start, not interval overlap or event times. dtc matches a code observed during the session, not its current state. Pass next_before_id as before_id with the same filters; null ends the result.", json!({
            "vehicle_id": int, "from_ms": from_ms, "to_ms": to_ms, "limit": page_limit(50),
            "before_id": {"type": "integer", "description": "Exclusive session ID cursor from next_before_id."},
            "state": string, "dtc": string, "annotation_status": {"type": "string", "enum": ["proposed", "confirmed", "rejected"]}, "problem_tag": string
        }), &[]),
        tool("get_session", "Read a session's lifecycle, bounds, vehicle_id, historical DTCs and named investigations. sample_coverage gives per-PID counts and first/last timestamps across the whole session; use query_session to verify coverage of a specific interval.", json!({"session_id": session_id}), &["session_id"]),
        tool("query_session", "Read samples as compact per-sensor series. Raw pages (default) are lossless: series[] has pid, unit, source and points [dt_ms, value], where t_ms = t0_ms + dt_ms; points are time-sorted within a series, while pages follow rowid order. Use next_after_rowid as after_rowid with the same filters until null. With bucket_ms, points are [dt_ms, n, min, avg, max] per time bucket starting at t0_ms (from_ms or session start); page with next_after_bucket as after_bucket. Use buckets to scan long ranges, then raw pages for short windows; min/max keep transients but not their exact timing. Omit pids or pass [] for all recorded sensors. Pin to_ms for an active session; check gaps and source before interpreting results.", json!({
            "session_id": session_id, "pids": pids, "from_ms": from_ms, "to_ms": to_ms, "limit": page_limit(500),
            "after_rowid": {"type": "integer", "description": "Exclusive row cursor from next_after_rowid (raw pages only)."},
            "bucket_ms": {"type": "integer", "minimum": 100, "maximum": 3600000, "description": "Aggregate to min/avg/max per bucket; limit then counts PID buckets."},
            "after_bucket": {"type": "integer", "description": "Exclusive bucket cursor from next_after_bucket (with bucket_ms)."}
        }), &["session_id"]),
        tool("get_session_events", "Read the archived timeline (including DTC scans, freeze frames, connection gaps and annotation revisions) in event-ID order. Pass next_after_id as after_id without changing filters until null. Event time is observation time, not necessarily fault onset.", json!({
            "session_id": session_id, "from_ms": from_ms, "to_ms": to_ms, "limit": page_limit(100),
            "after_id": {"type": "integer", "description": "Exclusive event cursor from next_after_id."}
        }), &["session_id"]),
        tool("list_vehicles", "List local vehicle profiles. Use their IDs to match history, not the masked VIN.", json!({}), &[]),
        tool("assign_session_vehicle", "UI-only: MCP calls are refused. Ask the user to select a local vehicle profile (or create one) in Session history.", json!({
            "session_id": session_id, "vehicle_id": int, "name": string
        }), &["session_id"]),
        tool("list_fault_annotations", "Read current assessment revisions for a session. Annotations are external interpretations, not sensor readings or ECU findings.", json!({"session_id": session_id}), &["session_id"]),
        tool("save_fault_annotation", "Write a proposed assessment with evidence inside the session and time range. Supply event_id or pid with the session_id for each reference. Updating requires both annotation_id and expected_revision. AI cannot confirm/reject proposals or overwrite reviewed assessments.", json!({
            "session_id": session_id, "from_ms": from_ms, "to_ms": to_ms, "annotation_id": int, "expected_revision": int,
            "title": string, "assessment": string, "problem_tag": {"type":"string", "description":"Stable label linking related assessments across sessions."},
            "status": {"type": "string", "const": "proposed", "default": "proposed"},
            "evidence": {"type": "array", "minItems": 1, "maxItems": 100, "items": obj(json!({"session_id": session_id, "event_id": int, "pid": string}), &["session_id"])}
        }), &["session_id", "from_ms", "to_ms", "title", "assessment", "problem_tag", "evidence"]),
        tool("find_similar_conditions", "Filter same-vehicle history, not diagnose recurrence. Each bucket needs every requested PID and all its recorded values in range. Returns intervals with per-PID coverage and a first page of events. Pass both next_cursor fields back unchanged until null. Check original samples, event pages and gaps; no interpolation or proof of simultaneous or continuous conditions.", json!({
            "vehicle_id": int, "from_ms": from_ms, "to_ms": to_ms, "limit": page_limit(100),
            "bucket_ms": {"type": "integer", "minimum": 100, "maximum": 60000},
            "min_duration_ms": {"type": "integer", "minimum": 0, "maximum": 86400000, "description": "Minimum span between the first and last observed readings, not continuous duration."},
            "after_session_id": {"type": "integer", "description": "Copy from next_cursor.after_session_id."},
            "after_bucket": {"type": "integer", "description": "Copy from next_cursor.after_bucket."},
            "conditions": {"type": "array", "minItems": 1, "maxItems": 20, "description": "Unique canonical PIDs with inclusive ranges in each sensor's output unit; min must not exceed max.",
                "items": obj(json!({"pid": string, "min": {"type": "number"}, "max": {"type": "number"}}), &["pid", "min", "max"])}
        }), &["vehicle_id", "conditions", "bucket_ms", "min_duration_ms"]),
    ]
}

fn names(args: &Value) -> Vec<String> {
    args["pids"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn id(args: &Value, key: &str) -> Result<i64> {
    args[key]
        .as_i64()
        .ok_or_else(|| anyhow!("missing integer argument '{key}'"))
}

pub fn clear_enabled_for_mcp() -> bool {
    std::env::var("OBDTRACE_ENABLE_CLEAR").is_ok_and(|v| v == "1")
}

/// Tools exposed when the stdio server runs with OBDTRACE_READ_ONLY=1.
pub(crate) const READ_ONLY_TOOLS: &[&str] = &[
    "connection_status",
    "list_supported_sensors",
    "get_live_snapshot",
    "read_live_data",
    "read_dtcs",
    "read_freeze_frame",
    "read_readiness",
    "read_vin",
    "list_vehicles",
    "list_sessions",
    "get_session",
    "get_session_events",
    "query_session",
    "list_fault_annotations",
    "list_recordings",
    "query_recording",
    "get_recording_summary",
    "compare_recordings",
    "find_similar_conditions",
];

pub fn allowed_for_mcp(name: &str) -> bool {
    std::env::var("OBDTRACE_READ_ONLY").as_deref() != Ok("1") || READ_ONLY_TOOLS.contains(&name)
}

#[derive(Clone, Copy)]
pub struct Ctx {
    pub origin: Origin,
    /// A human approved the action (UI dialog or MCP elicitation).
    pub confirmed: bool,
    /// The MCP side has OBDTRACE_ENABLE_CLEAR=1.
    pub mcp_clear_enabled: bool,
}

pub async fn call(hub: &Arc<Hub>, name: &str, args: Value, ctx: Ctx) -> Result<Value> {
    let Ctx {
        origin,
        confirmed,
        mcp_clear_enabled,
    } = ctx;
    if origin == Origin::Mcp && !allowed_for_mcp(name) {
        bail!("tool disabled in this MCP profile");
    }
    hub.note_tool(name, if origin == Origin::Mcp { "mcp" } else { "ui" });
    match name {
        "connection_status" => {
            let mut s = hub.status();
            if s["adapter"]["connecting"] == true {
                s["next_step"] = json!("A connection is in progress. Wait for it to finish before starting another connection attempt.");
            } else if s["adapter"]["connected"] != true {
                s["next_step"] = json!("No adapter connected. To connect, plug the adapter into the OBD-II port and switch on the ignition. If your client exposes list_adapters and connect, choose the intended target from discovery; otherwise use the app's Connect page. Archived observations are historical, not live.");
            }
            Ok(s)
        }
        "list_adapters" => Ok(hub
            .list_adapters(args["scan_bluetooth"].as_bool().unwrap_or(true))
            .await),
        "connect" => {
            hub.connect(
                args["target"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing 'target'"))?,
            )
            .await
        }
        "disconnect" => {
            hub.disconnect().await?;
            Ok(json!({"disconnected": true}))
        }
        "get_live_snapshot" => hub.live_snapshot().await,
        "read_live_data" => hub.live(&names(&args)).await,
        "list_supported_sensors" => hub.supported().await,
        "read_dtcs" => hub.dtcs().await,
        "read_freeze_frame" => hub.freeze_frame().await,
        "read_readiness" => hub.readiness().await,
        "read_vin" => hub.vin(false).await,
        "clear_dtcs" => {
            if args["confirm"].as_bool() != Some(true) {
                bail!("clear_dtcs needs confirm=true");
            }
            if origin == Origin::Mcp && !mcp_clear_enabled {
                bail!("clearing codes over MCP is disabled. Read the codes first, then clear them from the app (Trouble codes > Clear) or start the server with OBDTRACE_ENABLE_CLEAR=1.");
            }
            if !confirmed {
                bail!("no human confirmation received; clear_dtcs was not executed");
            }
            hub.clear_dtcs(id(&args, "session_id")?).await
        }
        "set_sampling" => hub.set_sampling(
            Some(names(&args)),
            args["interval_ms"].as_u64(),
            args["auto_record"].as_bool(),
        ),
        "set_ring_buffer" => hub.set_ring_buffer(
            args["seconds"]
                .as_u64()
                .ok_or_else(|| anyhow!("missing 'seconds'"))?,
        ),
        "save_diagnostic_log" => hub.save_diagnostic_log(
            args["duration_s"].as_u64().unwrap_or(120),
            args["label"].as_str().unwrap_or(""),
            args["include_raw"].as_bool().unwrap_or(false),
        ),
        "start_recording" => {
            hub.start_recording(
                args["label"].as_str().unwrap_or(""),
                Some(names(&args)),
                args["interval_ms"].as_u64(),
            )
            .await
        }
        "stop_recording" => hub.stop_recording().await,
        "list_recordings" => hub.list_recordings(),
        "get_recording_summary" => hub.recording_summary(id(&args, "recording_id")?),
        "query_recording" => hub.recording_query(
            id(&args, "recording_id")?,
            &names(&args),
            args["from_s"].as_f64().unwrap_or(0.0),
            args["to_s"].as_f64(),
            args["max_points"].as_u64().unwrap_or(200) as usize,
        ),
        "compare_recordings" => hub.recording_compare(id(&args, "a")?, id(&args, "b")?),
        "export_recording" => hub.recording_export(id(&args, "recording_id")?),
        "list_sessions"
        | "get_session"
        | "query_session"
        | "get_session_events"
        | "list_vehicles"
        | "assign_session_vehicle"
        | "list_fault_annotations"
        | "save_fault_annotation"
        | "find_similar_conditions" => hub.history(name, &args, origin == Origin::Mcp),
        other => bail!("unknown tool '{other}'"),
    }
}
