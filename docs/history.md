# Session history and external AI analysis / Sessiohistoria

For live diagnostics, adapter discovery and worked MCP queries, see the
[MCP agent guide](mcp.md). For verifying saved data, exporting it and writing
evidence-linked assessments, see the [MCP history and saving playbook](mcp-history.md).

## What runs where

The application has **no AI model, inference engine or AI-service API calls**.
It collects OBD-II observations, stores them locally and exposes deterministic MCP
queries. Your external MCP client (Claude, Copilot, etc.) chooses what to investigate,
interprets the evidence and can explicitly save its assessment back to the app.
Nothing in the UI invokes a model automatically.

## Automatic history

Every successful connection starts a persistent SQLite session, even without an AI
client or a `start_recording` call. Samples are flushed in batches. A normal
disconnect or Ctrl+C flushes and closes the session; after a crash, the next owner
marks it interrupted at the last persisted observation. The final uncommitted batch
can be lost on a crash. Only one process can own the data directory at a time.

A verified reconnect to the same VIN continues the session with gap/restoration
events. If identity cannot be verified, reconnect stops: connect explicitly to
start a new session. An unidentified session remains searchable but does not
silently join another vehicle's history.

`start_recording` and `stop_recording` now delimit **named investigations inside
the automatic session**. Their `recording_id` is the enclosing `session_id`;
`investigation_id` identifies the named segment. `stop_recording` also returns
`from_ms`/`to_ms` for `query_session`. Stopping an investigation does not stop
history. Old recording IDs and samples are preserved during migration.

`set_sampling(auto_record=false)` is rejected rather than silently disabling
history. Sensor choices and cycle intervals affect what can later be evaluated.
Only sampled data can be retrieved; generic OBD-II is not a high-frequency
oscilloscope and cannot guarantee capture of brief faults.

There is **no automatic history deletion**. Watch disk space and privately back up
the data directory. `storage_error` in `connection_status` and the live UI warns of
archive failure; the live connection alone does not mean data is being saved.
Failed batches remain buffered up to 10,000 samples. Beyond that limit, data can be
lost and a storage-gap event is written after recovery. Restore space/permissions
before relying on further recordings. A connection switch is refused if the
previous batch cannot be secured.

The default directory is `~/obdtrace` (`%USERPROFILE%/obdtrace` on Windows), or
`OBDTRACE_DATA_DIR`. The database is `recordings.db`. Raw adapter traffic remains a
bounded in-memory ring, not an unlimited archive.

## Vehicle identity and privacy

The complete VIN is read into memory to compute HMAC-SHA-256 using a random local
32-byte key (`vehicle.key`). The database contains only the HMAC and masked VIN;
public tools and diagnostic bundles expose neither the HMAC nor the key.
Different VINs with the same masked prefix remain separate vehicles.

This is **pseudonymisation**, not anonymity. A private backup of both the database
and original key is needed to recognise vehicles after restoration. Never attach
either to a public issue. Unix key files are created with mode 0600; on Windows
keep the data directory in your private user profile with restricted inherited
permissions. Missing/corrupt keys are not silently replaced when identified
vehicles already exist. Restore the original key from your private backup.
Different installations/keys do not produce interchangeable VIN identifiers.

Without a valid VIN, choose an existing local profile or create one in Session
history. `assign_session_vehicle` performs that explicit operation; old sessions
are not inferred to belong to a car from their masked VIN.

## DTC observations and clearing

DTCs are read on connect, with a **30-second target interval**, and before/after
clearing from this application. The same ELM connection serialises requests, so a
slow adapter or long sampling cycle can extend the interval. The UI shows the last
scan's age; it is not an instantaneous push notification from the ECU.

Stored/pending/permanent categories each retain their own read timestamp and
status: `ok`, `no_data`, `unsupported` or `error`. Only `ok` with an empty codes
array confirms an empty result for that category at that time. The observation
timestamp is not necessarily the fault onset time. A DTC present on first
connection may predate the entire recording.

Newly observed codes trigger a freeze-frame read. Availability and partial errors
are recorded; not every code has a frame and an exact ECU onset timestamp is not
promised. DTC presence is not a component diagnosis.

Clearing requires the existing UI or MCP human-confirmation safeguards. The app
archives fresh DTCs, available freeze-frame data and pending samples, and creates a
diagnostic backup **before** sending the command. Archive/backup failure prevents
the command. Attempts and acknowledgement/uncertainty are recorded, followed by
a new DTC scan. History and assessments are never erased by clearing.
Permanent codes may remain. Clearing elsewhere cannot be reliably identified.

## Searching and assessments

All times in history tools are UTC epoch milliseconds. `list_sessions` returns
`sessions` plus `next_before_id`; pass it as `before_id` for the next page. Filters
include `vehicle_id`, `from_ms`, `to_ms`, `dtc`, `state` and `annotation_status`.
`get_session` returns bounds, historical DTCs and named investigations.

