#!/usr/bin/env python3
"""Bounded offline verifier for one reviewed producer's retained attempt.

Stdlib only, one source module, no historical imports or helper bytecode caches.
Use Python 3.12 in isolated mode (-I); installation source is the trust anchor.
"""
import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import stat
import sys

DESCRIPTOR_SHA256 = 'c9e77ebb9a00122e80683ea2eed7d7b85efb20915ef096a3b0408d8e7602ef53'
INPUT_MAX = 16 * 1024 * 1024
JSON_MAX = 32 * 1024 * 1024
AGGREGATE_MAX = 160 * 1024 * 1024
FILE_MAX = 12
DEPTH_MAX = 64
NODE_MAX = 200_000
COLLECTION_MAX = 50_000
STRING_MAX = 1024 * 1024
STRINGS_MAX = 8 * 1024 * 1024
U64 = (1 << 64) - 1
U32 = (1 << 32) - 1
ISIZE_MAX = (1 << 63) - 1
ATTEMPT_NAMES = frozenset(['input.xml', 'source-inventory.json', 'receipt.json', 'failure.json'] +
                          [s + e for s in ('complete', 'partial')
                           for e in ('.json', '.status.json', '.stderr')])


class Refusal(Exception):
    def __init__(self, message, code=1, category='malformed_or_contradictory'):
        super().__init__(message)
        self.code, self.category = code, category


def require(condition, message):
    if not condition:
        raise Refusal(message)


def unavailable(message):
    raise Refusal(message, 2, 'unsupported_or_unavailable')


def resource(message):
    raise Refusal(message, 2, 'consumer_resource_failure')


def identify(data):
    return {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


class Files:
    """Hold a directory FD opened component by component, never following links.

    Each allowlisted file is opened once and its held bytes are used for both
    parsing and hashing. Nonblocking opens precede fstat, including raced FIFOs.
    Reject hard-linked files. This is bounded I/O, not a filesystem deadline.
    """
    def __init__(self, directory, names=ATTEMPT_NAMES):
        if not (os.name == 'posix' and hasattr(os, 'O_NOFOLLOW') and
                hasattr(os, 'O_DIRECTORY') and os.open in os.supports_dir_fd):
            unavailable('safe nofollow directory-FD profile unavailable')
        path = os.fspath(directory)
        if not os.path.isabs(path):
            path = os.getcwd() + '/' + path
        parts = path.split('/')
        require('..' not in parts, 'parent traversal in attempt directory')
        flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_NONBLOCK
        fd = os.open('/', flags)
        try:
            for part in parts:
                if part in ('', '.'):
                    continue
                child = os.open(part, flags, dir_fd=fd)
                os.close(fd)
                fd = child
        except BaseException:
            os.close(fd)
            raise
        self.fd, self.names, self.held, self.total = fd, names, {}, 0

    def __enter__(self):
        return self

    def __exit__(self, *args):
        os.close(self.fd)

    def read(self, name, limit=JSON_MAX, optional=False):
        require(name in self.names, 'file name outside fixed allowlist')
        if name in self.held:
            return self.held[name]
        if len(self.held) >= FILE_MAX:
            resource('consumer file count limit')
        try:
            fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=self.fd)
        except FileNotFoundError:
            if optional:
                self.held[name] = None
                return None
            unavailable('required attempt file absent')
        try:
            before = os.fstat(fd)
            require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1,
                    'single-link regular file required')
            if before.st_size > limit:
                resource('consumer file byte limit')
            chunks, count = [], 0
            while count <= limit:
                chunk = os.read(fd, min(65536, limit + 1 - count))
                if not chunk:
                    break
                count += len(chunk)
                if self.total + count > AGGREGATE_MAX:
                    resource('consumer aggregate byte limit')
                chunks.append(chunk)
            if count > limit:
                resource('consumer file grew beyond byte limit')
            after = os.fstat(fd)
            require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns,
                     before.st_ctime_ns, before.st_nlink) ==
                    (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns,
                     after.st_ctime_ns, after.st_nlink), 'file changed while reading')
            data = b''.join(chunks)
            require(len(data) == before.st_size, 'short or changing file read')
        finally:
            os.close(fd)
        self.total += len(data)
        self.held[name] = data
        return data


def strict_json(data):
    if len(data) > JSON_MAX:
        resource('consumer JSON byte limit')
    # Bound nesting and materialization BEFORE json.loads. Quotes/escapes count
    # raw token bytes; json.loads remains the sole JSON syntax parser.
    depth = tokens = run = 0
    quoted = escaped = False
    for byte in data:
        if quoted:
            run += 1
            if run > STRING_MAX:
                resource('consumer JSON string token limit')
            if escaped:
                escaped = False
            elif byte == 92:
                escaped = True
            elif byte == 34:
                quoted = False
        elif byte == 34:
            quoted, run = True, 0
            tokens += 1
        elif byte in (91, 123):
            depth += 1
            tokens += 1
            if depth > DEPTH_MAX:
                resource('consumer JSON nesting limit')
        elif byte in (93, 125):
            depth -= 1
        elif byte == 44:
            tokens += 1
        if tokens > NODE_MAX:
            resource('consumer JSON token limit')

    def pairs(values):
        result = {}
        for key, value in values:
            require(key not in result, 'duplicate JSON key')
            result[key] = value
        return result

    def integer(token):
        require(len(token) <= 20 and token != '-0', 'invalid integer token')
        return int(token)

    def noninteger(_):
        raise Refusal('noninteger or nonfinite JSON number')

    try:
        text = data.decode('utf-8', errors='strict')
        value = json.loads(text, object_pairs_hook=pairs, parse_int=integer,
                           parse_float=noninteger, parse_constant=noninteger)
    except (ValueError, UnicodeError, RecursionError) as error:
        raise Refusal('invalid UTF-8 or JSON syntax') from error
    stack, nodes, strings = [value], 0, 0
    while stack:
        item = stack.pop()
        nodes += 1
        if nodes > NODE_MAX:
            resource('consumer JSON node limit')
        if isinstance(item, str):
            require(not any(0xD800 <= ord(c) <= 0xDFFF for c in item), 'invalid Unicode scalar')
            size = len(item.encode('utf-8'))
            strings += size
            if size > STRING_MAX or strings > STRINGS_MAX:
                resource('consumer decoded string limit')
        elif isinstance(item, (list, dict)):
            if len(item) > COLLECTION_MAX:
                resource('consumer collection limit')
            stack.extend(item if isinstance(item, list) else item.values())
            if isinstance(item, dict):
                stack.extend(item.keys())
    return value


