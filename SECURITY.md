# Security policy

OBDTrace is a local, single-user diagnostics application. Release binaries are
tested against the built-in simulator; real-adapter support is best-effort.
Do not run it on a shared host or expose the local port via a proxy, tunnel or
container port mapping.

The browser requires a per-run credential, including
for WebSocket subscriptions. MCP uses a separate loopback TLS connection;
clients trust only the instance certificate from their private runtime file
and send their credential only after validating it. Both endpoints must use
the same data directory. No credential belongs in an MCP config or issue.
Keep the data directory and its parent under your account's control (on
Windows, use your private user profile with its normal restrictive ACL).
Administrator access, malware running as your user, browser extensions and
disclosure of the private launch file are outside this protection.

OBDTrace does not write MCP client configurations; you add the server to your
client yourself. Set `OBDTRACE_READ_ONLY=1` in the server environment to expose
only diagnostic and history reads, enforced by the stdio server regardless of
client settings. MCP code clearing requires opt-in on both the adapter owner and
the calling stdio process, a human confirmation and the same active session.

## Reporting a vulnerability

Use GitHub's [private vulnerability reporting](https://github.com/Avokid/obdtrace/security/advisories/new)
if available. If GitHub does not offer a private reporting channel, request one
from the maintainer without disclosing vulnerability details in a public issue.
Include the affected version, platform, expected behavior, and likely impact.
Use synthetic examples; do not attempt to demonstrate a problem on a real car.

Never attach a real database, `vehicle.key`, raw adapter log, VIN, Bluetooth
address, credentials or unreviewed diagnostic bundle. Masking the ECU VIN does
not make other identifying data anonymous. Do not open databases or
write-ahead logs from an untrusted source.
