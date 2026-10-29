#!/usr/bin/env python3
"""Fail when the app's version numbers disagree.

    python scripts/check_versions.py            # package.json, Cargo.toml, tauri.conf.json
    python scripts/check_versions.py v0.2.0     # ...and the release tag

A Tauri app keeps its version in three files. The in-app updater compares
against tauri.conf.json, the About panel shows Cargo's, and patchnotes checks
package.json, so a release where they drift ships a confused app.
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def cargo_version(text):
    # [workspace.package] version = "x.y.z"; the members inherit it
    m = re.search(r"^\[workspace\.package\][^\[]*?^version\s*=\s*\"([^\"]+)\"", text, re.M | re.S)
    return m.group(1) if m else None


def main(argv):
    found = {
        "package.json": json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"],
        "Cargo.toml": cargo_version((ROOT / "Cargo.toml").read_text(encoding="utf-8")),
        "src-tauri/tauri.conf.json": json.loads(
            (ROOT / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8")
        )["version"],
    }
    if len(argv) > 1:
        found["tag"] = argv[1].lstrip("vV")
    if len(set(found.values())) == 1 and None not in found.values():
        print(f"versions agree: {found['package.json']}")
        return 0
    for where, v in found.items():
        print(f"  {where}: {v}", file=sys.stderr)
    print("check_versions: the versions above must match", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