# Declarative exact serde grammar. Optional means explicit JSON null, never an
# omitted key. Unit enums and externally tagged payload enums stay distinct.
def opt(schema):
    return ('optional', schema)


def vec(schema):
    return ('vector', schema)


def enum(*variants):
    return ('unit', frozenset(variants))


def tagged(variants):
    return ('tagged', variants)


NAME = {'namespace': opt('str'), 'local': 'str'}
SUBJECT = tagged({'Element': NAME, 'Attribute': NAME})
PATH_STEP = tagged({'Element': {'name': NAME, 'position': 'u64'}, 'Attribute': {'name': NAME}})
REFERENCE = tagged({'Node': {'preorder': 'u64'},
                    'Attribute': {'owner_preorder': 'u64', 'attribute': 'u64'}})
SPAN = tagged({t: {'start': 'u64', 'end': 'u64'} for t in ('Utf8Bytes', 'UnicodeScalars')})
TYPE_CODES = ('None Item Node Document Element Attribute Namespace ProcessingInstruction Comment Text '
              'AnyType AnySimpleType AnyAtomicType UntypedAtomic String NormalizedString Token Language '
              'NmToken Name NCName Id IdRef Entity Boolean Decimal Float Double Integer NonPositiveInteger '
              'NegativeInteger Long Int Short Byte NonNegativeInteger UnsignedLong UnsignedInt UnsignedShort '
              'UnsignedByte PositiveInteger Duration DateTime Time Date GYearMonth GYear GMonthDay GDay '
              'GMonth YearMonthDuration DayTimeDuration DateTimeStamp HexBinary Base64Binary AnyUri QName '
              'Notation NmTokens IdRefs Entities Error').split()
TYPE_CODE = enum(*TYPE_CODES)
FACET_VARIANTS = {
    'LengthViolation': {'message': 'str'}, 'MinLengthViolation': {'actual': 'u64', 'min': 'u64'},
    'MaxLengthViolation': {'actual': 'u64', 'max': 'u64'},
    'PatternViolation': {'value': 'str', 'pattern': 'str'}, 'EnumerationViolation': {'value': 'str'},
    'MinInclusiveViolation': {'value': 'str', 'min': 'str'},
    'MinExclusiveViolation': {'value': 'str', 'min': 'str'},
    'MaxInclusiveViolation': {'value': 'str', 'max': 'str'},
    'MaxExclusiveViolation': {'value': 'str', 'max': 'str'},
    'TotalDigitsViolation': {'actual': 'u32', 'max': 'u32'},
    'FractionDigitsViolation': {'actual': 'u32', 'max': 'u32'},
    'ExplicitTimezoneViolation': {'message': 'str'}, 'InvalidPattern': {'pattern': 'str', 'message': 'str'},
    'DerivationRestriction': {'message': 'str'},
    'FixedFacetViolation': {'facet_name': 'str', 'base_value': 'str', 'derived_value': 'str'},
    'ConflictingFacets': {'message': 'str'}, 'NotApplicable': {'facet': 'str', 'type_name': 'str'},
}
FACET = tagged(FACET_VARIANTS)
VALUE_VARIANTS = {
    'InvalidLexical': {'value': 'str', 'type_name': 'str', 'message': 'str'},
    'FacetViolation': FACET, 'TypeError': {'expected': TYPE_CODE, 'actual': TYPE_CODE},
    'RangeError': {'value': 'str', 'type_name': 'str'},
}
VALUE = tagged(VALUE_VARIANTS)
NATIVE_VARIANTS = {k: None for k in ('IncompleteContent ProhibitedAttribute AttributeNotAllowed '
                                    'UndeclaredElement NilNotAllowed NilWithFixedValue NilWithContent').split()}
NATIVE_VARIANTS.update({'Value': VALUE, 'Facet': FACET, 'UnexpectedChild': {'found': NAME},
                        'MissingRequiredAttribute': {'name': NAME},
                        'AttributeFixedValue': {'expected': 'str', 'actual': 'str'},
                        'XsiTypeUnresolved': {'value': 'str'},
                        'XsiTypeDerivation': {'value': 'str', 'blocked': 'bool'}})
NATIVE = tagged(NATIVE_VARIANTS)
FAILURE_VARIANTS = {k: None for k in ('InvalidLimits ArithmeticOverflow Allocation Cancelled Deadline '
                                    'EpochRetired UnsupportedEvidence SourceJoin Consistency').split()}
FAILURE_VARIANTS.update({'Limit': {'dimension': 'str', 'limit': 'u64'},
                         'UnsupportedCapability': {'capability': 'str'}, 'Backend': {'phase': 'str'}})
FAILURE = tagged(FAILURE_VARIANTS)
COMPONENT = {'kind': enum('SimpleType', 'ComplexType', 'Attribute', 'Element'), 'name': opt(NAME),
             'global': opt('bool'), 'document_uri': opt('str'), 'schema_span': opt(('tuple', 'u64', 'u64'))}
EVIDENCE = {'cause': opt(NATIVE), **{k: opt(COMPONENT) for k in
                                 ('governing_type', 'actual_type', 'component', 'facet_origin')}}
JOIN = {'owner': SPAN, 'owner_reference': REFERENCE, 'subject_reference': REFERENCE,
        'subject': SPAN, 'attribute_value': opt(SPAN), 'line': 'u64', 'column': 'u64'}
