#!/usr/bin/env python3
"""Optional developer replay, Python 3.11+, no network, scratch or invoice corpus.

Verifies the complete installed Corretto image, pinned KoSIT JAR and original
configuration archive before extracting ONLY schxslt:location. Never executes
invoice rules or an acceptance backend. Rust tests need neither Java nor this
archive. The expected text is a retained independent result, not generated from
Rust. For another platform supply a separately verified image-tree SHA-256.
"""
import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile

JAR_SHA = "799e64befca97d4080e03608c80b85dd5a5ecc5f4ae4f35d1116ec2855b9a7c9"
SOURCE_SHA = "2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8"
MEMBER = "resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl"
MEMBER_SHA = "ec54f662803a87f527ec606d1320bc404f1657a848af6a76f37f32905e4aac40"
FUNCTION_SHA = "9a75d90bad2d77d41a7fc92f5da69a49d12b3dd72d1530032f141c0d3fff7d26"
# Local complete payload fingerprint, macOS aarch64. This is NOT represented as
# the upstream download/archive checksum (see evidence README).
DARWIN_IMAGE_SHA = "9dcdb182ab99f8017e5e535342b0c6dd3ccac49a3434ee1f1932e6eacbfc08cf"


def file_sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def image_sha(root):
    h = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix().encode()
        if path.is_symlink():
            h.update(b"L\0" + relative + b"\0" + os.readlink(path).encode() + b"\0")
        elif path.is_file():
            h.update(b"F\0" + relative + b"\0")
            with path.open("rb") as f:
                h.update(hashlib.file_digest(f, "sha256").digest())
    return h.hexdigest()


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jar", type=Path, required=True)
    parser.add_argument("--java-home", type=Path, required=True)
    parser.add_argument("--source-archive", type=Path, required=True)
    parser.add_argument("--corretto-tree-sha256", default=DARWIN_IMAGE_SHA)
    args = parser.parse_args()
    jar, home, archive = args.jar.resolve(), args.java_home.resolve(), args.source_archive.resolve()
    require(file_sha(jar) == JAR_SHA, "pinned JAR checksum mismatch")
    require(file_sha(archive) == SOURCE_SHA, "pinned original source archive checksum mismatch")
    require(image_sha(home) == args.corretto_tree_sha256, "complete Corretto image checksum mismatch")
    release = (home / "release").read_text()
    require('IMPLEMENTOR_VERSION="Corretto-27.0.1.0.11"' in release, "wrong Corretto version")
    require('JAVA_RUNTIME_VERSION="27.0.1+11-FR"' in release, "wrong Java runtime")
    with zipfile.ZipFile(archive) as source:
        xsl = source.read(MEMBER)
    require(hashlib.sha256(xsl).hexdigest() == MEMBER_SHA, "location-function source member mismatch")
    start = xsl.index(b'   <xsl:function name="schxslt:location"')
    end = xsl.index(b"</xsl:function>", start) + len(b"</xsl:function>")
    function = xsl[start:end]
    require(hashlib.sha256(function).hexdigest() == FUNCTION_SHA, "location function mismatch")
    evidence = Path(__file__).resolve().parent.parent / "tests" / "evidence"
    wrapper = (b'<xsl:stylesheet version="2.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform" '
               b'xmlns:xs="http://www.w3.org/2001/XMLSchema" '
               b'xmlns:schxslt="https://doi.org/10.5281/zenodo.1495494">'
               b'<xsl:output method="text"/>' + function +
               b'<xsl:template match="/"><xsl:for-each select="/descendant-or-self::node() | //@*">'
               b'<xsl:value-of select="schxslt:location(.)"/><xsl:text>|</xsl:text>'
               b'<xsl:value-of select="string-join(for $c in string-to-codepoints(string(.)) '
               b'return string($c), \',\')"/><xsl:text>&#10;</xsl:text>'
               b'</xsl:for-each></xsl:template></xsl:stylesheet>')
    with tempfile.TemporaryDirectory(prefix="einvoice-source-saxon-") as temporary:
        stylesheet = Path(temporary) / "paths.xsl"
        stylesheet.write_bytes(wrapper)
        run = subprocess.run([str(home / "bin" / "java"), "-cp", str(jar),
                              "net.sf.saxon.Transform", "-s:" + str(evidence / "paths.xml"),
                              "-xsl:" + str(stylesheet)], capture_output=True, timeout=45)
    require(run.returncode == 0, f"Saxon failed ({run.returncode}): {run.stderr.decode()}")
    expected = (evidence / "saxon-18.txt").read_bytes()
    require(run.stdout == expected, "Saxon path/value mismatch against independently retained rows")
    require(len(run.stdout.splitlines()) == 18, "missing/extra row")
    print("PASS 18 exact independently replayed schxslt path / Saxon string-value rows")
    print(f"Corretto image SHA-256 {args.corretto_tree_sha256}; JAR SHA-256 {JAR_SHA}")


if __name__ == "__main__":
    main()
