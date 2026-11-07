"""Small byte, JSON and filesystem boundaries shared by the two public tools."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat


class Invalid(ValueError):
    pass


def require(condition, code):
    if not condition:
        raise Invalid(code)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + '\n').encode('utf-8')


def strict_json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, 'duplicate_json_key')
            result[key] = value
        return result

    def nonfinite(_):
        raise Invalid('nonfinite_json')

    try:
        value = json.loads(data.decode('utf-8'), object_pairs_hook=pairs, parse_constant=nonfinite)
        # Reject lone surrogate escapes and overflowing JSON floats too.
        json.dumps(value, ensure_ascii=False, allow_nan=False).encode('utf-8')
        return value
    except (UnicodeError, ValueError) as exc:
        if isinstance(exc, Invalid):
            raise
        raise Invalid('invalid_json') from exc


def json_lines(data):
    require(bool(data) and data.endswith(b'\n'), 'partial_jsonl')
    lines = data[:-1].split(b'\n')
    require(all(lines), 'blank_jsonl')
    return [strict_json(line) for line in lines]


def relative(name):
    require(isinstance(name, str) and bool(name), 'unsafe_path')
    require('\\' not in name and ':' not in name and not any(ord(c) < 32 for c in name), 'unsafe_path')
    p = PurePosixPath(name)
    require(not p.is_absolute() and p.as_posix() == name, 'unsafe_path')
    require(all(x not in ('', '.', '..') for x in p.parts), 'unsafe_path')
    return p


def no_symlinks(path):
    path = Path(os.path.abspath(path))
    for p in (path, *path.parents):
        require(not p.is_symlink(), 'symlink_path')
    return path


def read_file(path):
    """Open through directory descriptors; digest and parser share this snapshot."""
    path = no_symlinks(path)
    fd = os.open(path.anchor, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in path.parts[1:-1]:
            next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = next_fd
        file_fd = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
        with os.fdopen(file_fd, 'rb') as stream:
            before = os.fstat(stream.fileno())
            require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1, 'file_alias_or_type')
            data = stream.read()
            after = os.fstat(stream.fileno())
            require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns) ==
                    (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns), 'read_race')
            require(len(data) == before.st_size, 'read_race')
            return data
    finally:
        os.close(fd)


def check_bytes(data, pin, code='digest_mismatch'):
    require(sha(data) == pin['sha256'] and ('bytes' not in pin or len(data) == pin['bytes']), code)


def fresh_path(path, protected, directory=False):
    path = no_symlinks(path)
    require(not os.path.lexists(path), 'output_exists')
    for item in protected:
        item = Path(item).resolve()
        require(not path.is_relative_to(item) and not item.is_relative_to(path), 'output_input_alias')
    require(path.parent.is_dir(), 'output_parent_missing')
    if directory:
        path.mkdir(mode=0o700)
    return path


def write_new(path, data):
    path = no_symlinks(path)
    with path.open('xb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def save_new(path, value):
    write_new(path, json_bytes(value))


class Report:
    """Exclusive report, replaced only while the original owned inode survives."""
    def __init__(self, path, protected):
        self.path = fresh_path(path, protected)
        save_new(self.path, {'status': 'incomplete'})
        self.identity = self.path.stat()

    def finish(self, value):
        current = self.path.lstat()
        require((current.st_dev, current.st_ino, current.st_nlink) ==
                (self.identity.st_dev, self.identity.st_ino, 1), 'report_replaced')
        temp = self.path.with_name(self.path.name + '.finishing')
        write_new(temp, json_bytes(value))
        os.replace(temp, self.path)
