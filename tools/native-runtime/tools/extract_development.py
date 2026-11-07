#!/usr/bin/env python3
"""Extract scalar development observations. No native executable or oracle update.
Usage: extract_development.py NUMERIC_DIR TYPED_PROBES_DIR FRESH_RESULTS OUTPUT
The original identity pins are immutable. The additive fresh-results pin is
recorded after the independently authored probe, never calculated by native code.
"""
import hashlib,json,pathlib,sys
n,t,f,o=map(pathlib.Path,sys.argv[1:])
def read(p,pin):
 b=p.read_bytes();assert hashlib.sha256(b).hexdigest()==pin,str(p);return json.loads(b)
nc=read(n/'cases.json','0c9485de37ba60db43a9ca7a594c95ddfdd0dd81b426e7d0c12b71947c67bd67')
nr=read(n/'saxon-results.json','e621b68975b00208ff3837ddbf4fa79ac81f1c681ef06794d47c49882e2a5e9a')
tc=read(t/'cases.json','b3f5f54696ca5c9fbdd94d190124c4cce249eba9bf80d605dbe813af9d189b17')
tr=read(t/'results.json','935699076f18f58fb48bbb05ece3087da4a13c2fc74b1260484b9c299a20f747')
fr=read(f,'4acc360994b75adf3cfb99c351d520792dcb4e01bf58d24e6c94c6005e35f3c9')
rows=[]
hextext=lambda s:s.encode().hex()
def add(family,id,op,at,a,bt='',b='',arg='',source=None,expr=None):
 x=source
 if 'items' in x:
  assert x['count']==1
  q=x['items'][0]['type'];value=x['items'][0]['value']
 elif x['status']=='OK':q=x['type_or_code'];value=x['value'];assert x['count']==1
 else:q=x.get('code',x.get('type_or_code'));value=''
 rows.append('\t'.join([family+':'+id,family,op,at,hextext(a),bt,hextext(b),arg,x['status'],q,hextext(value),hextext(expr)]))
results={x['id']:x for x in nr}
for c in nc:
 op=c.get('op')
 if op is None or op=='even':continue
 a=c['args'][0].replace('\\t','\t').replace('\\n','\n').replace('\\xa0','\xa0')
 b=c['args'][1] if len(c['args'])>1 else ''
 at='integer' if op=='imod' else 'decimal'
 add('numeric111',c['id'],{'cast':'lex','imod':'mod'}.get(op,op),at,a,at if b and op!='round' else '',b if op!='round' else '',b if op=='round' else '',source=results[c['id']],expr=c['expr'])
# Additional independently authored scalar rows, retaining original type evidence.
extra=[('literal_30','literal','integer','123456789012345678901234567890','','',''),
 ('literal_fraction_29','literal','decimal','0.12345678901234567890123456789','','',''),
 ('integer_to_decimal','cast','integer','123456789012345678901234567890','','','decimal'),
 ('double_money','+','double','0.1','double','0.2',''),('decimal_money','+','decimal','0.1','decimal','0.2',''),
 ('double_div_zero','div','double','1','integer','0',''),('double_zero_div_zero','div','double','0','integer','0',''),
 ('double_idiv_inf','idiv','double','INF','integer','1',''),('double_mod_inf','mod','double','INF','integer','2',''),
 ('double_cast_nan','lex','decimal','NaN','','',''),('integer_round_tie','round','integer','250','','','-2'),
 ('integer_round_large_precision','round','integer','250','','','-10'),('decimal_round_large_precision','round','decimal','250','','','-10'),
 ('integer_div_30','div','integer','123456789012345678901234567890','integer','3',''),
 ('double_to_big_integer','cast','double','1e100','','','integer'),('decimal_from_double_inf','cast','double','INF','','','decimal')]
ncases={x['id']:x for x in nc}
for id,op,at,a,bt,b,arg in extra:add('numeric111',id,op,at,a,bt,b,arg,source=results[id],expr=ncases[id]['expr'])
results={x['id']:x for x in tr};cases={x['id']:x for x in tc}
selected=[('float-double','+','float','0.1','double','0.2',''),('double-decimal','+','decimal','9007199254740993','double','0',''),('double-to-decimal','cast','double','0.1','','','decimal'),('float-to-decimal','cast','float','0.1','','','decimal'),('double-subnormal','lex','double','4.9E-324','','',''),('double-underflow','div','integer','1','double','-1E-4000',''),('double-overflow','lex','double','1E4000','','','')]
for id,op,at,a,bt,b,arg in selected:add('typed280',id,op,at,a,bt,b,arg,source=results[id],expr=cases[id]['expr'])
lexemes=['0','-0','0.000001','0.000000999999999','1000000','999999.99','1E23','1E-45','1.4E-45','4.9E-324','2.2250738585072014E-308','1.7976931348623157E308','1.17549435E-38','3.4028235E38','1.000000059604644775390625','+INF','+NaN','Infinity','1_0','1e','+0.1',' 1e2 ','\u00a01','1e999999999999999999999999','-1e-999999999999999999999999']
results={x['id']:x for x in fr}
for at in ['float','double']:
 for i,a in enumerate(lexemes):
  id=f'{at}-lex-{i}';add('fresh',id,'lex',at,a,source=results[id],expr=results[id]['expr'])
# Independently specified primitive routes, not an expression parser.
boundaries=[
 ('cast','double','NaN','','','integer'),('cast','double','INF','','','decimal'),('cast','double','-1.9','','','integer'),
 ('idiv','double','NaN','integer','0',''),('idiv','double','INF','integer','0',''),('idiv','integer','1','double','INF',''),('idiv','integer','1','double','-0',''),('idiv','double','1E308','double','1E-308',''),
 ('mod','double','-0','integer','3',''),('mod','integer','3','double','INF',''),('mod','double','INF','integer','3',''),('mod','integer','3','double','0',''),
 ('round','float','0.49999997','','','0'),('round','double','0.49999999999999994','','','0'),('round','double','-0.5','','','0'),('round','double','1.005','','','2'),
 ('cast','decimal','1.000000059604644775390625','','','float'),('cast','decimal','1.000000059604644775390626','','','float'),('cast','decimal','9007199254740993','','','double'),
 ('number','boolean','true','','',''),('cast','integer','12','','','untypedAtomic'),('cast','boolean','true','','','integer'),('cast','double','NaN','','','boolean'),('cast','double','4.9E-324','','','decimal'),('cast','double','1.7976931348623157E308','','','decimal'),
 ('round','double','2.675','','','2'),('round','float','2.675','','','2'),('round','double','1E308','','','-308'),('lex','float','1.000000059604644775390626','','',''),('idiv','float','16777215','float','0.7',''),('mod','float','16777215','float','0.7',''),
 ('round','double','1.005','','','2'),('round','double','2.675','','','2'),('round','float','2.675','','','2'),('round','double','-1E-320','','','320')]
for i,(op,at,a,bt,b,arg) in enumerate(boundaries):
 id=f'boundary-{i}';add('fresh',id,op,at,a,bt,b,arg,source=results[id],expr=results[id]['expr'])
o.write_text('# id\tfamily\tprimitive\ta_type\ta_hex\tb_type\tb_hex\targument\tstatus\ttype_or_error_qname\tvalue_hex\texpression_hex\n'+'\n'.join(rows)+'\n')
print('extracted',len(rows),'development scalar rows')
