import json, os, xml.etree.ElementTree as ET
from prepare import OUT, ROOT, SEED_HASH, sha, check, write

def main():
    os.umask(0o077)
    check(not (OUT/'ledger-before-label.json').exists(), 'ledger_already_exists')
    seed = (OUT/'snapshots/development-seed.xml').read_bytes()
    check(sha(seed) == SEED_HASH, 'seed_hash')
    # Insert exact bytes before the first referenced document; no XML serialization.
    anchor = b'      <ram:BuyerOrderReferencedDocument>'
    check(seed.count(anchor) == 1, 'anchor_cardinality')
    offset = seed.index(anchor)
    seller = b'      <ram:SellerTaxRepresentativeTradeParty>\n        <ram:ID>DEV-SELLER-REP-41</ram:ID>\n      </ram:SellerTaxRepresentativeTradeParty>\n'
    buyer = b'      <ram:BuyerTaxRepresentativeTradeParty>\n        <ram:Name>Development Buyer Representative 73</ram:Name>\n      </ram:BuyerTaxRepresentativeTradeParty>\n'
    provenance = json.loads((OUT/'provenance.json').read_text())
    ledger = {
      'format':'bsc10-development-discriminant-ledger-v1', 'assignment':'development-only',
      'labels_at_declaration':'unknown; no anticipated outcome is assigned',
      'baseline_role':'ancestral control only; not new independent evidence',
      'predeclaration_order':'XSD inspection, four recipe/input commitment, full original KoSIT labels',
      'neutral_value_origin':'Researcher-selected literals 41 and 73, chosen before labels; no runtime/builder examples consulted.',
      'seed':provenance['seed'], 'schema_inspection_sha256':sha((OUT/'schema-inspection.txt').read_bytes()),
      'source_association':'CII-SR-282 remains unresolved; no source meaning selected as authoritative',
      'schema_reading':{
        'agreement_type':'HeaderTradeAgreementType: sequence buyer representative then seller representative, before BuyerOrderReferencedDocument; both optional singletons',
        'trade_party_type':'TradePartyType: ID (udt:IDType/xsd:token) before Name (udt:TextType/xsd:string), all child declarations minOccurs=0; no required attributes on those types',
        'static_reading_is_validation':False,
      },
      'scripts':{n:sha((OUT/n).read_bytes()) for n in ('prepare.py','declare.py','run.py','DiscriminantDriver.java')},
      'recipes':[], 'revisions':[],
    }
    inputs = OUT/'inputs'; inputs.mkdir()
    for name, insertion in [('baseline',b''),('seller',seller),('buyer',buyer),('combined',buyer+seller)]:
        b = seed[:offset]+insertion+seed[offset:]
        ET.fromstring(b)
        p = inputs/(name+'.xml'); p.write_bytes(b); p.chmod(0o400)
        recipe = {'case':name,'sequence':0,'parent_sha256':SEED_HASH,'operation':'identity' if not insertion else 'single byte insertion',
          'original_byte_offset_zero_based':offset,'removed_bytes':0,'insertion_utf8':insertion.decode(),
          'insertion_sha256':sha(insertion),'inserted_bytes':len(insertion),'input':str(p.relative_to(OUT)),
          'input_sha256':sha(b),'input_bytes':len(b),
          'changed_node_parent':'/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement',
          'expected_oracle_result':None}
        recipe['recipe_sha256'] = sha(json.dumps(recipe,sort_keys=True,separators=(',',':')).encode())
        ledger['recipes'].append(recipe)
    # Only exact, preselected source nodes are inspected; preserve duplicate parameters.
    source_nodes = []
    sch = '{http://purl.oclc.org/dsdl/schematron}'
    xsl = '{http://www.w3.org/1999/XSL/Transform}'
    svrl = '{http://purl.oclc.org/dsdl/svrl}'
    selected = [
      ('source/schematron/preprocessed/EN16931-CII-validation-preprocessed.sch','assert'),
      ('source/schematron/CII/EN16931-CII-syntax.sch','param'),
      ('source/schematron/abstract/EN16931-CII-syntax.sch','assert'),
      ('config/resources/cii/16b/xsl/EN16931-CII-validation.xsl','compiled'),
    ]
    for member, kind in selected:
        data = (OUT/member).read_bytes(); tree = ET.fromstring(data)
        def walk(node, path, ancestors):
            match = (kind == 'param' and node.tag == sch+'param' and node.get('name') == 'CII-SR-282') or (kind == 'assert' and node.tag == sch+'assert' and node.get('id') == 'CII-SR-282') or (kind == 'compiled' and node.tag == svrl+'failed-assert' and node.get('id') == 'CII-SR-282')
            if match:
                source_nodes.append({'member':member,'sha256':sha(data),'node':path,'attributes':node.attrib,
                  'text':''.join(node.itertext()), 'ancestors':[{'tag':a.tag,'attributes':a.attrib} for a in ancestors if a.tag in (sch+'rule',sch+'pattern',xsl+'template',xsl+'if')],
                  'serialized_node_for_reading_only':ET.tostring(node,encoding='unicode')})
            counts = {}
            for child in node:
                counts[child.tag] = counts.get(child.tag,0)+1
                walk(child,path+'/'+child.tag+'['+str(counts[child.tag])+']',ancestors+[node])
        walk(tree,'/'+tree.tag+'[1]',[])
    check(len(source_nodes)==5,'selected_source_node_count')
    write(OUT/'source-nodes.json', source_nodes)
    ledger['source_nodes_sha256'] = sha((OUT/'source-nodes.json').read_bytes())
    write(OUT/'ledger-before-label.json',ledger)
    (OUT/'ledger-before-label.json').chmod(0o400)
    (OUT/'ledger-before-label.sha256').write_text(sha((OUT/'ledger-before-label.json').read_bytes())+'\n')
    print(json.dumps({'ledger_sha256':sha((OUT/'ledger-before-label.json').read_bytes()),'cases':[{'case':r['case'],'bytes':r['input_bytes'],'sha256':r['input_sha256']} for r in ledger['recipes']]}))

if __name__ == '__main__': main()
