# MCP history and saving playbook

## What “save” means

| What the user wants | MCP operation |
| --- | --- |
| Keep vehicle measurements | Automatic while connected; nothing to start. `connection_status` shows `session_id`, sampling settings and `storage_error`. Verify the requested data with `query_session`. |
| Mark the start and end of an investigation | `start_recording` opens a named interval; `stop_recording` ends it and returns `session_id`, `from_ms` and `to_ms`. Neither controls automatic history. Do not pass `investigation_id` in place of `session_id`. |
| Change which sensors are collected | `set_sampling` changes future background readings. It cannot fill historical gaps. If supplying PIDs or an interval to `start_recording`, those settings persist after `stop_recording`. |
| Save recent evidence to files | `save_diagnostic_log({"duration_s":120,"include_raw":false})` writes recent ring-buffer evidence to local files. Check the returned `files`, `samples`, `seconds_covered` and `note`; the memory window and actual readings limit coverage. Raw traffic is excluded unless explicitly requested. |
| Export a whole session | `export_recording({"recording_id":42})` writes sample rows to a local CSV and returns `file` and `rows`. `recording_id` is the session ID. This is not a DTC timeline or a complete database backup. |
| Save the agent's interpretation | `save_fault_annotation` stores a local proposal linked to original session evidence. It does not modify measurements or ECU state. |

Use only operations exposed by your MCP client. The default Copilot CLI
`obdtrace-auto` profile supports diagnostic and history reads, not sampling changes,
named investigations, exports or annotation writes. Automatic recording still
works. If the requested operation is unavailable, state that explicitly; do
not emulate it with shell commands, SQL or internal HTTP. Export paths refer
to the server's filesystem. Do not assume the client can open them or share
them externally.

## Verify that measurements were saved

1. For a live request, use `connection_status`. Check the session ID, sampling
   settings and `storage_error`; an active adapter does not prove that the ECU
   is answering or that storage is healthy. If no session is active, distinguish
   previously archived data from any new capture.
2. Query the intended `session_id` with the requested PIDs and time bounds using
   `query_session`. Check for original rows in the intended interval, not just
   `storage_error:null` or a populated live snapshot. Report the session ID,
   actual first/last sample times, sensors found and material gaps. If a
   history call reports a storage error, do not claim the capture was saved.
3. If a measurement is needed **now** and isn't already available, a short
   `read_live_data` request adds successful readings to the active session as
   `source:"direct"`. It cannot recover an earlier moment. `get_live_snapshot`
   only reads cached values.

## Investigate a past symptom

1. Ask for the approximate time, local time zone if needed, and the vehicle
   when ambiguous. Convert the window to UTC epoch milliseconds. Use
   `list_vehicles` and `list_sessions` to identify a local `vehicle_id` and
   session. Match the window against `started_ms` and `ended_ms` yourself:
   `list_sessions` time filters only match session starts, so setting
   `from_ms` to the symptom time can exclude a session already underway.
   Never join vehicles by masked VIN alone.
2. Use `get_session` for lifecycle and `sample_coverage` before querying.
   Coverage counts and first/last times describe the whole session; they
   do not prove the PID was sampled during the symptom window.
3. Use `query_session` for relevant original readings and `get_session_events`
   for DTC observations, freeze frames, connection/storage gaps and annotation
   revisions. For a long window, scan first with `bucket_ms` (per-bucket
   min/avg/max), then read raw points for the interesting minutes. Follow
   `next_after_rowid` as `after_rowid` (or `next_after_bucket` as
   `after_bucket`) and `next_after_id` as `after_id`, keeping the filters
   fixed. Start with a bounded window and pin `to_ms` for an active session.
   Timestamps are `t0_ms + dt_ms`; sensors are not necessarily sampled
   simultaneously. Inspect earlier events when
   needed for context. A code's first observation is not the time it began.
4. Compare with a baseline for the same vehicle and similar operating
   conditions. `find_similar_conditions` identifies candidate intervals
   using explicit PID ranges; inspect original rows and events before drawing
   conclusions. Report whether a code was observed again, whether the
   symptom is supported by comparable measurements, or whether the evidence
   is insufficient. No rows, missing PIDs, or no comparable intervals do not
   mean no fault. Keep simulated sessions separate; if provenance is unclear,
   do not claim the data comes from a real vehicle.

## Save an assessment with evidence

Only save an assessment if requested and `save_fault_annotation` is exposed.
Use references you actually retrieved: an event `id` from `get_session_events`
or a sample `name` as `pid` from `query_session`. Both must belong to the same
session and interval as the proposal. Example arguments (replace IDs, times
and text with observed evidence):

```json
{
  "session_id": 42,
  "from_ms": 1774000000000,
  "to_ms": 1774000030000,
  "title": "Reported rough idle",
  "assessment": "RPM varied during the reported interval. The cause was not established.",
  "problem_tag": "rough-idle",
  "evidence": [{"session_id": 42, "pid": "rpm"}]
}
```

Omitting `status` creates a `proposed` assessment. Retain the returned `id`
and `revision`. For an edit, read `list_fault_annotations`, then provide the
latest `id` as `annotation_id`, `revision` as `expected_revision`, and the
full updated assessment and evidence. If it changed meanwhile, read the
current revision and reconcile rather than retrying blindly. Only a user in
the app can confirm or reject an assessment; an agent cannot overwrite a
reviewed one. Report the saved proposal separately from measured evidence.
