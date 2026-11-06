#!/usr/bin/env python3
"""Extract selected, authored primitives from the recorded discovery evidence.

Usage: python3 extract_primitives.py CASES_JSON SAXON_RESULTS_JSON OUTPUT_TSV
The source checksums are pinned in tests/fixtures/README.md. No Java is invoked,
no corpus/labels are read, and no expected values are calculated by the kernel.
"""
import hashlib
import json
from pathlib import Path
import sys

SOURCE_SHA256 = (
    '0c9485de37ba60db43a9ca7a594c95ddfdd0dd81b426e7d0c12b71947c67bd67',
    'e621b68975b00208ff3837ddbf4fa79ac81f1c681ef06794d47c49882e2a5e9a',
)


def normalized_scale(text):
    text = text.strip(' \t\r\n').lstrip('+-')
    digits = text.replace('.', '')
    if not digits.strip('0'):
        return 0
    fraction = len(text.split('.')[1]) if '.' in text else 0
    return fraction - (len(digits) - len(digits.rstrip('0')))


def main():
    cases_path, reference_path, output_path = map(Path, sys.argv[1:])
    for path, expected in zip((cases_path, reference_path), SOURCE_SHA256):
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f'{path.name}: unexpected evidence SHA-256 {actual}')
        print(path.name, actual)
    cases = json.loads(cases_path.read_text())
    reference = {r['id']: r for r in json.loads(reference_path.read_text())}
    lines = ['# id\top\ta_hex\tb_hex\tscale\tmode\tstatus\tvalue_or_code_hex\texpression_hex']
    for case in cases:
        if 'op' not in case:
            continue
        op, args = case['op'], case['args']
        a, b = args[0], args[1] if len(args) > 1 else ''
        scale, mode = 0, 'TowardZero'
        if op == 'div':
            scale = max(18, normalized_scale(a) - normalized_scale(b) + 18)
            mode = 'HalfTowardZero'
        elif op in ('round', 'even'):
            scale = int(b)
            mode = 'HalfEven' if op == 'even' else 'HalfTowardPositiveInfinity'
            b = ''
        result = reference[case['id']]
        value = result['value'] if result['status'] == 'OK' else result['type_or_code'].split('}')[-1]
        fields = [case['id'], op, a.encode().hex(), b.encode().hex(), str(scale), mode,
                  result['status'], value.encode().hex(), case['expr'].encode().hex()]
        lines.append('\t'.join(fields))
    output_path.write_text('\n'.join(lines) + '\n')


if __name__ == '__main__':
    main()
