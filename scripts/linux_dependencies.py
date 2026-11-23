"""Generate the Linux build inventory and collect notices from Cargo sources."""

import json
from pathlib import Path
import re
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-unknown-linux-gnu"
REPORT = "linux-dependencies.md"


def collect(directory):
    def cargo(*args):
        return subprocess.check_output(["cargo", *args, "--locked", "--offline"], cwd=ROOT, text=True)

    tree = cargo("tree", "--target", TARGET, "--edges", "normal,build", "--prefix", "none")
    keys = set(re.findall(r"^(\S+) v(\S+)", tree, re.MULTILINE))
    metadata = json.loads(cargo("metadata", "--format-version", "1", "--filter-platform", TARGET))
    packages = sorted((p for p in metadata["packages"] if (p["name"], p["version"]) in keys),
                      key=lambda p: (p["name"], p["version"]))
    assert len(packages) == len(keys), "Incomplete Linux build graph"
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    direct = set(manifest["dependencies"]) | set(manifest["target"]['cfg(target_os = "linux")']["dependencies"]) | set(manifest["target"]["cfg(unix)"]["dependencies"])
    files = {}
    lines = [
        "# OBDTrace Linux build dependencies", "",
        f"Target: `{TARGET}`. Generated from `Cargo.lock` with normal and build edges.",
        "Includes build scripts and procedural macros, not just code linked into the executable.",
        "Excludes development-only and other-platform dependencies.", "",
        "Build: Rust/Cargo, a C compiler and linker, pkg-config, and D-Bus development headers.",
        "The Ubuntu CI build installs `pkg-config libdbus-1-dev libudev-dev`.",
        "Python is used to package and test the archive, not to run OBDTrace.", "",
        "The first Ubuntu 22.04 x86-64 binary directly needs `libdbus-1.so.3`,",
        "`libgcc_s.so.1`, `libm.so.6` and `libc.so.6` (ELF `DT_NEEDED`).",
        "BlueZ and a working system D-Bus are needed for Bluetooth. SQLite is bundled",
        "via `libsqlite3-sys`; the browser UI embeds i18next 27.2.6 (MIT).", "",
        "The native archive includes license files under `licenses/`, plus the application",
        "and i18next licenses at its root. Source links refer to the exact crate versions.",
        "For MPL-2.0 `serialport`, the corresponding unmodified source is available",
        "from the versioned download link below.", "",
        "| Crate | Version | Relationship | Declared license | Source |",
        "| --- | --- | --- | --- | --- |",
    ]
    for p in packages:
        if p["id"] == metadata["resolve"]["root"]:
            continue
        name, version = p["name"], p["version"]
        if p["source"] != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError(f"Review source distribution for {name} {version}")
        source = f"https://static.crates.io/crates/{name}/{name}-{version}.crate"
        lines.append(f"| {name} | {version} | {'Direct' if name in direct else 'Transitive'} "
                     f"| {p['license']} | [source]({source}) |")
        base = Path(p["manifest_path"]).parent
        notices = {f.relative_to(base).as_posix(): f for f in base.rglob("*")
                   if f.is_file() and not f.is_symlink() and
                   any(part.lower().startswith(("license", "licence", "notice", "copying", "copyright", "unlicense"))
                       for part in f.relative_to(base).parts)}
        if not notices and (name, version) in {("bluez-async", "0.8.2"), ("bluez-generated", "0.4.0")}:
            notices = {"LICENSE-MIT": ROOT / "licenses/bluez-rs-LICENSE-MIT"}
        if not notices and (name, version) == ("yasna", "0.5.2"):
            notices = {"LICENSE-MIT": ROOT / "licenses/yasna-LICENSE-MIT"}
        if not notices:
            raise ValueError(f"No notices packaged for {name} {version}; review upstream")
        for path, file in notices.items():
            files[f"licenses/{name}-{version}/{path}"] = file
    directory.mkdir(parents=True, exist_ok=True)
    report = directory / REPORT
    report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    files[REPORT] = report
    return files


if __name__ == "__main__":
    collect(ROOT / "docs")
