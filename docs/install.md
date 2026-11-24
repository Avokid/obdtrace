# Install and update OBDTrace

## Ready-to-run archives

Use [GitHub Releases](https://github.com/Avokid/obdtrace/releases) for published
downloads. If no release is available, see the [README](../README.md#install-and-try-the-demo)
for build and CI options. The first release is Linux x86-64 (`.tar.gz`);
Windows x64 and Apple silicon macOS are test targets, not release downloads. See the
[Linux dependency list](linux-dependencies.md). Check the release
notes for tested environments and signing status. An automated demo test does
not certify physical adapters.

For current source builds, start the UI with `obdtrace app` rather than a
bookmark. With `--no-browser`, open the private launch file printed in the
terminal. On Unix an existing data directory must belong to you and have
mode `0700`. If upgrading, check the selected directory first and remove
group and other access from that directory only. On Windows keep data in a
private user profile. Do not copy `.obdtrace-runtime` to another installation.

Compare the archive hash with `SHA256SUMS` using `sha256sum` on Linux,
`shasum -a 256` on macOS, or `Get-FileHash -Algorithm SHA256 <archive>` in
PowerShell. Extract to a folder you can keep across updates. No Rust, Node.js
or Python installation is needed to run the binary.

- **Linux:** The release is built on Ubuntu 22.04. A compatible glibc system
  with D-Bus and libudev is required (Debian/Ubuntu runtime packages:
  `libdbus-1-3`, `libudev1`). Bluetooth requires a running BlueZ service
  and access to the adapter; USB serial requires permission for the device.
- **Windows:** Use the x64 build and the normal Windows Bluetooth or serial
  driver. Classic Bluetooth needs an outgoing COM port. If Windows reports
  a missing MSVC runtime, install the official x64 Microsoft Visual C++
  Redistributable.
- **macOS:** Use the Apple silicon build. Grant Bluetooth permission to
  your terminal or the app hosting the process if requested. Classic
  Bluetooth serial requires a port exposed by macOS.

Use your platform's normal approval process for downloaded software. Do not
disable security controls to run a release your system does not trust.

## First use and updates

Try `obdtrace demo` first (`./obdtrace` in a Unix shell or `.\obdtrace.exe`
in PowerShell when using the extracted folder). Then stop the demo and use
`obdtrace app` for a real connection. On **Connect → Set up AI assistant**,
choose your client and copy the shown snippet into its MCP configuration;
OBDTrace does not edit client files itself. Confirm the setup by calling MCP
`connection_status`. For manual configuration the command is the absolute
path to the executable with one argument, `mcp`. `obdtrace-demo` is a
suggested profile name for a separate, manually configured demo instance;
use a different `OBDTRACE_PORT` and `OBDTRACE_DATA_DIR` to isolate it.

For regular updates, stop the app and MCP processes, back up your data and
replace the executable at the same path. If your path changes, update the MCP
configuration or regenerate the setup from the Connect page.

## Migrate from `obd2-mcp`

The rename is intentional: old commands, configuration keys and data locations
are not automatically migrated or recognized. The default port remains 8765.

1. Disconnect in the old app and stop the app and every MCP process that
   uses it. Keep a private backup of the old directory (`~/obd2-mcp`,
   `%USERPROFILE%\obd2-mcp`, or the old `OBD2MCP_DATA_DIR`).
2. Before starting OBDTrace, copy `recordings.db` and its original
   `vehicle.key` together into an empty `~/obdtrace` (Windows:
   `%USERPROFILE%\obdtrace`), or an empty directory selected with
   `OBDTRACE_DATA_DIR`. Keep the same access restrictions; do not overwrite
   existing OBDTrace data or mix keys from different installations. You can
   also copy existing `logs` and `exports` directories. Keep the backup until
   you verify that history and vehicle profiles are present.
3. Replace any needed `OBD2MCP_` environment variable prefixes with
   `OBDTRACE_`. Update the MCP command path to the new executable and rename
   the entry to `obdtrace`. Alternatively, copy a new entry from the Connect
   page and remove
   the obsolete entry yourself. Do not reuse the previous server process.
4. Open OBDTrace, verify Session history, and test `connection_status`.
   Do not attach the database, vehicle key, or private logs to a public issue.