DIAGNOSTIC = {'code': 'str', 'message': 'str',
              'raw_location': opt({'base_uri': 'str', 'line': 'u64', 'column': 'u64'}),
              'subject': opt(SUBJECT), 'path': opt(vec(PATH_STEP)), 'element_path': opt('str'),
              'evidence': opt(EVIDENCE),
              'evidence_availability': enum('Captured', 'Unavailable', 'NotExportedByWarningApi'),
              'source': opt(JOIN), 'triggering_occurrence': opt(vec(PATH_STEP)), 'causal_parent': opt('u64')}
EVENT = {'ordinal': 'u64', 'severity': enum('Error', 'Warning'), **DIAGNOSTIC}
LIMIT_FIELDS = ('input_bytes nodes depth attributes_per_element declarations_per_element '
                'namespaces_per_element name_bytes uri_bytes value_bytes temporary_bytes work '
                'diagnostic_count diagnostic_bytes terminal_bytes').split()
LIMITS = {k: 'u64' for k in LIMIT_FIELDS}
OBSERVATION = {'input_sha256': opt('hash'), 'input_bytes': 'u64', 'encoding': 'str', 'bom_bytes': 'u64',
               'validated_root': SPAN, 'validated_root_reference': opt(REFERENCE),
               'assessment': enum('Valid', 'Invalid', 'Incomplete'),
               'root_validity': opt(enum('Valid', 'Invalid', 'NotKnown')), 'events': vec(EVENT),
               'failure': opt(FAILURE), 'finalization_attempted': 'bool', 'finalization_succeeded': 'bool',
               'finalization_error': opt(DIAGNOSTIC), 'operational_failure': opt(DIAGNOSTIC),
               'terminal_capture_failure': opt(FAILURE), 'work': 'u64',
               'retained_diagnostic_bytes': 'u64', 'observed_callbacks': 'u64'}
SCHEMA = {'member': 'str', 'backend_uri': 'str', 'bytes': 'u64', 'sha256': 'hash'}
ARTIFACT = {'format': 'str', 'scope': 'str', 'schemas': vec(SCHEMA), 'configured_limits': LIMITS,
            'observation': OBSERVATION, 'epoch_retired': 'bool', 'epoch_remaining_requests': 'u64',
            'build_receipt': 'str'}
BINDING = {'bytes': 'u64', 'sha256': 'hash'}
STATUS = {'stage': 'str', 'command': vec('str'), 'cwd': 'str', 'returncode': opt('i32'),
          'launch_error': opt('str'), 'timeout_seconds': 'u64', 'timed_out': 'bool',
          'per_stream_limit': 'u64', 'truncated': ('tuple', 'bool', 'bool'),
          'read_errors': ('tuple', opt('str'), opt('str')), 'pipe_drain_complete': 'bool',
          'stdout': {'path': 'str', **BINDING}, 'stderr': {'path': 'str', **BINDING}}
BUILD_ARTIFACT = {'executable': opt('str'), 'features': vec('str'), 'filenames': vec('str'),
                  'fresh': 'bool', 'manifest_path': 'str', 'package_id': 'str',
                  'profile': {'debug_assertions': 'bool', 'debuginfo': opt('u32'), 'opt_level': 'str',
                              'overflow_checks': 'bool', 'test': 'bool'},
                  'reason': 'str', 'target': {'crate_types': vec('str'), 'doc': 'bool', 'doctest': 'bool',
                                           'edition': 'str', 'kind': vec('str'), 'name': 'str',
                                           'src_path': 'str', 'test': 'bool'}}
RECEIPT = {**{k: BINDING for k in ('archive', 'generator_accounting', 'input', 'source_inventory',
                                   'build_stdout', 'build_stderr', 'cargo_metadata',
                                   'complete_output', 'partial_output')},
           'kind': 'str', 'loaded_schema_bytes': ('map', BINDING), 'input_snapshot': 'str',
           'git_head': 'str', 'git_status_porcelain': 'str', 'source_unchanged_during_build_and_run': 'bool',
           'rustc': 'str', 'cargo': 'str', 'build_command': vec('str'), 'backend_features': vec('str'),
           'backend_build_artifact': BUILD_ARTIFACT, 'backend_consumed_source_identity': 'hash',
           'executable': {'path': 'str', **BINDING}, 'complete_command': vec('str'),
           'partial_command': vec('str'), 'target_directory': opt('str'), 'limits': 'str'}
FAILURE_FILE = {'kind': 'str', 'error_type': 'str', 'message': 'str'}


def grammar(value, schema):
    if isinstance(schema, dict):
        require(type(value) is dict and value.keys() == schema.keys(), 'missing or unknown struct field')
        for key, child in schema.items():
            grammar(value[key], child)
    elif isinstance(schema, tuple):
        kind, *args = schema
        if kind == 'optional':
            if value is not None:
                grammar(value, args[0])
        elif kind in ('vector', 'tuple'):
            require(type(value) is list, 'array required')
            if kind == 'tuple':
                require(len(value) == len(args), 'tuple length mismatch')
            for index, item in enumerate(value):
                grammar(item, args[0] if kind == 'vector' else args[index])
        elif kind == 'map':
            require(type(value) is dict, 'map required')
            for item in value.values():
                grammar(item, args[0])
        elif kind == 'unit':
            require(type(value) is str and value in args[0], 'unknown unit enum variant')
        elif kind == 'tagged':
            variants = args[0]
            if type(value) is str:
                require(value in variants and variants[value] is None, 'unknown or nonunit variant')
            else:
                require(type(value) is dict and len(value) == 1, 'single externally tagged variant required')
                tag = next(iter(value))
                require(tag in variants and variants[tag] is not None, 'unknown or unit payload variant')
                grammar(value[tag], variants[tag])
    elif schema in ('u64', 'u32', 'i32'):
        low, high = (-(1 << 31), (1 << 31) - 1) if schema == 'i32' else (0, U64 if schema == 'u64' else U32)
        require(type(value) is int and low <= value <= high, 'integer type or range mismatch')
    elif schema == 'bool':
        require(type(value) is bool, 'boolean required')
    elif schema in ('str', 'hash'):
        require(type(value) is str, 'string required')
        if schema == 'hash':
            require(len(value) == 64 and all(c in '0123456789abcdef' for c in value), 'SHA256 syntax mismatch')
    else:
        raise RuntimeError('consumer grammar configuration error')


