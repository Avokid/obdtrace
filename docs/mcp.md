# OBD-II MCP agent guide

## Start with the tools your client exposes

Use the OBDTrace MCP tools directly, not shell commands, the app's internal
bridge or SQL. The [Connect page](../README.md) sets up the client. The
`obdtrace-auto` profile created for Copilot CLI is diagnostic and history only:
it does not expose connection management, sampling changes, exports, annotation
writes or code clearing. Do not try to bypass a missing tool. Choose the intended
server before making calls; a mock `target` or `demo_seed` data is simulated
and must not be used as evidence about a real car.

For manual configurations, including Claude Code, set `OBDTRACE_DATA_DIR` to
the owner's private directory if it is not the default, and use the same
`OBDTRACE_PORT`. If the owner restarts, restart the stdio client connection
to trust the new instance. Never copy a runtime credential into client settings.

Start live work with `connection_status({})`. Check `adapter.connected`,
`target`, `session_id` and `storage_error`. A connected adapter does not prove
the ECU is responding or the archive is being written. If a connection is in
progress, wait for it to finish. If disconnected:

1. If the user wants to connect and both tools are available, call
   `list_adapters({})`, check `warnings` even when `adapters` is nonempty,
   and pass the chosen adapter's exact `target` to `connect`. Ask the user
   to choose if several devices are plausible. The simulated adapter is
   always listed; do not choose it for a real-car request.
2. Otherwise use the app's **Connect** page. Classic Bluetooth needs
   OS pairing first; some BLE adapters do too. See the [adapter table](../README.md#adapter-connection-targets)
   for manual targets. Do not switch an active connection without approval:
   a valid new target can close the old session even if the new connection fails.
3. If a live connection is unavailable, use archive tools when appropriate.
   Label results with their session and time; do not present them as current.

For MCP client implementers: `initialize` returns agent instructions and
`tools/list` returns the input schemas. Tool results use `content[0].text`;
if `isError` is false, parse the text as JSON. If `isError` is true, the text
describes the failure. Inspect sensor/category errors inside successful results
as well. Clients may also expose `obd://status`, the cached `obd://live`
snapshot, and the guided MCP prompts.

## Live diagnostics

- **Current values:** Start with `get_live_snapshot({})`, which makes no
  adapter requests. Check `age_ms` and `unit` for each sensor. If relevant values
  are stale or missing, use a short explicit request, e.g.
  `read_live_data({"pids":["rpm","intake_manifold_pressure"]})`.
  Inspect each sensor for `error`; missing or failed readings are not zero.
  `list_supported_sensors` reports advertised names, not guaranteed readings.
- **Codes and warning light:** Call `read_dtcs({})` for fresh observations
  rather than relying on `connection_status.last_dtc_scan`. For each of
  `stored`, `pending` and `permanent`, only `status:"ok"` with `codes:[]`
  means that no codes were reported in that category at `read_at_ms`.
  `last_success_ms` may be older than a failed attempt. Check `readiness`
  separately for monitor completion and the ECU-requested MIL state; use
  `read_readiness` if only readiness is needed.
- **Freeze frame:** A code newly observed by a scan may already have an
  associated `freeze_frame` event in the archive. Use `read_freeze_frame`
  when additional evidence is needed. Verify the associated DTC and field
  availability. A frame is not a live reading, and collection time is
  not the time the fault began.

Do not clear codes as part of diagnosis. Clearing erases evidence and resets
readiness. Report timestamps, units, gaps and what was not measured. A DTC
indicates a failed test, not a failed part. Generic OBD-II does not establish
the health of every vehicle module. For example, an unloaded rev does not
establish whether a turbo meets its boost target under load; the generic
`intake_manifold_pressure` reading is absolute, not gauge boost pressure.
Do not request driver interaction while moving or ask the user to reproduce
dangerous conditions. Refer safety-critical issues to a qualified professional.

## Use the automatic session archive

For a step-by-step workflow for capture verification, archived symptoms,
exports and saving assessments, see the [history and saving playbook](mcp-history.md).
MCP clients that expose resources can read it at `obd://history-guide`.

`start_recording` is not needed to collect samples. Each successful connection
creates an automatic session. Named investigations, if available, merely mark
a time span in that session. Fresh sensor/DTC reads can also write observations
locally without modifying the ECU. Use the `vehicle_id` when comparing sessions;
an unidentified session must not be matched by masked VIN alone.

1. Use `connection_status.session_id` for the connected session or find one
   with `list_sessions({"vehicle_id":7,"limit":10})`. Its time filters select
   session **starts**, not overlap or event times. Use `get_session` for metadata.
2. Select a few sensors and a short time window around the reported symptom.
   For example, call `query_session` with the following arguments (replace
   the IDs and UTC epoch-millisecond bounds with values from the actual case):

   ```json
   {"session_id":42,"pids":["rpm","engine_load"],"from_ms":1774000000000,"to_ms":1774000030000,"limit":200}
   ```

3. Query `get_session_events` for the same bounds to find DTC observations,
   freeze frames, connection gaps and other context. Check the `source` of
   each series; different PIDs are not necessarily sampled together.

`query_session` returns compact per-sensor series instead of one object per
sample. Reconstruct each timestamp as `t0_ms + dt_ms`:

```json
{"t0_ms":1774000000000,"rows":3,"downsampled":false,"next_after_rowid":null,
 "series":[{"pid":"rpm","unit":"rpm","source":"sampler","points":[[0,812.0],[500,815.0]]},
           {"pid":"engine_load","unit":"%","source":"sampler","points":[[250,21.6]]}]}
```

Raw pages are lossless: each `(pid, unit, source)` gets its own series,
points are time-sorted within a series, and `rows` counts samples on the
page. For long windows, add `"bucket_ms":10000` first. Points then become
`[dt_ms, n, min, avg, max]` per bucket from `t0_ms` (`from_ms` or the
session start), series list their `sources`, and `next_after_bucket` pages
on whole buckets via `after_bucket`. Minimum and maximum keep short spikes
visible, but locate and verify them with a narrow raw query.

| Tool | Returned cursor → next argument | Default page size |
| --- | --- | --- |
| `list_sessions` | `next_before_id` → `before_id` | 50 |
| `query_session` | `next_after_rowid` → `after_rowid` (raw) or `next_after_bucket` → `after_bucket` (with `bucket_ms`) | 500 samples or PID buckets |
| `get_session_events` | `next_after_id` → `after_id` | 100 |
| `find_similar_conditions` | `next_cursor` fields → top-level `after_session_id` and `after_bucket` | 100 |

Page sizes are 1–1000. Keep filters fixed and follow cursors until `null`
before drawing conclusions about the whole window. For active sessions, pin
`to_ms` while paging; rerun the bounded query if late writes could matter.
Use original samples for brief transients: the legacy `query_recording`
downsamples, while `get_recording_summary` and `compare_recordings` aggregate
whole sessions.

For a recurrence question, use `find_similar_conditions` with explicit PID
ranges in the sensor's units and a known `vehicle_id`. Inspect per-PID coverage
and gaps, then check original samples and events for the returned intervals.
Embedded `events` are only the first page: follow their `next_after_id` with
`get_session_events` over the same session and time bounds. Matching buckets
show recorded values within ranges, not continuous conditions or proof of the
same underlying fault. If `save_fault_annotation` is exposed and the user wants
an assessment saved, cite actual event IDs or sample PIDs within the interval.
The output is a proposal, not measured evidence or a confirmed diagnosis.

See [session history and privacy](history.md) for storage, annotation revisions
and additional history examples.
