# Compare the symptom after code clearing

Context: vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

You, the calling external model, make the assessment. This app only collects, filters and stores evidence. Verify vehicle identity using `get_session`. Use `list_sessions` with `vehicle_id` and follow `next_before_id` as `before_id` to reach all relevant sessions, including the active one.

Use `list_fault_annotations` and `get_session_events` to locate the earlier symptom and the actual `clear_attempt`/`clear_result`. Page events with `after_id` from `next_after_id`. A lost acknowledgement is an uncertain clearing outcome, and a code disappearing after a separate app's clearing is not a confirmed clearing event here.

Read the earlier interval using `query_session` with `session_id`, `from_ms`, `to_ms`, chosen `pids`, and `after_rowid` from `next_after_rowid`. YOU choose which measured conditions matter and their tolerances, before looking for recurrence.

Use `find_similar_conditions` with `{"vehicle_id": VEHICLE, "from_ms": AFTER_CLEAR, "conditions": [{"pid": "rpm", "min": MIN_RPM, "max": MAX_RPM}], "bucket_ms": WIDTH_MS, "min_duration_ms": SPAN_MS, "limit": 50}`. This is a schema example, not suggested thresholds. Derive actual ranges and other relevant PIDs from the earlier samples. Bucket width is 100-60000 ms. It controls temporal coarseness, not proven simultaneity. Follow `next_cursor` by passing its `after_session_id` and `after_bucket` fields.

For each match, inspect per-PID coverage and max_gap_ms, and fetch original samples and full events for that interval. The filter only tests recorded values in matching buckets. A reported observed span is not proof of continuously comparable conditions; missing sensors or sparse readings weaken evidence. Query alternate windows if the onset was between polls.

Distinguish: the DTC was observed again; the live-data symptom appears again in YOUR assessment; comparable conditions were observed without evidence of that symptom; or evidence is insufficient. Report sample durations, gaps and actual DTC read statuses. No matching interval does not establish a repair. An empty successful DTC read after clearing, unknown read statuses or incomplete readiness monitors do not prove a fix. Permanent codes need not disappear immediately.

Explain with source references. If justified, persist YOUR proposed assessment with `save_fault_annotation`, including `session_id`, `from_ms`, `to_ms`, `title`, `assessment`, the same `problem_tag` and valid `evidence` references in that session. Never overwrite user-reviewed annotations. Do not clear codes again or ask the driver to recreate hazardous conditions. Interact only while parked.