def variant(value):
    return (value, None) if isinstance(value, str) else next(iter(value.items()))


def pinned_descriptor():
    with Files(os.path.dirname(os.path.abspath(__file__)), {'producer-2d82953.json'}) as files:
        raw = files.read('producer-2d82953.json')
    require(identify(raw)['sha256'] == DESCRIPTOR_SHA256, 'consumer compatibility descriptor changed')
    return strict_json(raw)


class SourceChecks:
    """Necessary coordinate/reference consistency only; no XML tree parser."""
    def __init__(self, raw, observation, schemas=()):
        self.schema_sizes = {item['backend_uri']: item['bytes'] for item in schemas}
        encoding, bom = observation['encoding'], observation['bom_bytes']
        codecs = {'UTF-8': ('utf-8', b'\xef\xbb\xbf'),
                  'UTF-16LE': ('utf-16-le', b'\xff\xfe'),
                  'UTF-16BE': ('utf-16-be', b'\xfe\xff'),
                  'ISO-8859-1': ('latin1', b''), 'US-ASCII': ('ascii', b'')}
        if encoding not in codecs:
            unavailable('unsupported source encoding profile')
        codec, marker = codecs[encoding]
        actual_bom = 3 if raw.startswith(b'\xef\xbb\xbf') else 2 if raw.startswith((b'\xff\xfe', b'\xfe\xff')) else 0
        require(bom == actual_bom, 'BOM byte count mismatch')
        if encoding.startswith('UTF-16'):
            require(bom == 2 and raw.startswith(marker), 'UTF16 BOM mismatch')
        elif bom:
            require(encoding == 'UTF-8' and raw.startswith(marker), 'encoding BOM conflict')
        try:
            text = raw[bom:].decode(codec, errors='strict')
        except UnicodeError as error:
            raise Refusal('source encoding decoding failure') from error
        # Only inspect the optional declaration's encoding label, not XML syntax.
        # A complete lexical/tree/header authority is explicitly unavailable.
        if text.startswith('<?xml') and len(text) > 5 and text[5] in ' \t\r\n':
            end = text.find('?>', 0, 4096)
            if end < 0:
                unavailable('source declaration outside supported header bound')
            import re
            match = re.search(r'''\bencoding\s*=\s*(['"])([A-Za-z0-9._-]+)\1''', text[:end])
            label = match.group(2).lower() if match else None
        else:
            label = None
        labels = {'UTF-8': {'utf-8', 'utf8'}, 'UTF-16LE': {'utf-16', 'utf16'},
                  'UTF-16BE': {'utf-16', 'utf16'}, 'ISO-8859-1': {'iso-8859-1', 'latin1'},
                  'US-ASCII': {'us-ascii', 'ascii'}}
        require((label in labels[encoding]) if label is not None else
                encoding in ('UTF-8', 'UTF-16LE', 'UTF-16BE'), 'source encoding label conflict')
        self.raw, self.bom, self.scalar_count = raw, bom, len(text)
        self.unit = 'Utf8Bytes' if encoding == 'UTF-8' else 'UnicodeScalars'
        self.references, self.locations, self.root_name = {}, {}, None
        self.root = self.span(observation['validated_root'], nonempty=True)
        rootref = observation['validated_root_reference']
        if rootref is not None:
            require(variant(rootref)[0] == 'Node', 'root reference must be Node')
            require(rootref['Node']['preorder'] > 0, 'root cannot be document node')
            self.remember(rootref, self.root)
        self.rootref = rootref

    def span(self, value, nonempty=False):
        unit, pair = variant(value)
        require(unit == self.unit, 'source span coordinate unit mismatch')
        start, end = pair['start'], pair['end']
        require(start <= end and (not nonempty or start < end), 'source span ordering mismatch')
        bound = len(self.raw) if unit == 'Utf8Bytes' else self.scalar_count
        require(end <= bound, 'source span exceeds original input')
        if unit == 'Utf8Bytes':
            require(start >= self.bom, 'source span includes BOM')
            for offset in (start, end):
                require(offset == len(self.raw) or self.raw[offset] & 0xC0 != 0x80,
                        'source span splits UTF8 code point')
        return start, end

    @staticmethod
    def contains(outer, inner):
        require(outer[0] <= inner[0] <= inner[1] <= outer[1], 'source span containment mismatch')

    def remember(self, reference, span, name=None, path=None, location=None):
        key = json.dumps(reference, sort_keys=True, separators=(',', ':'))
        previous = self.references.setdefault(key, [span, None, None])
        require(previous[0] == span, 'same reference has different spans')
        for index, value in ((1, name), (2, path)):
            if value is not None:
                require(previous[index] is None or previous[index] == value,
                        'same reference has different name or path')
                previous[index] = value
        if location is not None:
            require(self.locations.setdefault(key, location) == location,
                    'same reference has different locations')

    def diagnostic(self, diagnostic, terminal=False):
        d = diagnostic
        warning = d.get('severity') == 'Warning'
        require(d['causal_parent'] is None and d['triggering_occurrence'] is None,
                'producer does not export causal or triggering links')
        evidence, availability = d['evidence'], d['evidence_availability']
        if warning:
            require(evidence is None and availability == 'NotExportedByWarningApi' and
                    d['element_path'] is None, 'warning evidence missingness mismatch')
        else:
            require(availability == ('Captured' if evidence is not None else 'Unavailable'),
                    'error evidence missingness mismatch')
        if evidence is not None:
            for field in ('governing_type', 'actual_type', 'component', 'facet_origin'):
                component = evidence[field]
                if component is None:
                    continue
                uri = component['document_uri']
                if uri is not None:
                    require(uri in self.schema_sizes, 'schema component URI outside declared closure')
                if component['schema_span'] is not None:
                    start, end = component['schema_span']
                    require(start <= end, 'schema span ordering mismatch')
                    if uri is not None:
                        require(end <= self.schema_sizes[uri], 'schema span exceeds declared schema size')
                    # Size pins are references, not supplied XSD byte proof. A
                    # tuple never becomes an instance span or declaration proof.
        if terminal:
            require(d['source'] is None, 'terminal diagnostic cannot have source join')
            return
        subject, path, join = d['subject'], d['path'], d['source']
        require((join is not None) == (subject is not None and path is not None),
                'event subject/path/source presence mismatch')
        if join is None:
            return
        require(path and 'Element' in path[0] and path[0]['Element']['position'] == 1,
                'joined path must start at full root position one')
        for index, step in enumerate(path):
            kind, value = variant(step)
            if kind == 'Element':
                require(value['position'] > 0, 'zero element occurrence')
            else:
                require(index == len(path) - 1 and index > 0, 'attribute path role mismatch')
        rootname = path[0]['Element']['name']
        require(self.root_name is None or self.root_name == rootname, 'joined root names differ')
        self.root_name = rootname
        subjectkind, subjectname = variant(subject)
        pathkind, last = variant(path[-1])
        require(pathkind == subjectkind and last['name'] == subjectname, 'subject and path disagree')
        owner = self.span(join['owner'], nonempty=True)
        target = self.span(join['subject'], nonempty=True)
        self.contains(self.root, owner)
        self.contains(owner, target)
        require(variant(join['owner_reference'])[0] == 'Node', 'owner reference must be Node')
        owner_preorder = join['owner_reference']['Node']['preorder']
        require(owner_preorder > 0, 'owner cannot be document node')
        require(join['line'] > 0 and join['column'] > 0, 'source location must be one based')
        ownerpath = path if subjectkind == 'Element' else path[:-1]
        ownername = ownerpath[-1]['Element']['name']
        if len(ownerpath) == 1:
            require(owner == self.root and join['owner_reference'] == self.rootref,
                    'root owner and validated root disagree')
        self.remember(join['owner_reference'], owner, ownername, ownerpath)
        if subjectkind == 'Element':
            require(join['subject_reference'] == join['owner_reference'] and target == owner and
                    join['attribute_value'] is None, 'element source role mismatch')
        else:
            require(variant(join['subject_reference'])[0] == 'Attribute' and
                    join['subject_reference']['Attribute']['owner_preorder'] == owner_preorder and
                    join['attribute_value'] is not None, 'attribute source role mismatch')
            self.contains(target, self.span(join['attribute_value']))
        self.remember(join['subject_reference'], target, subjectname, path,
                      (join['line'], join['column']))


