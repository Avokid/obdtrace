# Releasing OBDTrace

The GitHub workflow builds and tests the locked Rust dependencies on native
Linux x86-64, macOS Apple silicon and Windows x64 runners. It smoke-tests the
extracted executable against a simulated car. A `v<version>` tag matching
`Cargo.toml` creates a **draft** release in `Avokid/obdtrace`, initially with
only the Linux x86-64 archive and a `SHA256SUMS` file. Windows and macOS
remain CI test targets, not release downloads. Only Linux archives are uploaded
from ordinary private main-branch builds; PR validation does not upload binaries.
On public repositories the workflow runs tests and package checks but skips
all binary uploads and draft releases until public distribution is approved.
The Linux archive includes a
[locked dependency inventory](linux-dependencies.md) and crate notices.
Regenerate the inventory with `python3 scripts/linux_dependencies.py` after
building the locked Linux target. No tag is needed to test the workflow
and download build artifacts from a branch.

Release binaries are tested against the simulator and intended for
single-user machines, not shared hosts. Before adding a platform to the
release, verify the authenticated browser and MCP flows and session-bound
approvals on it. Do not share diagnostic bundles as anonymous data. See the [security policy](../SECURITY.md).

1. Before making a formerly private repository public, review the complete
   Git history, release files, issues and other formerly private content.
   Never publish diagnostic captures, vehicle identifiers or `vehicle.key`.
   Check for credentials, personal paths and third-party license obligations.
   When the owner approves the rename and publication, set the repository
   description to “Local OBD-II diagnostics, session history and MCP for
   ELM327 adapters” and add topics such as `obd-ii`, `elm327`, `bluetooth`,
   `vehicle-diagnostics` and `mcp`.
2. Update the version in `Cargo.toml` and `Cargo.lock`; run CI. Verify the
   tested OS versions, archive hashes, platform signing status and actual
   hardware limitations before publishing the release notes. The workflow
   does not provision macOS notarization or Windows Authenticode credentials.
3. Tag the reviewed commit and inspect the draft. Publish it only after
   confirming the download and [installation instructions](install.md).

## Before changing GitHub visibility

Opening the source also exposes Git history and formerly private collaboration
material. A draft or prerelease label is not an access control. The repository
owner, not CI, makes the visibility change.

1. Review every branch and tag, not just `main`. Run a local secret scanner
   over all refs (for example `gitleaks git --redact --log-opts="--all" .`).
   Inspect contributor names, author/committer emails and trailers separately;
   decide whether they may be public. If a real secret is found, revoke it
   before considering any cleanup. Do not rewrite shared history by default.
2. Review issue/PR bodies, review comments, attachments, Actions logs and
   downloadable artifacts, release assets, and enabled wiki/pages/discussions.
   A clean working tree or `.gitignore` does not cover these.
   Remove or let unapproved artifacts expire **before** the switch; changing
   future workflow uploads does not hide existing artifacts.
3. Explicitly decide whether the existing simulator-only Linux prerelease
   and its tag may become public. For a source-only opening, withdraw
   unapproved binaries and keep binary artifact uploads disabled on public runs.
   If existing history or collaboration content cannot be released, keep the
   private repository and publish a reviewed source snapshot separately.
4. Check branch protections/rulesets, Actions write permissions, secrets and
   environments for public-fork exposure. Do not run untrusted PR code with
   release credentials. Verify the private reporting channel in `SECURITY.md`
   and enable private vulnerability reporting, secret scanning/push protection
   and dependency alerts where available. Recheck these settings immediately
   after the owner changes visibility.

## Optional MCP Registry listing

In a private repository the Apple silicon build produces a separate `obdtrace-mcpb-candidate` workflow
artifact on tag or manual runs. It contains a native `.mcpb` bundle and
matching `server.json`. This is a candidate for a client that supports
Apple silicon MCPB bundles, not a universal installer. The binary runs
`obdtrace mcp` over stdio. The bundle does not supply the narrower Copilot CLI
allowlist; it does not enable code clearing or auto-connect by default.

Before listing: install the bundle on a clean Apple silicon system in an
MCPB-capable desktop client, verify MCP discovery and demo history, and
record the client and OS versions. Attach the exact tested bundle to the
matching versioned release; verify the downloaded file matches
`packages[0].fileSha256` in `server.json`. Keep the `.mcpb` candidate and
registry entry unpublished if this test cannot be performed.

Authenticate `mcp-publisher` for `io.github.avokid/obdtrace` and publish
the generated `server.json` from its directory only after checking namespace
availability and the public asset URL. See the [MCP Registry quickstart](https://modelcontextprotocol.io/registry/quickstart)
and [MCPB package rules](https://modelcontextprotocol.io/registry/package-types#mcpb-packages).
The Registry lists metadata; GitHub Releases hosts the package. Standalone
archives and the Connect page remain the recommended installation path.
