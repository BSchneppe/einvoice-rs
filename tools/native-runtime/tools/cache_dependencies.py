#!/usr/bin/env python3
"""Build an isolated offline Cargo source from checksum-verified cached archives.
No downloads, registry mutations, or dependency version decisions. Python 3.12+.
"""
import argparse
import hashlib
import json
from pathlib import Path
import tarfile
import tomllib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--registry-cache", type=Path, default=Path.home() / ".cargo/registry/cache")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
cache = root / ".cache"
vendor = cache / "vendor"
vendor.mkdir(parents=True, exist_ok=True)
for package in tomllib.loads((root / "Cargo.lock").read_text())["package"]:
    if "source" not in package:
        continue
    if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
        raise ValueError("Only pinned crates.io archives are supported")
    stem = package["name"] + "-" + package["version"]
    archives = sorted(args.registry_cache.glob("*/" + stem + ".crate"))
    archive = next((p for p in archives if hashlib.sha256(p.read_bytes()).hexdigest()
                    == package["checksum"]), None)
    if archive is None:
        raise FileNotFoundError("No checksum-matching cached archive for " + stem)
    with tarfile.open(archive) as source:
        if any(not x.name.startswith(stem + "/") for x in source.getmembers()):
            raise ValueError("Unexpected archive root")
        source.extractall(vendor, filter="data")
    destination = vendor / stem
    files = {str(p.relative_to(destination)): hashlib.sha256(p.read_bytes()).hexdigest()
             for p in destination.rglob("*") if p.is_file() and p.name != ".cargo-checksum.json"}
    (destination / ".cargo-checksum.json").write_text(json.dumps(
        {"files": files, "package": package["checksum"]}))
    print(stem, package["checksum"])
cargo_home = cache / "cargo-home"
cargo_home.mkdir(exist_ok=True)
(cargo_home / "config.toml").write_text(
    '[source.crates-io]\nreplace-with = "local"\n[source.local]\ndirectory = '
    + json.dumps(str(vendor)) + "\n")