def failure_relations(failure, limits):
    if failure is None:
        return
    tag, payload = variant(failure)
    if tag == 'Backend':
        require(payload['phase'] in ('operational', 'finalization'), 'unknown backend phase')
    elif tag == 'Limit':
        dimension = payload['dimension']
        mapped = {'namespace_temporary_bytes': 'temporary_bytes',
                  'walker_temporary_bytes': 'temporary_bytes', 'path_temporary_bytes': 'temporary_bytes'}
        key = mapped.get(dimension, dimension)
        require(key in limits and payload['limit'] == limits[key], 'limit dimension or value mismatch')


def check_observation(artifact, raw_input, descriptor):
    require(type(artifact) is dict and type(artifact.get('format')) is str,
            'artifact format string required')
    if artifact['format'] != descriptor['format']:
        unavailable('unsupported producer format')
    grammar(artifact, ARTIFACT)
    require(artifact['scope'] == descriptor['scope'] and artifact['build_receipt'] == descriptor['build_receipt'],
            'exact producer projection strings differ')
    require(artifact['schemas'] == descriptor['schemas'], 'schema member order or identity differs')
    limits, o = artifact['configured_limits'], artifact['observation']
    for key, value in descriptor['example_fixed_limits'].items():
        require(limits[key] == value, 'example fixed limit differs')
    require(o['input_bytes'] == len(raw_input), 'observation input length mismatch')
    if o['input_sha256'] is not None:
        require(o['input_sha256'] == identify(raw_input)['sha256'], 'observation input digest mismatch')
    checks = SourceChecks(raw_input, o, descriptor['schemas'])
    for index, event in enumerate(o['events']):
        require(event['ordinal'] == index, 'combined event ordinal mismatch')
        checks.diagnostic(event)
    for field in ('finalization_error', 'operational_failure'):
        if o[field] is not None:
            checks.diagnostic(o[field], terminal=True)
    failure_relations(o['failure'], limits)
    failure_relations(o['terminal_capture_failure'], limits)
    incomplete = o['assessment'] == 'Incomplete'
    require(artifact['epoch_remaining_requests'] == 1 and artifact['epoch_retired'] == incomplete,
            'example epoch state mismatch')
    require((o['failure'] is not None) == incomplete, 'assessment and failure disagree')
    require(len(o['events']) <= o['observed_callbacks'], 'retained events exceed callbacks')
    require(o['observed_callbacks'] <= len(o['events']) + 1, 'callbacks continue after capture refusal')
    require(len(o['events']) <= limits['diagnostic_count'], 'retained events exceed allowance')
    require(o['retained_diagnostic_bytes'] <= limits['diagnostic_bytes'], 'retained byte allowance exceeded')
    require(bool(o['retained_diagnostic_bytes']) == bool(o['events']), 'retained byte/event presence mismatch')
    require(o['work'] <= limits['work'], 'work exceeds allowance')
    require(o['retained_diagnostic_bytes'] <= o['work'], 'retained bytes exceed charged work')
    invalid_limits = any(v == 0 or v > ISIZE_MAX for v in limits.values())
    if invalid_limits:
        require(o['failure'] == 'InvalidLimits' and o['work'] == 0 and o['input_sha256'] is None and
                o['validated_root_reference'] is None, 'invalid limits admission state mismatch')
    else:
        require(o['failure'] != 'InvalidLimits', 'InvalidLimits with valid configured limits')
    if o['input_sha256'] is not None:
        require(o['work'] >= len(raw_input), 'hash emitted before input work charge')
    # Before the first input-byte work charge succeeds no input hash is emitted.
    if o['input_sha256'] is None:
        require(o['work'] == 0 and o['validated_root_reference'] is None, 'prehash admission state mismatch')
    if o['input_sha256'] is None or o['validated_root_reference'] is None:
        require(incomplete and not o['events'] and o['observed_callbacks'] == 0 and
                o['root_validity'] is None and not o['finalization_attempted'] and
                o['operational_failure'] is None and o['terminal_capture_failure'] is None,
                'admission stop has runtime evidence')
    if o['root_validity'] is not None or o['observed_callbacks']:
        require(o['input_sha256'] is not None and o['validated_root_reference'] is not None,
                'runtime evidence without admitted bindings')
    if not incomplete:
        require(o['input_sha256'] is not None and o['validated_root_reference'] is not None and
                o['root_validity'] is not None and o['finalization_attempted'] and o['finalization_succeeded'] and
                all(o[k] is None for k in ('finalization_error', 'operational_failure', 'terminal_capture_failure')),
                'completed assessment lacks healthy terminal state')
        require(o['observed_callbacks'] == len(o['events']), 'complete capture has refused callbacks')
        errors = sum(e['severity'] == 'Error' for e in o['events'])
        if o['assessment'] == 'Valid':
            require(o['root_validity'] == 'Valid' and errors == 0, 'Valid assessment contradiction')
        else:
            require(o['root_validity'] == 'Invalid' or (o['root_validity'] == 'Valid' and errors > 0),
                    'Invalid assessment contradiction')
    if o['finalization_attempted']:
        require(o['root_validity'] is not None and o['input_sha256'] is not None and
                o['validated_root_reference'] is not None, 'finalization without completed walk')
    else:
        require(not o['finalization_succeeded'] and o['finalization_error'] is None,
                'unattempted finalization has result')
    if o['finalization_succeeded']:
        require(all(o[k] is None for k in ('finalization_error', 'operational_failure', 'terminal_capture_failure')),
                'successful finalization has terminal failure')
    elif o['finalization_attempted']:
        require(o['finalization_error'] is not None or o['terminal_capture_failure'] is not None,
                'failed finalization lacks retained error or capture refusal')
    if not o['finalization_attempted'] and (o['root_validity'] is not None or o['observed_callbacks']):
        require(o['operational_failure'] is not None or o['terminal_capture_failure'] is not None,
                'stopped runtime lacks abort evidence or capture refusal')
    if any(o[k] is not None for k in ('operational_failure', 'finalization_error', 'terminal_capture_failure')):
        require(incomplete, 'terminal failure with complete assessment')
    if o['failure'] is not None:
        tag, payload = variant(o['failure'])
        require(tag != 'EpochRetired', 'outer EpochRetired cannot be an Observation')
        if tag == 'Consistency':
            require(o['finalization_succeeded'] and o['root_validity'] != 'Invalid' and
                    (o['root_validity'] != 'Valid' or any(e['severity'] == 'Error' for e in o['events'])),
                    'Consistency without possible post-finalization assessment conflict')
        if tag == 'Limit' and payload['dimension'] == 'diagnostic_count':
            require(len(o['events']) == limits['diagnostic_count'] and
                    o['observed_callbacks'] == len(o['events']) + 1,
                    'diagnostic count failure without saturated count and refused callback')
        if tag == 'Backend':
            if payload['phase'] == 'finalization':
                require(o['finalization_attempted'] and not o['finalization_succeeded'],
                        'backend finalization phase contradiction')
            else:
                require(o['operational_failure'] is not None or o['terminal_capture_failure'] is not None,
                        'backend operational failure lacks terminal evidence')
    if o['terminal_capture_failure'] is not None:
        tag, payload = variant(o['terminal_capture_failure'])
        require(tag in ('ArithmeticOverflow', 'SourceJoin', 'UnsupportedEvidence') or
                (tag == 'Limit' and payload['dimension'] in ('terminal_bytes', 'path_temporary_bytes')),
                'failure has no terminal capture origin')
    return o