`query_session` returns original, non-downsampled samples as compact per-sensor
series (`points` are `[dt_ms, value]` from `t0_ms`). Follow
`next_after_rowid` as `after_rowid`, preserving your filters. With `bucket_ms`
it returns per-bucket `[dt_ms, n, min, avg, max]` instead and pages with
`next_after_bucket`/`after_bucket`; see the [MCP guide](mcp.md). `get_session_events`
uses `next_after_id` as `after_id`. Page limits are 1-1000.
Legacy `list_recordings` still shows the latest 200 sessions; it is not the full
history interface. You can also filter sessions by the shared `problem_tag`.
Legacy summary/comparison tools describe whole sessions, not
only named investigations, and their aggregates are not proof that no transient
occurred. Use the original bounded `query_session` data for investigation.

For a demo, connect `mock:audi_demo` (also used by the app's demo button and the
`demo` CLI command). Each new demo session includes 32 seconds of simulated
history, sampled twice per second across every sensor the demo car advertises
(including a wideband upstream and a narrowband downstream oxygen sensor). A `demo_seed` event describes the fixture and seeded samples use
the `demo_seed` source. The car stays stationary while the throttle, RPM, load
and related signals rise and fall. Live simulated sampling continues after the
fixture. This is not an Audi-specific diagnostic dataset (e.g. ABS, airbag,
or proprietary VAG signals are not included).

Find the session with `list_sessions`, then page through `query_session`. For
oxygen-sensor history, check `list_supported_sensors` first: narrowband
sensors report `o2_bXsY_voltage` (and `o2_bXsY_stft`), wideband sensors report
`o2_bXsY_lambda` with `o2_bXsY_current` or `o2_bXsY_wr_voltage`, and
`o2_sensor_locations` maps each prefix to the bank and sensor position the ECU
reports. To compare the requested and measured mixture, query
`pids: ["commanded_lambda", "o2_b1s1_lambda", "o2_b1s2_voltage"]` with
`bucket_ms` and `layout: "table"`, which returns one row per bucket with one
average column per sensor.

`save_fault_annotation` stores an external assessment with session/time/PID or
event evidence. `list_fault_annotations` returns current revisions.
AI writes are `proposed`; the UI user may edit, confirm or reject them. Original
AI/user provenance stays intact and `updated_by` records who changed the revision. Updates
require `expected_revision` to avoid overwriting newer work. Reviewed assessments
cannot be overwritten by an AI call. Full revisions remain on the event timeline.
A shared `problem_tag` links assessments conceptually across sessions; the same
code alone does not establish a shared root cause.

`find_similar_conditions` accepts explicit PID `min`/`max` ranges chosen by the
external model, a vehicle ID, `bucket_ms` (100-60000) and `min_duration_ms`. Every
requested PID must occur in each matching bucket and all its recorded values must
be in range. Consecutive matching buckets are grouped before pagination/duration
filtering; pages do not split qualifying intervals. Use `next_cursor` fields
`after_session_id`/`after_bucket` to continue with identical filters.

Results contain observed spans, per-PID counts/times/ranges and maximum sampling
gaps, plus paginated events. They contain **no diagnosis or recurrence verdict**.
No interpolation is performed; a span is not proof of continuous conditions.
Different PIDs are usually sampled at different times. Sparse readings, missing
sensors and gaps must be considered by the calling model; consult original data.
History can grow while paging an active session: repeat a bounded query for newly
arrived observations rather than treating an earlier page as the final record.

## Documented prompts in the UI and MCP

These templates are embedded in the binary at build time. The English sources
are linked below; translations for the other UI languages live next to them in
[`assets/prompts`](../assets/prompts).

| Workflow | Template |
| --- | --- |
| Identify a symptom | [identify_symptom.en.md](../assets/prompts/identify_symptom.en.md) |
| Describe earlier conditions | [explain_previous_fault.en.md](../assets/prompts/explain_previous_fault.en.md) |
| Compare after clearing | [compare_after_clear.en.md](../assets/prompts/compare_after_clear.en.md) |

Open a history session, choose the time range and optionally an event/assessment,
preview a prompt, then copy it to your external AI client. Only local IDs and time
bounds are inserted. Missing selections remain explicitly marked.
The same templates are available through MCP `prompts/list` and `prompts/get`,
with optional `language`, `vehicle_id`, `session_id`, `from_ms`, `to_ms`, `event_id`
and `annotation_id` arguments. `language` defaults to `en` and accepts `fi`, `es`,
`de`, `fr` and `pt-BR` as well; regional tags such as `fr-CA` use the matching
base language when available.

The external model must distinguish:

- a DTC observed again;
- its own assessment that a live-data symptom reappeared;
- comparable measured conditions without observed evidence of that symptom;
- insufficient comparable evidence.

Empty DTCs just after clearing do not prove a repair. Incomplete readiness can
mean the relevant test has not run. No matching conditions means lack of
comparison evidence, not proof of a fix. Interact only while parked; never recreate
a dangerous condition for the AI.
