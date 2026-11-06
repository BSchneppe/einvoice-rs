#!/usr/bin/env python3
"""Prelabel DEVELOPMENT proposal only; byte recipes and synthetic syntax QA.

No schema execution, XPath evaluation, oracle import/process, or network access.
Run with -B. All writes remain below this file's directory, in a fresh v1/.
"""
import base64
import hashlib
import io
import json
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET
import zipfile

HERE = Path(__file__).resolve().parent
SEED = HERE.parent / 'dev-seed-smoke-01/development-seed.xml'
SEED_SHA = '881f7bc3ff0513a6627a981301af2c0ec9bcdce2c9d1628d7858e581f4d8e91c'
CACHE = Path('/Users/bene/.t3/worktrees/rust-einvoice/benediktschneppe-bsc-5--45552c76f3d0fedb/tools/oracle/.cache/downloads')
RAM = 'urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100'
NS = {'ram': RAM, 'rsm': 'urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100'}
ARCHIVES = {
    'cen.zip': ('1cd53cb8a84d38aedc82c0caede217da983a7934dd663f793a092fd66443c561',
                'https://github.com/ConnectingEurope/eInvoicing-EN16931/releases/download/validation-1.3.16/en16931-cii-1.3.16.zip'),
    'xr.zip': ('ca5e07afd04e72cd283d581590ffff3a15f4aac9d2f40c993345d78e67ea22b4',
               'https://projekte.kosit.org/api/v4/projects/356/packages/maven/de/xeinkauf/xrechnung-3.0.2-schematron/2.6.0/xrechnung-3.0.2-schematron-2.6.0.zip'),
    'config.zip': ('2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8',
                   'https://github.com/itplr-kosit/validator-configuration-xrechnung/releases/download/v2026-08-31/xrechnung-3.0.2-validator-configuration-2026-08-31.zip'),
}
# Only these exact original-source members are ever opened. No archive listings.
MEMBERS = {
    'cen-model': ('cen.zip', 'schematron/CII/EN16931-CII-model.sch'),
    'cen-syntax': ('cen.zip', 'schematron/CII/EN16931-CII-syntax.sch'),
    'cen-codes': ('cen.zip', 'schematron/codelist/EN16931-CII-codes.sch'),
    'xr-cii': ('xr.zip', 'schematron/cii/XRechnung-CII-validation.sch'),
    'cii-xsd': ('config.zip', 'resources/cii/16b/xsd/CrossIndustryInvoice_100pD16B.xsd'),
    'ram-xsd': ('config.zip', 'resources/cii/16b/xsd/CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd'),
    'udt-xsd': ('config.zip', 'resources/cii/16b/xsd/CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd'),
}
# Every citation is a source location, never a result or copied expectation.
CITES = {
    'namespace': ('cii-xsd', 15, 24, 'qualified namespace and root sequence'),
    'empty': ('xr-cii', 296, 299, 'PEPPOL-EN16931-R008 lexical name() context'),
    'buyer-reference': ('xr-cii', 336, 338, 'BR-DE-15'),
    'buyer-cardinality': ('ram-xsd', 311, 316, 'HeaderTradeAgreementType/BuyerReference default maxOccurs=1'),
    'date-cardinality': ('ram-xsd', 229, 246, 'ExchangedDocumentType/IssueDateTime'),
    'careof-forbidden': ('cen-syntax', 806, 810, 'CII-DT-093'),
    'careof-position': ('ram-xsd', 925, 948, 'TradeAddressType/CareOf after CountryID'),
    'lines-xsd': ('ram-xsd', 901, 908, 'SupplyChainTradeTransactionType'),
    'lines-required': ('cen-model', 24, 24, 'BR-16'),
    'product-required': ('cen-model', 29, 35, 'BR-21/BR-25/BR-27'),
    'product-xsd': ('ram-xsd', 1115, 1124, 'TradeProductType/Name'),
    'terms-count': ('cen-syntax', 645, 646, 'CII-SR-452/453'),
    'terms-position': ('ram-xsd', 384, 391, 'HeaderTradeSettlementType/SpecifiedTradePaymentTerms'),
    'line-tax-count': ('cen-syntax', 313, 314, 'CII-SR-454'),
    'line-tax-position': ('ram-xsd', 476, 488, 'LineTradeSettlementType/ApplicableTradeTax'),
    'amount-xsd': ('udt-xsd', 16, 23, 'AmountType extends xsd:decimal'),
    'line-precision': ('cen-model', 253, 253, 'BR-DEC-23'),
    'line-sum-round': ('cen-model', 76, 76, 'BR-CO-10'),
    'taxbasis-round': ('cen-model', 79, 83, 'BR-CO-13/14'),
    'grandtotal-round': ('cen-model', 92, 119, 'BR-CO-15/16'),
    'price-sign': ('cen-model', 35, 35, 'BR-27'),
    'line-arithmetic': ('xr-cii', 239, 257, 'PEPPOL-EN16931-R120 and operand lets'),
    'slack': ('xr-cii', 32, 33, 'slackValue, not an acceptance policy'),
    'date-shape': ('cen-syntax', 811, 813, 'CII-DT-097'),
    'date-required': ('cen-model', 11, 11, 'BR-03 format=102 selector'),
    'date-string': ('udt-xsd', 51, 64, 'DateTimeType/DateTimeString xsd:string'),
    'currency-code': ('cen-codes', 22, 26, 'BR-CL-04'),
    'unit-code': ('cen-codes', 148, 152, 'BR-CL-23'),
    'text-attributes': ('udt-xsd', 145, 152, 'TextType attributes xsd:token; later schema whitespace processing may erase XML-layer differences'),
    'note-content': ('ram-xsd', 566, 574, 'NoteType/Content udt:TextType'),
}
W3C = {
    'xml10-eol': {'url': 'https://www.w3.org/TR/2008/REC-xml-20081126/#sec-line-ends', 'section': '2.11', 'read': '2026-11-06', 'basis': 'Literal CRLF and CR are normalized to LF before parsing. NEL and LS are outside this list.'},
    'xml10-attr': {'url': 'https://www.w3.org/TR/2008/REC-xml-20081126/#AVNormalize', 'section': '3.3.3', 'read': '2026-11-06', 'basis': 'Literal whitespace becomes space after line-end processing; a character reference contributes its referenced character. No DTD is introduced.'},
    'xml10-ref': {'url': 'https://www.w3.org/TR/2008/REC-xml-20081126/#sec-references', 'section': '4.1', 'read': '2026-11-06', 'basis': 'Numeric character references supply the specified characters.'},
    'xml10-encoding': {'url': 'https://www.w3.org/TR/2008/REC-xml-20081126/#charencoding', 'section': '4.3.3', 'read': '2026-11-06', 'basis': 'UTF-8 may have a BOM; UTF-16 requires one. Encoding declarations and actual bytes must agree.'},
    'xml10-comments': {'url': 'https://www.w3.org/TR/2008/REC-xml-20081126/#sec-comments', 'section': '2.5', 'read': '2026-11-06', 'basis': 'A fixed comment changes source positions without adding element occurrences.'},
    'xml11-eol': {'url': 'https://www.w3.org/TR/2006/REC-xml11-20060816/#sec-line-ends', 'section': '2.11', 'read': '2026-11-06', 'basis': 'Literal NEL and LS join the line-end normalization set; references are a separate source form.'},
    'xml11-version': {'url': 'https://www.w3.org/TR/2006/REC-xml11-20060816/#sec-entity-version', 'section': '4.3.4', 'read': '2026-11-06', 'basis': 'The document entity declaration selects the document version. Backend capability remains unobserved.'},
    'names': {'url': 'https://www.w3.org/TR/2009/REC-xml-names-20091208/#scoping', 'section': '6.1/6.2', 'read': '2026-11-06', 'basis': 'Prefix scope and defaults determine expanded element names. Unprefixed attributes have no namespace.'},
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def b64(data):
    return base64.b64encode(data).decode('ascii')


def jsonbytes(obj):
    return (json.dumps(obj, ensure_ascii=True, sort_keys=True, indent=2) + '\n').encode()


def read_sources():
    snapshots = {}
    for name, (pin, _) in ARCHIVES.items():
        data = (CACHE / name).read_bytes()
        if sha(data) != pin:
            raise ValueError('selected_archive_integrity')
        snapshots[name] = data
    members = {}
    for key, (archive, member) in MEMBERS.items():
        with zipfile.ZipFile(io.BytesIO(snapshots[archive])) as z:
            data = z.read(member)
        members[key] = {'archive': archive, 'archive_sha256': ARCHIVES[archive][0],
                        'member': member, 'sha256': sha(data), 'bytes': len(data),
                        '_lines': data.decode('utf-8').splitlines(keepends=True)}
    citations = {}
    for key, (member, lo, hi, locator) in CITES.items():
        lines = members[member]['_lines']
        assert 1 <= lo <= hi <= len(lines)
        citations[key] = {'member_ref': member, 'line_start_1based': lo,
                          'line_end_1based': hi, 'locator': locator,
                          'utf8_excerpt_sha256': sha(''.join(lines[lo-1:hi]).encode())}
    for record in members.values():
        record.pop('_lines')
    return {'archives': {k: {'sha256': v[0], 'url': v[1]} for k, v in ARCHIVES.items()},
            'members': members, 'citations': citations, 'normative_reads': W3C,
            'source_use': 'static intent only; no assertion execution or predicted full verdict'}


def positions(data, needle):
    return [m.start() for m in re.finditer(re.escape(needle), data)]


class Proposal:
    def __init__(self, seed):
        self.seed = seed
        self.cases = []

    def edit(self, start, stop, new, location):
        if isinstance(new, str):
            new = new.encode('utf-8')
        old = self.seed[start:stop]
        # Independent checkable unique surrounding seed anchor for each splice.
        width = 24
        while True:
            left, right = max(0, start-width), min(len(self.seed), stop+width)
            anchor = self.seed[left:right]
            if self.seed.count(anchor) == 1:
                break
            width *= 2
            assert width <= 4 * len(self.seed)
        return {'seed_start_byte_0based': start, 'seed_end_byte_0based': stop,
                'old_base64': b64(old), 'new_base64': b64(new), 'old_sha256': sha(old),
                'unique_seed_anchor_base64': b64(anchor), 'anchor_seed_start_byte_0based': left,
                'anchor_match_count': 1, 'source_occurrence': location,
                'seed_line_1based': self.seed[:start].count(b'\n') + 1,
                'seed_byte_column_1based': start - self.seed.rfind(b'\n', 0, start)}

    def replace(self, old, new, location, occurrence=1, count=1):
        old = old.encode('utf-8') if isinstance(old, str) else old
        hits = positions(self.seed, old)
        assert len(hits) == count and 1 <= occurrence <= count, (location, len(hits), count)
        start = hits[occurrence-1]
        return self.edit(start, start+len(old), new, location)

    def block(self, tag, occurrence, count):
        pattern = rb'<' + tag.encode() + rb'(?:\s[^>]*)?>.*?</' + tag.encode() + rb'>'
        hits = list(re.finditer(pattern, self.seed, re.S))
        assert len(hits) == count, (tag, len(hits), count)
        hit = hits[occurrence-1]
        return hit.start(), hit.end(), hit.group()

    def block_edit(self, tag, occurrence, count, transform, path):
        start, stop, old = self.block(tag, occurrence, count)
        return self.edit(start, stop, transform(old), path)

    def add(self, family, name, params, edits, cites, positive=(), negative=(),
            encoding='utf-8', bom=False, version='1.0', related=(), role='candidate'):
        edits = sorted(edits, key=lambda e: e['seed_start_byte_0based'])
        assert all(a['seed_end_byte_0based'] <= b['seed_start_byte_0based'] for a,b in zip(edits,edits[1:]))
        recipe = {'format': 'bsc10-seed-byte-splices-v1', 'seed_sha256': SEED_SHA,
                  'splices_against_original_seed': edits,
                  'final_encoding': encoding, 'bom': bom,
                  'encoding_step': 'apply seed byte splices in descending offsets, decode UTF-8 strictly, encode requested encoding, then prepend explicit BOM',
                  'xml_version': version, 'no_xml_reserialization': True}
        data = replay(self.seed, recipe)
        # Repeat from the same ancestor, never by reapplying mutations to a derivative.
        assert replay(self.seed, recipe) == data
        identity = f'{family}-{name}'
        row = {'id': identity, 'revision': 1, 'status': 'proposed_not_frozen',
               'assignment': 'DEVELOPMENT', 'whole_family': family, 'parameter_family': family,
               'parameters': params, 'role': role, 'seed_sha256': SEED_SHA,
               'ancestry': ['public-sinabis-original-development-seed'],
               'intent': {'positive': list(positive), 'negative': list(negative),
                          'is_observation': False},
               'source_citations': cites, 'related_cases_same_family': list(related),
               'recipe': recipe, 'recipe_sha256': sha(jsonbytes(recipe)),
               'input': {'path': f'inputs/{identity}.xml', 'sha256': sha(data), 'bytes': len(data)},
               'original_location_encoding_transformation': {
                   'mapping': 'exact original seed offsets and byte columns in each splice; downstream character/line offsets may shift',
                   'final_encoding': encoding, 'bom': bom, 'xml_version': version,
                   'diagnostic_location_claim': 'none; future target locations retained verbatim with input binding'},
               'expected': {'xml': 'unknown', 'schema': 'unknown', 'business': 'unknown',
                            'scenario': 'unknown', 'acceptance': 'unknown', 'processing': 'unknown'},
               'observation': None, 'oracle_evaluated': False,
               'retention': 'retain every future outcome, collateral finding, duplicate occurrence and processing failure'}
        self.cases.append((row, data))


def replay(seed, recipe):
    assert sha(seed) == recipe['seed_sha256']
    data = seed
    for e in reversed(recipe['splices_against_original_seed']):
        start, stop = e['seed_start_byte_0based'], e['seed_end_byte_0based']
        old = base64.b64decode(e['old_base64'], validate=True)
        anchor = base64.b64decode(e['unique_seed_anchor_base64'], validate=True)
        anchor_start = e['anchor_seed_start_byte_0based']
        assert seed.count(anchor) == 1
        assert seed[anchor_start:anchor_start+len(anchor)] == anchor
        assert seed[start:stop] == old and sha(old) == e['old_sha256']
        assert data[start:stop] == old
        data = data[:start] + base64.b64decode(e['new_base64'], validate=True) + data[stop:]
    text = data.decode('utf-8', errors='strict')
    data = text.encode(recipe['final_encoding'], errors='strict')
    if recipe['bom']:
        data = {'utf-8': b'\xef\xbb\xbf', 'utf-16-le': b'\xff\xfe'}[recipe['final_encoding']] + data
    return data


def make_proposal(seed):
    p = Proposal(seed)
    R = p.replace
    B = p.block_edit
    # F01: lexical prefix transformations and changed expanded-name controls.
    for name, default in [('prefix-b', False), ('default-ram', True)]:
        edits = [R('xmlns:ram=', 'xmlns=' if default else 'xmlns:b=', 'root namespace declaration')]
        for token, replacement in [(b'<ram:', b'<' if default else b'<b:'), (b'</ram:', b'</' if default else b'</b:')]:
            for pos in positions(seed, token):
                edits.append(p.edit(pos, pos+len(token), replacement, 'all original ram element QNames; exact seed lexical occurrences'))
        p.add('F01', name, {'prefix': '' if default else 'b'}, edits, ['namespace','names','empty'],
              positive=['preserve expanded element names while varying lexical QNames; no assumed verdict invariance'])
    seller = '<ram:Name>Rheinaue Kabeltechnik GmbH</ram:Name>'
    p.add('F01','local-alias',{'prefix':'b','target':'seller Name'},
          [R(seller, f'<b:Name xmlns:b="{RAM}">Rheinaue Kabeltechnik GmbH</b:Name>', 'SellerTradeParty/Name[1]')],
          ['namespace','names','empty'], positive=['local prefix alias retains expanded name'])
    p.add('F01','wrong-name-uri',{'uri':'urn:bsc10:neutral:other'},
          [R(seller, '<b:Name xmlns:b="urn:bsc10:neutral:other">Rheinaue Kabeltechnik GmbH</b:Name>', 'SellerTradeParty/Name[1]')],
          ['namespace','names'], negative=['change namespace URI of one schema-qualified Name'])
    p.add('F01','local-shadow',{'prefix':'ram','uri':'urn:bsc10:neutral:other'},
          [R(seller, '<ram:Name xmlns:ram="urn:bsc10:neutral:other">Rheinaue Kabeltechnik GmbH</ram:Name>', 'SellerTradeParty/Name[1]')],
          ['namespace','names'], negative=['shadow ram binding only at seller Name'])

    # F02: presence, schema/business cardinality, and actual sequential contexts.
    buyer = '<ram:BuyerReference>TICKET-283008</ram:BuyerReference>'
    for name, new in [('missing',''), ('empty','<ram:BuyerReference/>'),
                      ('whitespace','<ram:BuyerReference> \t </ram:BuyerReference>'),
                      ('duplicate',buyer+buyer)]:
        p.add('F02','buyer-'+name, {'presence':name}, [R(buyer,new,'ApplicableHeaderTradeAgreement/BuyerReference[1]')],
              ['buyer-reference','buyer-cardinality','empty'], negative=['exercise '+name+' buyer-reference representation'])
    seller_address = p.block('ram:PostalTradeAddress',1,2)
    a,b,old = seller_address
    p.add('F02','careof', {'value':'Neutral Contact'},
          [p.edit(a,b,old.replace(b'</ram:PostalTradeAddress>',b'<ram:CareOf>Neutral Contact</ram:CareOf></ram:PostalTradeAddress>'), 'SellerTradeParty/PostalTradeAddress[1]')],
          ['careof-forbidden','careof-position'], negative=['insert source-forbidden address component at XSD-declared position'])
    p.add('F02','issue-date-duplicate',{'count':2},
          [B('ram:IssueDateTime',1,1,lambda old:old+old,'ExchangedDocument/IssueDateTime[1]')],
          ['date-cardinality'], negative=['cross XSD singleton upper boundary without changing date text'])
    edits = [B('ram:IncludedSupplyChainTradeLineItem',i,3,lambda old:b'',f'SupplyChainTradeTransaction/IncludedSupplyChainTradeLineItem[{i}]') for i in (1,2,3)]
    p.add('F02','zero-lines',{'line_count':0},edits,['lines-xsd','lines-required'],
          negative=['cross business lower line-item boundary; unchanged header totals deliberately retained'])
    p.add('F02','terms-duplicate',{'terms_count':2},
          [B('ram:SpecifiedTradePaymentTerms',1,1,lambda old:old+old,'ApplicableHeaderTradeSettlement/SpecifiedTradePaymentTerms[1]')],
          ['terms-count','terms-position'], negative=['cross business upper boundary for payment terms'])
    # ApplicableTradeTax occurrences 1..3 are lines; 4 is header, checked below.
    for name,n in [('tax-zero',0),('tax-two',2)]:
        p.add('F02',name,{'line':2,'tax_count':n},
              [B('ram:ApplicableTradeTax',2,4,lambda old,n=n:old*n,'IncludedSupplyChainTradeLineItem[2]/SpecifiedLineTradeSettlement/ApplicableTradeTax[1]')],
              ['line-tax-count','line-tax-position'], negative=['cross exact-one line-tax boundary in the second line context'])
    product_names = ['Drohnengestützte Thermografie-Befliegung (PV-Park)',
                     'Sensorkalibrierung &amp; Pyranometer-Prüfung',
                     'Sachverständigen-Abnahmegutachten BESS-Speicher']
    for indices in [(2,), (3,), (2,3)]:
        name = 'name-missing-'+'-'.join(map(str,indices))
        edits = [R(f'<ram:Name>{product_names[i-1]}</ram:Name>','',f'IncludedSupplyChainTradeLineItem[{i}]/SpecifiedTradeProduct/Name[1]') for i in indices]
        p.add('F02',name,{'line_contexts':list(indices)},edits,['product-required','product-xsd'],
              negative=['remove required product name in the declared sequential line context(s)'],
              related=['F02-name-missing-2','F02-name-missing-3','F02-name-missing-2-3','F02-name-missing-2-3-shifted'])
    edits = [R(f'<ram:Name>{product_names[i-1]}</ram:Name>','',f'IncludedSupplyChainTradeLineItem[{i}]/SpecifiedTradeProduct/Name[1]') for i in (2,3)]
    a,_,_ = p.block('ram:IncludedSupplyChainTradeLineItem',2,3)
    edits.append(p.edit(a,a,'<!--BSC10 location control\nneutral\n-->\n','before IncludedSupplyChainTradeLineItem[2]'))
    p.add('F02','name-missing-2-3-shifted',{'line_contexts':[2,3],'inserted_comment_lf_count':3},edits,
          ['product-required','product-xsd','xml10-comments'],
          positive=['retain same two affected contexts while shifting original source locations'],
          negative=['same missing-name mutations as sibling case; one family, no new independent technique'],
          related=['F02-name-missing-2-3'])

    # F03: finite lexical boundaries and exact rational round-sum neighborhoods.
    def line_total(value, line=1):
        old = ['1927.35','728.40','2503.78'][line-1]
        return R(f'<ram:LineTotalAmount>{old}</ram:LineTotalAmount>', f'<ram:LineTotalAmount>{value}</ram:LineTotalAmount>',f'IncludedSupplyChainTradeLineItem[{line}]/SpecifiedLineTradeSettlement/SpecifiedTradeSettlementLineMonetarySummation/LineTotalAmount[1]')
    for name,value in [('plus','+1927.35'),('leading-zero','01927.35'),('trailing-zero','1927.350'),
                       ('exponent','1.92735E3'),('magnitude','123456789012345678901234567890.12')]:
        p.add('F03',name,{'line':1,'line_total_lexical':value},[line_total(value)],['amount-xsd','line-precision','line-sum-round','line-arithmetic'],
              positive=['preserve mathematical line amount'] if name in ('plus','leading-zero','trailing-zero','exponent') else [],
              negative=['probe lexical decimal boundary and all collateral arithmetic/precision findings; no numeric-equivalence verdict assumption'])
    p.add('F03','negative-price',{'price':'-0.01'},
          [R('<ram:ChargeAmount>1927.35</ram:ChargeAmount>','<ram:ChargeAmount>-0.01</ram:ChargeAmount>','IncludedSupplyChainTradeLineItem[1]/SpecifiedLineTradeAgreement/NetPriceProductTradePrice/ChargeAmount[1]')],
          ['amount-xsd','price-sign','line-arithmetic'], negative=['cross nonnegative net-price source boundary; retain original totals'])
    for name,line,header in [('round-below','1927.344','5159.52'),('round-tie','1927.345','5159.53'),
                             ('round-above','1927.346','5159.53'),('round-tie-lower-header','1927.345','5159.52')]:
        edits=[line_total(line)]
        # Prices unchanged: this is the source sum/round constraint, not a rebuilt invoice.
        if header == '5159.52':
            for tag in ['LineTotalAmount','TaxBasisTotalAmount','BasisAmount']:
                edits.append(R(f'<ram:{tag}>5159.53</ram:{tag}>',f'<ram:{tag}>5159.52</ram:{tag}>','header '+tag+'[1]'))
            for tag in ['GrandTotalAmount','DuePayableAmount']:
                edits.append(R(f'<ram:{tag}>6139.84</ram:{tag}>',f'<ram:{tag}>6139.83</ram:{tag}>','header '+tag+'[1]'))
        p.add('F03',name,{'line1_total':line,'header_line_total':header,'other_lines_sum':'3232.18',
                         'unrounded_sum':{'1927.344':'5159.524','1927.345':'5159.525','1927.346':'5159.526'}[line],
                         'tax_total_unchanged':'980.31','prices_unchanged':True},edits,
              ['line-sum-round','line-precision','taxbasis-round','grandtotal-round','line-arithmetic','slack'],
              positive=['probe exact below/tie/above sum neighborhood with predeclared matching-intent header'] if name!='round-tie-lower-header' else [],
              negative=['three fractional digits deliberately retained; collateral checks remain evidence'] +
                       (['opposing lower-header intent at the same tie input'] if name=='round-tie-lower-header' else []),
              related=['F03-round-below','F03-round-tie','F03-round-above','F03-round-tie-lower-header'])
    p.add('F03','precision-lines-2-3',{'lines':[2,3],'values':['728.400','2503.780']},
          [line_total('728.400',2),line_total('2503.780',3)],['amount-xsd','line-precision','line-sum-round'],
          positive=['preserve numeric totals in two sequential line contexts'],negative=['cross source lexical precision boundary in both contexts'])

    # F04: string shape, calendar neighborhood and format selector; no date library oracle.
    old='<udt:DateTimeString format="102">20251206</udt:DateTimeString>'
    for name,value,fmt in [('leap-day','20240229','102'),('nonleap-day','20230229','102'),
                           ('month-13','20251301','102'),('day-zero','20251200','102'),
                           ('hyphenated','2025-12-06','102'),('format-203','20251206','203')]:
        p.add('F04',name,{'issue_date':value,'format':fmt},
              [R(old,f'<udt:DateTimeString format="{fmt}">{value}</udt:DateTimeString>','ExchangedDocument/IssueDateTime/DateTimeString[1]')],
              ['date-shape','date-required','date-string'],
              positive=['calendar boundary probe independent of regex shape; no calendar-library label'] if name=='leap-day' else [],
              negative=['contrast calendar, string-shape and exact format-selector constraints'] if name!='leap-day' else [])

    # F05: pinned code-list lexical boundaries, including two different contexts.
    for name,value in [('currency-case','eur'),('currency-space',' EUR '),('currency-outside','ZZZ')]:
        p.add('F05',name,{'currency':value},[R('<ram:InvoiceCurrencyCode>EUR</ram:InvoiceCurrencyCode>',f'<ram:InvoiceCurrencyCode>{value}</ram:InvoiceCurrencyCode>','ApplicableHeaderTradeSettlement/InvoiceCurrencyCode[1]')],
              ['currency-code'], positive=['explore source normalize-space behavior'] if name=='currency-space' else [],
              negative=['contrast pinned code-list lexical boundary; tax currency attribute remains EUR'])
    p.add('F05','unit-case',{'line':2,'unit':'hur'},
          [R('<ram:BilledQuantity unitCode="HUR">8.00</ram:BilledQuantity>','<ram:BilledQuantity unitCode="hur">8.00</ram:BilledQuantity>','IncludedSupplyChainTradeLineItem[2]/SpecifiedLineTradeDelivery/BilledQuantity[1]/@unitCode')],
          ['unit-code'],negative=['case boundary in second line unit code'])
    edits=[]
    for line,old_unit,qty in [(2,'HUR','8.00'),(3,'C62','1.00')]:
        token=f'<ram:BilledQuantity unitCode="{old_unit}">{qty}</ram:BilledQuantity>'
        edits.append(R(token,f'<ram:BilledQuantity unitCode="ZZZ">{qty}</ram:BilledQuantity>',f'IncludedSupplyChainTradeLineItem[{line}]/SpecifiedLineTradeDelivery/BilledQuantity[1]/@unitCode',occurrence=2 if line==3 else 1,count=2 if line==3 else 1))
    p.add('F05','unit-lines-2-3',{'line_contexts':[2,3],'unit':'ZZZ'},edits,['unit-code'],
          negative=['out-of-list intent in two actual sequential unit-code contexts; retain separate diagnostics'])

    # F06: source forms are explicit bytes, separate from interpreted XML and verdicts.
    _,_,note = p.block('ram:Content',1,4)
    for version,kind,literal,reference in [('1.0','crlf','\r\n','&#xD;&#xA;'),('1.0','cr','\r','&#xD;'),
                                            ('1.0','nel','\u0085','&#x85;'),('1.1','nel','\u0085','&#x85;'),
                                            ('1.0','ls','\u2028','&#x2028;'),('1.1','ls','\u2028','&#x2028;')]:
        for form,separator in [('literal',literal),('reference',reference)]:
            edits=[R(note,f'<ram:Content>A{separator}B</ram:Content>','ExchangedDocument/IncludedNote[1]/Content[1]')]
            if version=='1.1':
                edits.append(R("version='1.0'","version='1.1'",'XML declaration version'))
            name=f'xml{version.replace(".","")}-{kind}-{form}'
            p.add('F06',name,{'version':version,'kind':kind,'source_form':form,'content_escaped':f'A{separator}B'},edits,
                  ['note-content','xml10-eol','xml10-ref'] + (['xml11-eol','xml11-version'] if version=='1.1' else []),
                  positive=['probe normative source normalization with distinct literal/reference bytes'],version=version,
                  related=[f'F06-xml{v}-{kind}-{f}' for v in (['10','11'] if kind in ('nel','ls') else ['10']) for f in ('literal','reference')])
    for kind,literal,reference in [('tab','\t','&#x9;'),('crlf','\r\n','&#xD;&#xA;')]:
        for form,separator in [('literal',literal),('reference',reference)]:
            new=note.replace(b'<ram:Content>',f'<ram:Content languageID="de{separator}DE">'.encode())
            p.add('F06',f'attr-{kind}-{form}',{'attribute':'languageID','source_form':form,'separator_kind':kind,'value_escaped':f'de{separator}DE'},
                  [R(note,new,'ExchangedDocument/IncludedNote[1]/Content[1]/@languageID (new source attribute)')],
                  ['text-attributes','note-content','xml10-attr','xml10-eol','xml10-ref'],
                  positive=['contrast literal whitespace and numeric references before later schema token collapse; no backend-stage assumption'])
    p.add('F06','utf8-bom',{'encoding':'UTF-8','bom_hex':'efbbbf'},[],['xml10-encoding'],
          positive=['encoding signature variation of the same character document'],bom=True,role='representation_control')
    p.add('F06','utf16-bom',{'encoding':'UTF-16','byte_order':'little','bom_hex':'fffe'},
          [R("encoding='UTF-8'","encoding='UTF-16'",'XML declaration encoding')],['xml10-encoding'],
          positive=['consistent UTF-16 declaration and BOM with original character document'],encoding='utf-16-le',bom=True,role='representation_control')
    return p


def qa(seed, row, data):
    check = {'id':row['id'],'input_sha256':sha(data),'recipe_replayed_from_seed_twice':True,
             'synthetic_only':True,'schema_evaluated':False,'business_evaluated':False,
             'oracle_evaluated':False,'counts_are_syntax_observations_not_findings':True}
    if row['recipe']['xml_version']=='1.1':
        check.update({'syntax':'not_checked_xml11_capability_not_assumed','counts':None})
        return check
    try:
        root=ET.fromstring(data)
        lines=root.findall('rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem',NS)
        check['syntax']='stdlib_elementtree_parsed_xml10_synthetic_only'
        check['counts']={'line_items':len(lines),
                         'product_names_by_line':[len(x.findall('ram:SpecifiedTradeProduct/ram:Name',NS)) for x in lines],
                         'line_taxes_by_line':[len(x.findall('ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax',NS)) for x in lines],
                         'line_ids_in_source_order':[x.findtext('ram:AssociatedDocumentLineDocument/ram:LineID',namespaces=NS) for x in lines],
                         'buyer_references':len(root.findall('rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:BuyerReference',NS)),
                         'header_payment_terms':len(root.findall('rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradePaymentTerms',NS))}
    except ET.ParseError as exc:
        check['syntax']='stdlib_synthetic_parse_failure_retained'
        check['error']=str(exc)
    return check


def verify(directory):
    seed=SEED.read_bytes()
    assert sha(seed)==SEED_SHA
    ledger=json.loads((directory/'proposed-ledger.json').read_bytes())
    rows=ledger['candidates']
    checks=json.loads((directory/'synthetic-checks.json').read_bytes())['cases']
    assert len(rows)==len({x['id'] for x in rows})
    for row,check in zip(rows,checks,strict=True):
        data=replay(seed,row['recipe'])
        assert data==(directory/row['input']['path']).read_bytes()
        assert sha(data)==row['input']['sha256']
        assert sha(jsonbytes(row['recipe']))==row['recipe_sha256']
        assert all(value=='unknown' for value in row['expected'].values())
        assert row['observation'] is None and not row['oracle_evaluated']
        assert row['assignment']=='DEVELOPMENT'
        assert qa(seed,row,data)==check
    byid={c['id']:c for c in checks}
    assert byid['F02-name-missing-2']['counts']['product_names_by_line']==[1,0,1]
    assert byid['F02-name-missing-3']['counts']['product_names_by_line']==[1,1,0]
    assert byid['F02-name-missing-2-3']['counts']['product_names_by_line']==[1,0,0]
    assert byid['F02-name-missing-2-3-shifted']['counts']==byid['F02-name-missing-2-3']['counts']
    assert byid['F02-tax-zero']['counts']['line_taxes_by_line']==[1,0,1]
    assert byid['F02-tax-two']['counts']['line_taxes_by_line']==[1,2,1]
    assert byid['F02-zero-lines']['counts']['line_items']==0
    assert byid['F02-terms-duplicate']['counts']['header_payment_terms']==2
    # Identity and encoding controls are retained, excluded from diversity counts.
    hashes=[row['input']['sha256'] for row in rows]
    assert len(hashes)==len(set(hashes)) and SEED_SHA not in hashes
    sources=json.loads((directory/'source-provenance.json').read_bytes())
    assert sources==read_sources()
    for row in rows:
        for cite in row['source_citations']:
            assert cite in sources['citations'] or cite in sources['normative_reads']
    manifest=json.loads((directory/'file-manifest.json').read_bytes())
    for entry in manifest['files']:
        assert sha((directory/entry['path']).read_bytes())==entry['sha256']
    return {'status':'recipe_and_synthetic_checks_passed','candidates':len(rows),
            'family_counts':{f:sum(r['whole_family']==f for r in rows) for f in sorted({r['whole_family'] for r in rows})},
            'all_verdicts':'unknown','oracle_runs':0,'source_archives_read_only':True,
            'xml11_synthetic_parse_skipped':sum(r['recipe']['xml_version']=='1.1' for r in rows)}


def main():
    if sys.argv[1:]==['verify']:
        print(json.dumps(verify(HERE/'v1'),sort_keys=True))
        return
    if sys.argv[1:]:
        raise SystemExit('usage: design_batch.py [verify]')
    seed=SEED.read_bytes()
    assert sha(seed)==SEED_SHA and len(seed)==11214
    sources=read_sources()
    proposal=make_proposal(seed)
    directory=HERE/'v1'
    directory.mkdir(exist_ok=False)
    (directory/'inputs').mkdir()
    rows=[];checks=[]
    for row,data in proposal.cases:
        (directory/row['input']['path']).write_bytes(data)
        rows.append(row);checks.append(qa(seed,row,data))
    ledger={'format':'bsc10-development-family-proposal-v1','status':'proposed_not_frozen',
            'seed':{'path':str(SEED.relative_to(HERE.parent)), 'sha256':SEED_SHA,'bytes':11214},
            'source_provenance_sha256':sha(jsonbytes(sources)),
            'design_script_sha256':sha(Path(__file__).read_bytes()),
            'allocation':'all whole families DEVELOPMENT; no held-out inputs or labels generated',
            'families':{
                'F01':'namespace and lexical prefix variants, including changed-URI controls',
                'F02':'structural presence/cardinality and sequential product-name mutations, including location control',
                'F03':'amount lexical, magnitude, precision, sign and exact sum/round neighborhoods',
                'F04':'issue-date string, calendar neighborhood and format selector',
                'F05':'currency/unit code-list lexical boundaries and sequential unit contexts',
                'F06':'XML source normalization, version and encoding representations'},
            'independence':'technique groups share one public ancestor; no independent source-provenance claim; related and overlapping recipes remain within whole family',
            'identity_control':{'seed_sha256':SEED_SHA,'expected':'unknown','materialized':False,
                                'counts_as_independent_diversity':False},
            'representation_controls_count_as_independent_diversity':False,
            'retention_policy':'future observations append separately; no silent replacement/deletion/reallocation; retain all collateral findings and process failures',
            'evidence_complete':False,'release_gate_eligible':False,
            'native_execution_attested':False,'native_coverage':'unavailable',
            'runtime_coverage':'unavailable','candidates':rows}
    (directory/'source-provenance.json').write_bytes(jsonbytes(sources))
    (directory/'proposed-ledger.json').write_bytes(jsonbytes(ledger))
    (directory/'synthetic-checks.json').write_bytes(jsonbytes({'scope':'synthetic byte recipe and XML1.0 syntax/count checks only; no schema or business validity','cases':checks}))
    (directory/'revisions.jsonl').write_text(''.join(json.dumps({'id':r['id'],'revision':1,'parent_revision':None,'recipe_sha256':r['recipe_sha256'],'input_sha256':r['input']['sha256'],'reason':'initial source-derived prelabel proposal','oracle_exposure':False},sort_keys=True)+'\n' for r in rows))
    files=sorted(x for x in directory.rglob('*') if x.is_file())
    (directory/'file-manifest.json').write_bytes(jsonbytes({'files':[{'path':str(x.relative_to(directory)),'sha256':sha(x.read_bytes()),'bytes':x.stat().st_size} for x in files]}))
    print(json.dumps(verify(directory),sort_keys=True))


if __name__=='__main__':
    main()
