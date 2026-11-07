"""Bounded producer I/O checks using real child processes."""
import json
import os
from pathlib import Path, PureWindowsPath
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch
import observe


class ProducerIO(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='bsc8-producer-io-')
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def test_exact_short_over_and_directory(self):
        path = self.root / 'input'
        for size in [0, 2, 3, 4]:
            path.write_bytes(b'x' * size)
            if size <= 3:
                self.assertEqual(observe.read_regular(path, 3), b'x' * size)
            else:
                with self.assertRaises(ValueError):
                    observe.read_regular(path, 3)
        with self.assertRaises(ValueError):
            observe.read_regular(self.root, 3)

    @unittest.skipUnless(hasattr(os, 'mkfifo'), 'Unix FIFO policy')
    def test_fifo_rejected_without_a_writer(self):
        path = self.root / 'fifo'
        os.mkfifo(path)
        with self.assertRaises(ValueError):
            observe.read_regular(path, 3)

    @unittest.skipUnless(os.name == 'posix', 'Unix symlink policy')
    def test_symlink_rejected(self):
        source = self.root / 'source'
        source.write_bytes(b'x')
        link = self.root / 'link'
        link.symlink_to(source)
        with self.assertRaises(ValueError):
            observe.read_regular(link, 3)

    def status(self, stage):
        return json.loads((self.root / (stage + '.status.json')).read_bytes())

    def test_failed_child_preserves_both_streams_and_status(self):
        with self.assertRaises(RuntimeError):
            observe.run([sys.executable, '-c', 'import sys; print("partial"); print("specific error", file=sys.stderr); sys.exit(7)'], self.root, 'failure')
        self.assertEqual((self.root / 'failure.stdout').read_bytes(), b'partial' + os.linesep.encode())
        self.assertEqual((self.root / 'failure.stderr').read_bytes(), b'specific error' + os.linesep.encode())
        self.assertEqual(self.status('failure')['returncode'], 7)
        self.assertEqual(self.status('failure')['truncated'], [False, False])
        self.assertFalse((self.root / 'receipt.json').exists())

    def test_success_without_observation(self):
        result = observe.run([sys.executable, '-c', 'print("tool identity")'], self.root, 'identity')
        self.assertEqual(result.stdout, b'tool identity' + os.linesep.encode())
        self.assertEqual(self.status('identity')['returncode'], 0)

    def test_raw_child_line_endings_are_preserved_without_normalization(self):
        payload = b'CRLF\r\nLF\nCR\rEND'
        code = f'import sys; sys.stdout.buffer.write({payload!r}); sys.stderr.buffer.write({payload!r})'
        result = observe.run([sys.executable, '-c', code], self.root, 'raw-endings')
        self.assertEqual(result.stdout, payload)
        self.assertEqual(result.stderr, payload)
        for stream in ['stdout', 'stderr']:
            self.assertEqual((self.root / ('raw-endings.' + stream)).read_bytes(), payload)
            self.assertEqual(self.status('raw-endings')[stream]['sha256'], observe.digest(payload))

    def test_late_real_read_cannot_turn_incomplete_capture_into_success(self):
        payload = b'direct-child-real-bytes\n'
        release = threading.Event()
        intercepted = threading.Event()
        held = []
        real_read, real_write = os.read, Path.write_bytes

        def scheduled_read(fd, count):
            data = real_read(fd, count)
            thread = threading.current_thread()
            if data == payload and thread is not threading.main_thread():
                held.append(thread)
                intercepted.set()
                if not release.wait(10):
                    raise OSError('test reader scheduling gate timed out')
            return data

        def scheduled_write(path, data):
            if path == self.root / 'late.stdout':
                self.assertTrue(intercepted.is_set())
                release.set()
                held[0].join(timeout=3)
                self.assertFalse(held[0].is_alive())
            return real_write(path, data)

        try:
            with patch.object(os, 'read', scheduled_read), patch.object(Path, 'write_bytes', scheduled_write):
                with self.assertRaises(RuntimeError):
                    observe.run([sys.executable, '-c', f'import os; os.write(1,{payload!r})'],
                                self.root, 'late', limit=31, timeout=5)
        finally:
            release.set()
            for thread in held:
                thread.join(timeout=3)
                self.assertFalse(thread.is_alive())
        self.assertEqual(self.status('late')['returncode'], 0)
        self.assertFalse(self.status('late')['pipe_drain_complete'])
        self.assertEqual((self.root / 'late.stdout').read_bytes(), b'')

    def test_windows_inventory_keys_retain_backend_membership_and_digest(self):
        actual = observe.inventory()
        expected = json.loads((observe.ROOT / 'vendor/xsd-schema.provenance.json').read_bytes())
        members = {key.removeprefix('vendor/xsd-schema/'): value['sha256']
                   for key, value in actual.items() if key.startswith('vendor/xsd-schema/')}
        self.assertEqual(members, {row['path']: row['patched']['sha256'] for row in expected['files']})
        windows_root = PureWindowsPath('C:/review/repo')
        with patch.object(observe, 'ROOT', windows_root):
            windows = {observe.inventory_key(windows_root / key): value for key, value in actual.items()}
        self.assertEqual(windows, actual)
        self.assertEqual(observe.backend_identity(windows), observe.backend_identity(actual))
        changed = {**windows, 'vendor/xsd-schema/src/lib.rs': observe.identify(b'changed')}
        self.assertNotEqual(observe.backend_identity(changed), observe.backend_identity(actual))
        with self.assertRaises(ValueError):
            observe.backend_identity({})

    def test_output_limit_preserves_prefix_and_fails(self):
        with self.assertRaises(RuntimeError):
            observe.run([sys.executable, '-c', 'import os; os.write(1,b"x"*10000)'], self.root, 'overflow', limit=31)
        self.assertEqual((self.root / 'overflow.stdout').read_bytes(), b'x' * 31)
        self.assertTrue(self.status('overflow')['truncated'][0])

    def test_timeout_not_completion(self):
        with self.assertRaises(RuntimeError):
            observe.run([sys.executable, '-c', 'import time; time.sleep(30)'], self.root, 'timeout', timeout=0.1)
        self.assertTrue(self.status('timeout')['timed_out'])

    def test_launch_failure_without_fabricated_status(self):
        with self.assertRaises(RuntimeError):
            observe.run([str(self.root / 'absent-executable')], self.root, 'launch')
        self.assertIsNone(self.status('launch')['returncode'])
        self.assertIsNotNone(self.status('launch')['launch_error'])
        self.assertEqual((self.root / 'launch.stdout').read_bytes(), b'')

    def test_actual_input_binding_includes_present_digest(self):
        expected = observe.identify(b'<r/>')
        good = {'input_bytes': 4, 'input_sha256': expected['sha256']}
        observe.require_input(good, expected)
        for bad in [dict(good, input_bytes=5), dict(good, input_sha256=None),
                    dict(good, input_sha256=observe.digest(b'<s/>'))]:
            with self.assertRaises(ValueError):
                observe.require_input(bad, expected)


if __name__ == '__main__':
    unittest.main()
