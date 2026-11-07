#!/usr/bin/env python3
"""Explicit maintenance operation: record actual original and patched bytes."""
import argparse
import json
from pathlib import Path
import tarfile

from verify_provenance import ARCHIVE_SHA256, REPOSITORY, digest, inventory, relative_path, require, verify


def record(archive):
    data = archive.read_bytes()
    require(digest(data) == ARCHIVE_SHA256, "wrong published archive")
    original = {}
    executables = {}
    with tarfile.open(archive) as package:
        for member in package:
            require(member.isfile(), "nonregular archive member")
            prefix = "xsd-schema-0.3.0/"
            require(member.name.startswith(prefix), "unexpected archive prefix")
            name = relative_path(member.name[len(prefix):])
            require(name not in original, "duplicate archive member")
            original[name] = package.extractfile(member).read()
            executables[name] = bool(member.mode & 0o111)
    patched = inventory(REPOSITORY / "vendor/xsd-schema")
    require(set(original) == set(patched) and len(original) == 328, "package membership changed")
    identify = lambda b: {"bytes": len(b), "sha256": digest(b)}
    patch_dir = REPOSITORY / "vendor/xsd-schema-patches"
    patch_names = ["0001-msrv-unpublished.patch", "0002-owned-diagnostic-evidence.patch", "0003-cooperative-stop.patch", "0004-xml-character-content.patch"]
    manifest = {
        "format": "bsc8-maintained-xsd-v1",
        "package": {"name": "xsd-schema", "version": "0.3.0", "local_revision": "bsc8-maintenance-2", "publish": False},
        "archive": {"url": "https://crates.io/api/v1/crates/xsd-schema/0.3.0/download", **identify(data)},
        "recorded_upstream_vcs": json.loads(original[".cargo_vcs_info.json"]),
        "patches": [{"path": name, **identify((patch_dir / name).read_bytes())} for name in patch_names],
        "files": [{"path": name, "executable": executables[name], "original": identify(original[name]), "patched": identify(patched[name])} for name in sorted(original)],
    }
    destination = REPOSITORY / "vendor/xsd-schema.provenance.json"
    destination.write_bytes((json.dumps(manifest, indent=2) + "\n").encode())
    # Recording an inventory is not enough: patches must reconstruct those bytes.
    print("recorded and replay-verified source sha256=" + verify(archive=archive))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    record(parser.parse_args().archive)
