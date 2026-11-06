#!/usr/bin/env python3
"""Reject feature unification outside the approved default backend surface."""
import argparse
import json
from pathlib import Path
import subprocess
import tomllib

EXPECTED = {"default", "unicode-normalization"}


def check(metadata):
    packages = [p for p in metadata["packages"] if p["name"] == "xsd-schema"]
    if len(packages) != 1 or packages[0]["version"] != "0.3.0":
        raise ValueError("expected one xsd-schema 0.3.0")
    package = packages[0]
    expected_manifest = Path(__file__).resolve().parents[2] / "vendor/xsd-schema/Cargo.toml"
    if Path(package["manifest_path"]).resolve() != expected_manifest:
        raise ValueError("backend is not the maintained local source")
    nodes = [n for n in metadata["resolve"]["nodes"] if n["id"] == package["id"]]
    if len(nodes) != 1 or set(nodes[0]["features"]) != EXPECTED:
        raise ValueError("unexpected backend features: " + repr(nodes))
    lock_packages = tomllib.loads((Path(__file__).parent / "Cargo.lock").read_text())["package"]
    for name, version in [("proc-macro-crate", "3.3.0"), ("indexmap", "2.13.0"), ("hashbrown", "0.16.1")]:
        # These may be inactive lock-only packages: verify the deliberate pins
        # separately in Cargo.lock, not by pretending all are enabled host code.
        found = [p["version"] for p in lock_packages if p["name"] == name]
        if found != [version]:
            raise ValueError(f"unexpected {name} lock entries: {found}")
    return sorted(nodes[0]["features"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    args = parser.parse_args()
    result = subprocess.run(["cargo", "+1.83.0", "metadata", "--locked", "--format-version", "1",
                             "--manifest-path", str(Path(__file__).parent / "Cargo.toml"),
                             "--filter-platform", args.target], check=True, capture_output=True)
    print("enabled xsd-schema features: " + ", ".join(check(json.loads(result.stdout))))


if __name__ == "__main__":
    main()
