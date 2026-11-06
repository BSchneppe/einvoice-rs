"""Offline mutation regressions for the inventory and feature guards."""
import copy
import os
from pathlib import Path
import shutil
import tempfile
import unittest

import verify_features
import verify_provenance as provenance


class ProvenanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="xsd-provenance-test-")
        self.addCleanup(self.temp.cleanup)
        self.repository = Path(self.temp.name)
        shutil.copytree(provenance.REPOSITORY / "vendor", self.repository / "vendor")

    def test_unmodified_inventory(self):
        self.assertEqual(provenance.verify(self.repository), provenance.verify())

    @unittest.skipIf(os.name == "nt", "Git executable mode is not represented by Windows stat")
    def test_executable_mode(self):
        p = self.repository / "vendor/xsd-schema/scripts/particle-gate.sh"
        p.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "mode mismatch"):
            provenance.verify(self.repository)

    def test_missing_file(self):
        (self.repository / "vendor/xsd-schema/src/lib.rs").unlink()
        with self.assertRaisesRegex(ValueError, "membership"):
            provenance.verify(self.repository)

    def test_extra_file(self):
        (self.repository / "vendor/xsd-schema/extra").write_bytes(b"extra")
        with self.assertRaisesRegex(ValueError, "membership"):
            provenance.verify(self.repository)

    def test_changed_byte(self):
        p = self.repository / "vendor/xsd-schema/assets/xlink.xsd"
        p.write_bytes(p.read_bytes().replace(b"\r\n", b"\n", 1))
        with self.assertRaisesRegex(ValueError, "mismatch"):
            provenance.verify(self.repository)

    @unittest.skipIf(os.name == "nt", "Windows runners need symlink privilege; native Unix CI covers links")
    def test_file_and_directory_symlinks(self):
        for directory in [False, True]:
            with self.subTest(directory=directory):
                p = self.repository / "vendor/xsd-schema/linked"
                p.symlink_to("src" if directory else "src/lib.rs", target_is_directory=directory)
                try:
                    with self.assertRaisesRegex(ValueError, "symlink"):
                        provenance.verify(self.repository)
                finally:
                    p.unlink()

    def test_missing_patch(self):
        (self.repository / "vendor/xsd-schema-patches/0002-owned-diagnostic-evidence.patch").unlink()
        with self.assertRaisesRegex(ValueError, "membership"):
            provenance.verify(self.repository)

    def test_changed_patch(self):
        p = self.repository / "vendor/xsd-schema-patches/0003-cooperative-stop.patch"
        p.write_bytes(p.read_bytes() + b"\n")
        with self.assertRaisesRegex(ValueError, "mismatch"):
            provenance.verify(self.repository)

    def test_extra_patch(self):
        (self.repository / "vendor/xsd-schema-patches/extra.patch").write_bytes(b"extra")
        with self.assertRaisesRegex(ValueError, "membership"):
            provenance.verify(self.repository)

    def test_upstream_hash_inventory_is_checked_when_archive_supplied(self):
        archive = os.environ.get("XSD_UPSTREAM_ARCHIVE")
        if not archive:
            self.skipTest("optional upstream archive not supplied")
        manifest_path = self.repository / "vendor/xsd-schema.provenance.json"
        manifest = provenance.load_manifest(manifest_path)
        manifest["files"][0]["original"]["sha256"] = "0" * 64
        import json
        manifest_path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "mismatch"):
            provenance.verify(self.repository, Path(archive))

    def test_forward_reverse_and_reproduction(self):
        archive = os.environ.get("XSD_UPSTREAM_ARCHIVE")
        if not archive:
            self.skipTest("optional upstream archive not supplied")
        destination = self.repository / "reproduced"
        provenance.verify(self.repository, Path(archive), destination)
        self.assertEqual(provenance.inventory(destination), provenance.inventory(self.repository / "vendor/xsd-schema"))


class FeatureTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {"packages": [{"name": "xsd-schema", "version": "0.3.0", "id": "backend",
                         "manifest_path": str(provenance.REPOSITORY / "vendor/xsd-schema/Cargo.toml")}],
                         "resolve": {"nodes": [{"id": "backend", "features": ["default", "unicode-normalization"]}]}}

    def test_default_surface(self):
        self.assertEqual(verify_features.check(self.metadata), ["default", "unicode-normalization"])

    def test_unexpected_unification_and_missing_default(self):
        for features in [["default", "unicode-normalization", "xsd11"], ["default", "unicode-normalization", "async"],
                         ["default", "unicode-normalization", "compose"], ["default"], []]:
            with self.subTest(features=features):
                metadata = copy.deepcopy(self.metadata)
                metadata["resolve"]["nodes"][0]["features"] = features
                with self.assertRaisesRegex(ValueError, "features"):
                    verify_features.check(metadata)


if __name__ == "__main__":
    unittest.main()