def check_status(status, stage, stdout, stderr):
    grammar(status, STATUS)
    require(status['stage'] == stage, 'status stage mismatch')
    require(status['timeout_seconds'] == 300 and status['per_stream_limit'] == 32 * 1024 * 1024,
            'exact harness supervision profile differs')
    require(status['returncode'] is not None or status['launch_error'] is not None,
            'missing child returncode without recorded launch failure')
    for stream, raw, name in (('stdout', stdout, stage + '.json'), ('stderr', stderr, stage + '.stderr')):
        record = status[stream]
        require(record == {'path': name, **identify(raw)}, 'raw stream/status binding mismatch')
        require(len(raw) <= status['per_stream_limit'], 'status retained stream exceeds cap')
    return (status['returncode'] == 0 and status['launch_error'] is None and not status['timed_out'] and
            status['truncated'] == [False, False] and status['read_errors'] == [None, None] and
            status['pipe_drain_complete'])


def check_command(command, artifact, stage, complete_work):
    require(len(command) == (4 if stage == 'complete' else 5), 'example command arity mismatch')
    require(command[2].replace('\\', '/').endswith('/input.xml'), 'command input snapshot role mismatch')
    require(command[1].replace('\\', '/').endswith('/original-resources'), 'command resource role mismatch')
    require(command[3] == str(artifact['configured_limits']['diagnostic_count']), 'command count mismatch')
    if stage == 'complete':
        require(artifact['configured_limits']['work'] == 100_000_000, 'complete command work default mismatch')
    else:
        require(command[4] == str(artifact['configured_limits']['work']), 'command work mismatch')
        if complete_work is not None:
            require(command[4] == str(max(1, complete_work - 1)), 'paired work calibration mismatch')


