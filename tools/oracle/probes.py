#!/usr/bin/env python3
"""Re-run the discovery XPath counterexamples against locked Xee and pinned Saxon."""
import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import oracle


def main():
    oracle.prepare()
    cases = oracle.read_json(oracle.ROOT/'probes/xpath/cases.json')
    listing = oracle.CACHE/'xpath.tsv'
    listing.write_text(''.join(c['id']+'\t'+base64.b64encode(c['expr'].encode()).decode()+'\n' for c in cases))
    cp = os.pathsep.join([str(oracle.CACHE/'classes'), str(oracle.CACHE/'downloads/validator.jar')])
    saxon = subprocess.run(['java','-cp',cp,'XPathProbe',str(listing)],check=True,capture_output=True,text=True)
    xee = subprocess.run(['cargo','run','--locked','--offline','--quiet','--manifest-path',
        str(oracle.ROOT/'probes/xpath/Cargo.toml')],check=True,capture_output=True,text=True)
    reference = {v['id']: v for v in map(json.loads,saxon.stdout.splitlines())}
    candidate = {v['id']: v for v in map(json.loads,xee.stdout.splitlines())}
    if set(reference) != {c['id'] for c in cases} or set(candidate)!=set(reference):
        raise ValueError('Missing probe outcomes')
    results=[]
    for case in cases:
        a,b = reference[case['id']],candidate[case['id']]
        match = a['error']==b['error'] and (a['error'] or a['value']==b['value'])
        results.append(dict(id=case['id'],expression=case['expr'],saxon=a,xee=b,match=match))
    divergent={r['id'] for r in results if not r['match']}
    oracle.write_json(oracle.ROOT/'results/xpath.json',dict(format=1,
        saxon='HE 12.9 (KoSIT 1.6.3)',xee='xee-xpath 0.1.5; xee-interpreter 0.2.0 (Cargo.lock)',
        results=results,conformant=not divergent))
    if divergent != {'decimal_30_digits','decimal_29_fraction_digits'}:
        raise ValueError(f'Probe behavior changed; investigate rather than bless: {divergent}')
    print('16 XPath probes reproduced; two known decimal divergences. Xee is NOT approved unchanged.')


if __name__=='__main__':
    main()
