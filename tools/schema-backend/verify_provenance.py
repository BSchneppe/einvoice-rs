#!/usr/bin/env python3
"""Offline inventory verification; optional authenticated archive reverse/replay."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tarfile
import tempfile

REPOSITORY = Path(__file__).resolve().parents[2]
ARCHIVE_SHA256 = "c6e18f97809c6066c39ea64c9067fee882e92181bdcb366c6a8d99ab4f618d3e"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def relative_path(value):
    p = PurePosixPath(value)
    require(value and not p.is_absolute() and str(p) == value
            and all(part not in (".", "..") for part in p.parts)
            and "\\" not in value, f"unsafe path: {value!r}")
    return value


def inventory(directory):
    require(not directory.is_symlink() and directory.is_dir(), f"not a directory: {directory}")
    result = {}
    # Do not follow symlinks, including directory symlinks. Reject all specials.
    for p in sorted(directory.rglob("*")):
        mode = p.lstat().st_mode
        require(not stat.S_ISLNK(mode), f"symlink: {p}")
        if stat.S_ISDIR(mode):
            continue
        require(stat.S_ISREG(mode), f"nonregular file: {p}")
        result[p.relative_to(directory).as_posix()] = p.read_bytes()
    return result


def unique_object(pairs):
    result = {}
    for k, v in pairs:
        require(k not in result, f"duplicate JSON key: {k}")
        result[k] = v
    return result


def load_manifest(path):
    require(not path.is_symlink() and path.is_file(), "manifest must be a regular file")
    return json.loads(path.read_bytes(), object_pairs_hook=unique_object)


def check_bytes(actual, expected, label):
    require(len(actual) == expected["bytes"], f"size mismatch: {label}")
    require(digest(actual) == expected["sha256"], f"hash mismatch: {label}")


def apply_patches(directory, patches, reverse=False):
    for data in reversed(patches) if reverse else patches:
        args = ["git", "-c", "core.autocrlf=false", "apply", "--whitespace=nowarn"]
        if reverse:
            args.append("--reverse")
        result = subprocess.run(args, input=data, cwd=directory, capture_output=True)
        require(result.returncode == 0,
                "patch replay failed: " + result.stderr.decode(errors="replace"))


def archive_inventory(path, manifest):
    require(not path.is_symlink() and path.is_file(), "archive must be a regular file")
    data = path.read_bytes()
    check_bytes(data, manifest["archive"], "archive")
    require(digest(data) == ARCHIVE_SHA256, "unexpected upstream archive")
    result = {}
    executable_flags = {row["path"]: row["executable"] for row in manifest["files"]}
    with tarfile.open(path) as archive:
        for member in archive:
            require(member.isfile(), f"nonregular archive member: {member.name}")
            prefix = "xsd-schema-0.3.0/"
            require(member.name.startswith(prefix), "wrong archive prefix")
            name = relative_path(member.name[len(prefix):])
            require(name not in result, f"duplicate archive member: {name}")
            require(name in executable_flags and bool(member.mode & 0o111) == executable_flags[name], "upstream executable mode mismatch: " + name)
            result[name] = archive.extractfile(member).read()
    return result


def write_inventory(directory, files, executables=()):
    directory.mkdir(parents=True, exist_ok=True)
    for name, data in files.items():
        p = directory / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
        p.chmod(0o755 if name in executables else 0o644)


def verify(repository=REPOSITORY, archive=None, reproduce=None):
    manifest = load_manifest(repository / "vendor/xsd-schema.provenance.json")
    require(manifest["format"] == "bsc8-maintained-xsd-v1", "unknown manifest format")
    require(manifest["archive"]["sha256"] == ARCHIVE_SHA256, "wrong archive identity")
    rows = manifest["files"]
    names = [relative_path(row["path"]) for row in rows]
    require(names == sorted(set(names)) and len(names) == 328, "invalid file inventory")
    files = inventory(repository / "vendor/xsd-schema")
    require(set(files) == set(names),
            f"file membership mismatch: missing={sorted(set(names)-set(files))}, extra={sorted(set(files)-set(names))}")
    executables = {row["path"] for row in rows if row["executable"]}
    if os.name != "nt":
        for row in rows:
            require(bool((repository / "vendor/xsd-schema" / row["path"]).stat().st_mode & 0o111) == row["executable"],
                    "executable mode mismatch: " + row["path"])
    for row in rows:
        check_bytes(files[row["path"]], row["patched"], row["path"])
    patch_dir = repository / "vendor/xsd-schema-patches"
    patch_files = inventory(patch_dir)
    patch_names = [relative_path(p["path"]) for p in manifest["patches"]]
    require(len(patch_names) == len(set(patch_names)) and patch_names, "invalid patch inventory")
    require(set(patch_files) == set(patch_names) | {"README.md"}, "patch membership mismatch")
    patches = []
    for row in manifest["patches"]:
        check_bytes(patch_files[row["path"]], row, row["path"])
        patches.append(patch_files[row["path"]])
    # Manifest does not digest itself. This identity is computed from the actual
    # consumed file bytes, not copied from a desired revision or expected hash.
    identity_data = {"domain": "bsc8-maintained-xsd-consumed-source-v1",
                     "files": [{"path": n, "sha256": digest(files[n])} for n in sorted(files)]}
    identity = digest(json.dumps(identity_data, sort_keys=True, separators=(",", ":")).encode())
    if archive is not None:
        original = archive_inventory(archive, manifest)
        require(set(original) == set(names), "upstream membership mismatch")
        for row in rows:
            check_bytes(original[row["path"]], row["original"], "upstream " + row["path"])
        with tempfile.TemporaryDirectory(prefix="xsd-replay-") as temp:
            root = Path(temp)
            forward, reverse = root / "forward", root / "reverse"
            write_inventory(forward, original, executables)
            apply_patches(forward, patches)
            require(inventory(forward) == files, "forward patch bytes differ")
            write_inventory(reverse, files, executables)
            apply_patches(reverse, patches, reverse=True)
            require(inventory(reverse) == original, "reverse patch bytes differ")
        if reproduce is not None:
            require(not reproduce.is_symlink() and not reproduce.exists(), "reproduction destination must not exist")
            write_inventory(reproduce, original, executables)
            apply_patches(reproduce, patches)
            require(inventory(reproduce) == files, "reproduction differs")
    else:
        require(reproduce is None, "--reproduce requires --archive")
    return identity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--reproduce", type=Path)
    args = parser.parse_args()
    print("verified consumed source sha256=" + verify(archive=args.archive, reproduce=args.reproduce))


if __name__ == "__main__":
    main()