def check_receipt(receipt, files, descriptor, statuses, artifacts):
    grammar(receipt, RECEIPT)
    if receipt['git_head'] not in descriptor['supported_receipt_revisions']:
        unavailable('unreviewed receipt producer revision')
    require(receipt['kind'] == 'schema-only-development-reproduction; no execution attestation' and
            receipt['input_snapshot'] == 'input.xml', 'receipt kind or snapshot differs')
    require(receipt['source_unchanged_during_build_and_run'], 'receipt records source change')
    require(receipt['input'] == identify(files.held['input.xml']) and
            receipt['source_inventory'] == identify(files.held['source-inventory.json']),
            'receipt input or source inventory binding mismatch')
    for stage in ('complete', 'partial'):
        require(stage in artifacts and statuses[stage]['healthy'], 'success receipt lacks healthy paired capture')
        require(receipt[stage + '_output'] == identify(files.held[stage + '.json']) and
                receipt[stage + '_command'] == statuses[stage]['data']['command'],
                'receipt output or command binding mismatch')
        require(artifacts[stage]['observation']['input_sha256'] is not None,
                'successful reproduction receipt has unbound Observation input')
    require(receipt['partial_command'][:-1] == receipt['complete_command'], 'receipt commands do not share input')
    require(receipt['executable']['path'] == receipt['complete_command'][0], 'receipt executable path mismatch')
    require(receipt['loaded_schema_bytes'] == {s['member']: {'bytes': s['bytes'], 'sha256': s['sha256']}
                                              for s in descriptor['schemas']}, 'receipt schema identities differ')
    require(receipt['archive'] == {'bytes': 499607, 'sha256':
            '2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8'}, 'archive reference differs')
    require(receipt['backend_consumed_source_identity'] == descriptor['backend_consumed_source_identity'],
            'receipt consumed source identity mismatch')
    expected_features = ['default', 'unicode-normalization']
    require(sorted(receipt['backend_features']) == expected_features and
            sorted(receipt['backend_build_artifact']['features']) == expected_features,
            'receipt backend feature mismatch')
    require(receipt['build_command'] == ['cargo', '+1.83.0', 'build', '--offline', '--locked', '--all-features',
                                        '-p', 'einvoice-schema', '--example', 'cii-observe', '--message-format=json'],
            'receipt build command differs')
    require(receipt['backend_build_artifact']['reason'] == 'compiler-artifact' and
            receipt['backend_build_artifact']['target']['name'] == 'xsd_schema', 'backend build record differs')
    require(artifacts['complete']['observation']['assessment'] in ('Valid', 'Invalid') and
            artifacts['partial']['observation']['assessment'] == 'Incomplete' and
            artifacts['partial']['observation']['failure'] is not None, 'success receipt pair terminal mismatch')


def report_template(selection):
    return {
        'consumer': 'exact-producer schema capture verifier / 1',
        'selected_observation': selection, 'format_support': 'not_checked',
        'producer_source_compatibility': 'not_checked', 'exact_byte_bindings': 'not_checked',
        'internal_consistency': 'not_checked', 'producer_reported_assessment': None,
        'capture_completeness': 'unavailable', 'selected_input_binding': 'unavailable', 'attempt_failure': None,
        'attempt_status': 'not_checked', 'selected_terminal': None, 'byte_identities': {},
        'source_witness_verification': 'UNAVAILABLE',
        'source_consistency_checks': ['strict decoding and BOM count', 'declared encoding label when present',
                                     'coordinate units, ordering, bounds and UTF8 boundaries',
                                     'root/owner/subject/value containment',
                                     'reference roles and repeated-reference agreement',
                                     'schema URI membership and declared size bounds; schema bytes unavailable'],
        'unavailable': {key: 'UNAVAILABLE' for key in (
            'execution_authenticity', 'custody', 'loaded_executable_identity', 'build_execution',
            'excluded_build_and_schema_bytes', 'compiler_correspondence', 'source_lexical_tree_path_preorder_truth',
            'schema_component_semantic_truth', 'diagnostic_cause_truth', 'event_phase_timing',
            'structural_equivalence', 'context_coverage', 'site_coverage', 'full_positive_variant_coverage',
            'hard_resources', 'benchmark_eligibility', 'native_acceptance')},
        'release_gate_eligible': False,
    }


