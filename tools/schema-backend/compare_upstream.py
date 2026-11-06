#!/usr/bin/env python3
"""Compare raw authored-fixture records to archive + manifest-only patch."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from verify_provenance import REPOSITORY as ROOT, archive_inventory, apply_patches, digest, load_manifest, verify, write_inventory


def run(manifest, env):
    result = subprocess.run(["cargo", "+1.83.0", "run", "--offline", "--locked", "--manifest-path", str(manifest),
                             "--example", "raw_records"], check=True, capture_output=True, env=env)
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    verify(archive=args.archive)
    args.output.mkdir(parents=True, exist_ok=True)
    metadata = load_manifest(ROOT / "vendor/xsd-schema.provenance.json")
    original = archive_inventory(args.archive, metadata)
    env = dict(os.environ)
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "tools/schema-backend/target/comparison"))
    patched = run(ROOT / "tools/schema-backend/Cargo.toml", env)
    with tempfile.TemporaryDirectory(prefix="xsd-raw-baseline-") as temp:
        directory = Path(temp)
        package = directory / "package"
        write_inventory(package, original)
        apply_patches(package, [(ROOT / "vendor/xsd-schema-patches/0001-msrv-unpublished.patch").read_bytes()])
        harness = directory / "harness"
        harness.mkdir()
        manifest = (ROOT / "tools/schema-backend/Cargo.toml").read_text().replace('path = "../../vendor/xsd-schema"', 'path = ' + json.dumps(package.as_posix()))
        (harness / "Cargo.toml").write_bytes(manifest.encode())
        shutil.copyfile(ROOT / "tools/schema-backend/Cargo.lock", harness / "Cargo.lock")
        for path in ["src/lib.rs", "examples/raw_records.rs", "tests/support/mod.rs"]:
            target = harness / path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / "tools/schema-backend" / path, target)
        baseline = run(harness / "Cargo.toml", env)
    (args.output / "raw-baseline.log").write_bytes(baseline)
    (args.output / "raw-patched.log").write_bytes(patched)
    if baseline != patched:
        raise ValueError("raw upstream diagnostics differ; inspect saved records")
    print(f"{len(patched.splitlines())} raw records byte-identical; sha256={digest(patched)}")


if __name__ == "__main__":
    main()
