import json
from pathlib import Path
import shutil
import tempfile
import unittest
import verify_parser as verifier


class ParserInventory(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        shutil.copytree(verifier.ROOT / 'vendor/xml-rs', self.root / 'vendor/xml-rs')
        for name in ['xml-rs.patch', 'xml-rs.provenance.json']:
            shutil.copyfile(verifier.ROOT / 'vendor' / name, self.root / 'vendor' / name)
        self.addCleanup(self.temp.cleanup)

    def test_exact_inventory(self):
        self.assertEqual(verifier.verify(self.root), verifier.verify())

    def test_missing(self):
        (self.root / 'vendor/xml-rs/LICENSE').unlink()
        with self.assertRaises(ValueError): verifier.verify(self.root)

    def test_extra(self):
        (self.root / 'vendor/xml-rs/extra').write_bytes(b'')
        with self.assertRaises(ValueError): verifier.verify(self.root)

    def test_changed(self):
        (self.root / 'vendor/xml-rs/LICENSE').write_bytes(b'changed')
        with self.assertRaises(ValueError): verifier.verify(self.root)

    def test_patch_change(self):
        (self.root / 'vendor/xml-rs.patch').write_bytes(b'changed')
        with self.assertRaises(ValueError): verifier.verify(self.root)

    def test_unsafe_manifest_path(self):
        path = self.root / 'vendor/xml-rs.provenance.json'
        data = json.loads(path.read_text()); data['files'][0]['path'] = '../outside'
        path.write_text(json.dumps(data))
        with self.assertRaises(ValueError): verifier.verify(self.root)

    def test_wrong_archive(self):
        archive = self.root / 'bad.crate'; archive.write_bytes(b'bad')
        with self.assertRaises(ValueError): verifier.verify(self.root, archive)

    def test_symlink(self):
        target = self.root / 'vendor/xml-rs/link'
        try: target.symlink_to('LICENSE')
        except OSError as error: self.skipTest(str(error))
        with self.assertRaises(ValueError): verifier.verify(self.root)

    def test_duplicate_manifest_key(self):
        path = self.root / 'vendor/xml-rs.provenance.json'
        text = path.read_text()
        path.write_text(text.replace('{', '{"format":"einvoice-xml-parser-source/1",', 1))
        with self.assertRaises(ValueError): verifier.verify(self.root)
