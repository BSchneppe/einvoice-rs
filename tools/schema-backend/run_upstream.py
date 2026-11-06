#!/usr/bin/env python3
"""Run unchanged, self-contained upstream integration suites with the harness lock.

Cargo cannot test a nonmember path dependency with dev-dependencies. A disposable
workspace selects public integration tests explicitly, so criterion/bench deps
and unapproved optional features do not require a second dependency resolution.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
SUITES = ["decimal_capacity", "attribute_uses", "nil_property", "root_validity",
          "validation_api", "validation_root", "facet_soundness"]


def main():
    with tempfile.TemporaryDirectory(prefix="xsd-upstream-tests-") as temp:
        directory = Path(temp)
        manifest = (ROOT / "tools/schema-backend/Cargo.toml").read_text()
        manifest = manifest.replace('path = "../../vendor/xsd-schema"', 'path = ' + json.dumps((ROOT / "vendor/xsd-schema").as_posix()))
        manifest = manifest.replace('[package]\n', '[package]\nautotests = false\n')
        # Names are only recognized for cfg in the unchanged upstream tests;
        # none is enabled, nor do these declarations forward backend features.
        manifest += '\n[features]\nasync = []\nxsd11 = []\ncompose = []\n'
        for suite in SUITES:
            manifest += '\n[[test]]\nname = ' + json.dumps(suite) + '\npath = ' + json.dumps((ROOT / f"vendor/xsd-schema/tests/{suite}.rs").as_posix()) + '\n'
        (directory / "Cargo.toml").write_bytes(manifest.encode())
        shutil.copyfile(ROOT / "tools/schema-backend/Cargo.lock", directory / "Cargo.lock")
        (directory / "src").mkdir()
        (directory / "src/lib.rs").write_bytes(b"//! Disposable upstream public-API test harness.\n")
        env = dict(os.environ)
        env.setdefault("CARGO_TARGET_DIR", str(ROOT / "tools/schema-backend/target/upstream"))
        subprocess.run(["cargo", "+1.83.0", "test", "--locked", "--offline", "--manifest-path", str(directory / "Cargo.toml"), "--tests"], check=True, env=env)
        if (directory / "Cargo.lock").read_bytes() != (ROOT / "tools/schema-backend/Cargo.lock").read_bytes():
            raise ValueError("upstream test resolution changed the lock")


if __name__ == "__main__":
    main()