def verify_attempt(directory, selection='complete'):
    require(selection in ('complete', 'partial'), 'unknown observation selection')
    report = report_template(selection)
    try:
        descriptor = pinned_descriptor()
        with Files(directory) as files:
            raw_input = files.read('input.xml', INPUT_MAX)
            inventory_raw = files.read('source-inventory.json')
            receipt_raw = files.read('receipt.json', optional=True)
            failure_raw = files.read('failure.json', optional=True)
            failure = strict_json(failure_raw) if failure_raw is not None else None
            if failure_raw is not None:
                grammar(failure, FAILURE_FILE)
                require(failure['kind'] == 'development reproduction failure; not an Observation',
                        'failure file is not a reproduction failure')
                # Data is retained exactly in the fixture, not echoed unboundedly.
                report['attempt_failure'] = {'kind': failure['kind'], 'error_type': failure['error_type'],
                                             'message_sha256': identify(failure['message'].encode())['sha256']}
            receipt = strict_json(receipt_raw) if receipt_raw is not None else None
            if receipt_raw is not None:
                grammar(receipt, RECEIPT)
                require(receipt['input'] == identify(raw_input) and
                        receipt['source_inventory'] == identify(inventory_raw), 'receipt input/source binding mismatch')
            inventory = strict_json(inventory_raw)
            grammar(inventory, ('map', BINDING))
            if inventory != descriptor['source_members']:
                unavailable('source map is not the reviewed exact 407-member producer')
            report['producer_source_compatibility'] = 'reviewed_407_member_map_matches; execution_unavailable'
            statuses, artifacts = {}, {}
            for stage in ('complete', 'partial'):
                stdout = files.read(stage + '.json', optional=True)
                stderr = files.read(stage + '.stderr', optional=True)
                status_raw = files.read(stage + '.status.json', optional=True)
                if stdout is None and stderr is None and status_raw is None:
                    continue
                require(stdout is not None and stderr is not None and status_raw is not None,
                        'incomplete retained stream/status group')
                status = strict_json(status_raw)
                healthy = check_status(status, stage, stdout, stderr)
                statuses[stage] = {'data': status, 'healthy': healthy}
                if healthy:
                    require(stdout, 'healthy child has no Observation')
                    artifact = strict_json(stdout)
                    check_observation(artifact, raw_input, descriptor)
                    artifacts[stage] = artifact
                # Unhealthy raw stdout is a prefix, not necessarily an Observation.
                # Do not parse a truncated prefix or manufacture a validation verdict.
            report['byte_identities'] = {name: identify(raw) for name, raw in files.held.items() if raw is not None}
            report['format_support'] = 'supported_exact_projection' if artifacts else 'no_Observation'
            if 'partial' in artifacts:
                require('complete' in statuses, 'partial without complete attempt')
            complete_work = artifacts.get('complete', {}).get('observation', {}).get('work')
            for stage, artifact in artifacts.items():
                check_command(statuses[stage]['data']['command'], artifact, stage, complete_work)
            if 'complete' in statuses and 'partial' in statuses:
                require(statuses['partial']['data']['command'][:-1] == statuses['complete']['data']['command'],
                        'paired status commands do not share admitted input')
            if 'complete' in artifacts and 'partial' in artifacts:
                left, right = (artifacts[stage]['observation'] for stage in ('complete', 'partial'))
                for field in ('validated_root', 'encoding', 'bom_bytes'):
                    require(left[field] == right[field], 'paired immutable source fact differs: ' + field)
                if left['validated_root_reference'] is not None and right['validated_root_reference'] is not None:
                    require(left['validated_root_reference'] == right['validated_root_reference'],
                            'paired root references differ')
            require(receipt is None or failure is None, 'success receipt coexists with recorded attempt failure')
            if receipt is not None:
                check_receipt(receipt, files, descriptor, statuses, artifacts)
                report['attempt_status'] = 'receipted_pair_consistent'
                report['producer_source_compatibility'] = 'reviewed_revision_and_407_member_map_match; execution_unavailable'
            else:
                report['attempt_status'] = 'retained_failure' if failure is not None else 'required_receipt_unavailable'
            if not all(s['healthy'] for s in statuses.values()):
                require(receipt is None, 'success receipt has failed child status')
                report['attempt_status'] = 'retained_child_failure'
            report['exact_byte_bindings'] = 'supplied_relations_consistent'
            report['internal_consistency'] = 'checked_available_capture_and_attempt_relations'
            if selection in artifacts:
                observation = artifacts[selection]['observation']
                report['producer_reported_assessment'] = observation['assessment']
                report['selected_input_binding'] = ('original_bytes_match' if observation['input_sha256'] is not None
                                                    else 'unavailable_admission_hash')
                report['capture_completeness'] = 'Incomplete' if observation['assessment'] == 'Incomplete' else 'Complete'
                report['selected_terminal'] = {key: observation[key] for key in (
                    'root_validity', 'failure', 'finalization_attempted', 'finalization_succeeded',
                    'terminal_capture_failure', 'work', 'retained_diagnostic_bytes', 'observed_callbacks')}
            if receipt is None or failure is not None or not statuses or not all(s['healthy'] for s in statuses.values()):
                unavailable('retained failed attempt or required successful-pair receipt unavailable')
            if selection not in artifacts:
                unavailable('selected Observation unavailable')
            for artifact in artifacts.values():
                observation = artifact['observation']
                if observation['input_sha256'] is None or observation['validated_root_reference'] is None:
                    unavailable('required admission binding unavailable')
                if observation['failure'] is not None and variant(observation['failure'])[0] == 'UnsupportedCapability':
                    unavailable('producer required capability unavailable')
            code = 2 if report['capture_completeness'] == 'Incomplete' else 0
            report['result'] = 'consistent_incomplete_capture' if code == 2 else 'consistent_complete_capture'
            return code, report
    except Refusal as error:
        if error.code == 1:
            report['exact_byte_bindings'] = 'not_established'
            report['internal_consistency'] = 'rejected'
        report['result'] = error.category
        report['reason'] = str(error)[:240]
        return error.code, report
    except OSError as error:
        unsafe = error.errno in (errno.ELOOP, errno.ENOTDIR, errno.ENXIO)
        report['result'] = 'unsafe_file_path' if unsafe else 'consumer_io_failure'
        report['reason'] = 'nonregular or symlink path refused' if unsafe else 'required evidence could not be read'
        return (1 if unsafe else 2), report
    except (MemoryError, RecursionError):
        report['result'] = 'consumer_resource_failure'
        report['reason'] = 'consumer memory or recursion resource failure'
        return 2, report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--attempt', required=True)
    parser.add_argument('--observation', choices=('complete', 'partial'), default='complete')
    args = parser.parse_args()
    code, report = verify_attempt(args.attempt, args.observation)
    try:
        output = json.dumps(report, ensure_ascii=True, sort_keys=True, separators=(',', ':'))
    except (MemoryError, RecursionError):
        print('{"result":"consumer_report_resource_failure","release_gate_eligible":false}')
        return 2
    if len(output) > 16 * 1024:
        output = '{"result":"consumer_report_limit","release_gate_eligible":false}'
        code = 2
    print(output)
    return code


if __name__ == '__main__':
    sys.exit(main())
