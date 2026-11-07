#!/usr/bin/env python3
"""Offline maintained-parser inventory, optionally authenticated archive replay."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[3]
ARCHIVE_SHA = "3ae8337f8a065cfc972643663ea4279e04e7256de865aa66fe25cec5fb912d3f"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def inventory(root):
    require(root.is_dir() and not root.is_symlink(), "not a regular directory")
    result = {}
    for path in sorted(root.rglob("*")):
        mode = path.lstat().st_mode
        require(not stat.S_ISLNK(mode), "symlink in parser inventory")
        if stat.S_ISDIR(mode):
            continue
        require(stat.S_ISREG(mode), "special parser file")
        result[path.relative_to(root).as_posix()] = path.read_bytes()
    return result


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate manifest key")
        result[key] = value
    return result


def verify(root=ROOT, archive=None):
    manifest = json.loads((root / "vendor/xml-rs.provenance.json").read_bytes(), object_pairs_hook=unique_object)
    require(manifest["format"] == "einvoice-xml-parser-source/1", "unknown format")
    require(manifest["archive"]["sha256"] == ARCHIVE_SHA, "unexpected archive identity")
    rows = manifest["files"]
    names = [r["path"] for r in rows]
    require(len(names) == 35 and names == sorted(set(names)), "invalid inventory")
    for name in names:
        p = PurePosixPath(name)
        require(not p.is_absolute() and ".." not in p.parts and str(p) == name and "\\" not in name, "unsafe path")
    actual = inventory(root / "vendor/xml-rs")
    require(set(actual) == set(names), "parser membership mismatch")
    for row in rows:
        require(digest(actual[row["path"]]) == row["patched"], "parser bytes changed: " + row["path"])
        if os.name != "nt":
            require(bool((root / "vendor/xml-rs" / row["path"]).stat().st_mode & 0o111) == row["executable"], "parser mode changed")
    require(manifest["patch"]["path"] == "xml-rs.patch", "unexpected patch path")
    patch = (root / "vendor/xml-rs.patch").read_bytes()
    require(digest(patch) == {k: manifest["patch"][k] for k in ("bytes", "sha256")}, "patch bytes changed")
    require(sum(r["original"] != r["patched"] for r in rows) == 3, "unexpected modified-file count")
    if archive is not None:
        data = archive.read_bytes()
        require(digest(data) == {k: manifest["archive"][k] for k in ("bytes", "sha256")}, "archive mismatch")
        original = {}
        with tarfile.open(archive) as source:
            for member in source:
                require(member.isfile() and member.name.startswith("xml-rs-0.8.28/"), "invalid archive member")
                name = member.name[len("xml-rs-0.8.28/"):]
                require(name in actual and name not in original, "unexpected archive member")
                original[name] = source.extractfile(member).read()
                row = next(row for row in rows if row["path"] == name)
                require(bool(member.mode & 0o111) == row["executable"], "archive mode mismatch")
        require(set(original) == set(names), "original membership mismatch")
        for row in rows:
            require(digest(original[row["path"]]) == row["original"], "original bytes mismatch")
        with tempfile.TemporaryDirectory(prefix="einvoice-parser-replay-") as temp:
            work = Path(temp)
            for name, data in original.items():
                path = work / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
            for reverse, expected in [(False, actual), (True, original)]:
                args = ["git", "-c", "core.autocrlf=false", "apply", "--whitespace=nowarn"]
                if reverse:
                    args.append("--reverse")
                subprocess.run(args, input=patch, cwd=work, check=True, capture_output=True)
                require(inventory(work) == expected, "patch replay differs")
    return hashlib.sha256(json.dumps({k: digest(v) for k, v in sorted(actual.items())}, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path)
    args = parser.parse_args()
    print("verified maintained parser source sha256=" + verify(archive=args.archive))
