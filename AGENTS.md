# Developing OBDTrace with agents

These guidelines apply to contributors and coding agents working in this
repository. For agents **using** the running MCP server to diagnose a vehicle,
see the [MCP agent guide](docs/mcp.md) and [history playbook](docs/mcp-history.md).

## Contribution workflow

- Use English for source comments, commit messages, pull requests, and primary
  documentation. Keep user-facing translations in `assets/locales/` and
  localized history prompts in `docs/prompts/` in sync when changing shared
  wording, keys, or placeholders.
- Start from the issue or agreed scope. Inspect the existing code and tests
  before editing. Keep diffs focused, preserve unrelated work, and use small
  commits that explain why the change was made.
- Treat agent output as a draft. The contributor reviews the entire diff,
  verifies behavior, and owns the result. In a pull request, explain the
  user-visible change, how it was tested, and any unverified platforms or
  hardware. Do not equate simulator results with physical adapter testing.
- Do not rewrite shared history, tag releases, publish packages, or change
  repository visibility as a side effect of code changes. Follow the
  [release checklist](docs/releasing.md) for publication.

## Project boundaries

- OBDTrace is a standalone Rust binary with an embedded browser UI and a local
  stdio MCP server. The app collects observations; external clients interpret
  them. Keep one owner of the adapter and preserve automatic session history.
  `src/hub.rs` coordinates connection and sampling, `src/transport/` handles
  adapters, and `src/history.rs` and `src/store.rs` handle persistence.
- When changing an MCP operation, keep the handler, the discovery schema
  in `src/tools.rs`, the agent instructions in `src/mcp.rs`, and the relevant
  documentation aligned.
  Preserve tool names, argument units, pagination cursors, and error semantics
  unless the change explicitly requires a documented migration. Agents should
  use the available MCP tools for diagnostics, not bypass client limits using
  the internal HTTP bridge or database.
- Keep Copilot CLI's `obdtrace-auto` profile limited to diagnostics and history
  reads. Do not weaken the code-clearing safeguards or turn on live actions
  implicitly. Keep observations, missing data, and AI assessments distinct.
- Use synthetic fixtures for tests and examples. Never commit or attach a real
  VIN, `vehicle.key`, diagnostics database, private logs, or credentials.
- Never commit run logs, CI logs, adapter logs, exports or database files
  (`*.log`, `*.db*`, `*.sqlite*`, `logs/`, `exports/`), even synthetic ones.
  CI rejects them. Inspect downloaded logs outside the repository and delete them.

## Verify changes

Follow the [build prerequisites](README.md#build-from-source). Run a focused
Rust test for behavioral changes, and use `cargo test --locked` before handing
off changes that touch multiple components. For MCP or UI changes, exercise
the affected flow with `obdtrace doctor` or `obdtrace demo --no-browser` as
appropriate. Use a temporary `OBDTRACE_DATA_DIR` and an unused `OBDTRACE_PORT`
so tests do not touch anyone's history or running app. Test on a real adapter
only with explicit approval and while parked.
