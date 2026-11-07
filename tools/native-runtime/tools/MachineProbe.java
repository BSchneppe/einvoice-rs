import net.sf.saxon.s9api.*;
import javax.xml.transform.stream.StreamSource;
import java.io.*;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
public final class MachineProbe {
 static String enc(String s){return Base64.getEncoder().encodeToString(s.getBytes(StandardCharsets.UTF_8));}
 static String dec(String s){return new String(Base64.getDecoder().decode(s), StandardCharsets.UTF_8);}
 public static void main(String[] args) throws Exception {
  Processor p=new Processor(false);
  System.err.println("Saxon="+p.getSaxonEdition()+" "+p.getSaxonProductVersion());
  XPathCompiler c=p.newXPathCompiler(); c.setLanguageVersion("2.0"); c.declareNamespace("p", "urn:p");
  XdmNode doc=p.newDocumentBuilder().build(new StreamSource(new File(args[1])));
  for(String line:Files.readAllLines(Path.of(args[0]))) {
   String[] parts=line.split("\t",-1); String id=parts[0],kind=parts[1],input=dec(parts[2]);
   String phase="compile"; ArrayList<XmlProcessingError> errors=new ArrayList<>();
   try {
    XdmValue v;
    if(kind.equals("xpath2")) {
     XPathExecutable ex=c.compile(input); phase="evaluate";
     XPathSelector sel=ex.load();sel.setContextItem(doc);v=sel.evaluate();
     System.out.println(id+"\tOK\t"+v.size());
     for(XdmItem item:v) {
      String type=item instanceof XdmAtomicValue?((XdmAtomicValue)item).getPrimitiveTypeName().getClarkName():"node:"+((XdmNode)item).getNodeKind().toString();
      System.out.println(id+"\tITEM\t"+enc(type)+"\t"+enc(item.getStringValue()));
     }
    } else {
     XsltCompiler xc=p.newXsltCompiler();xc.setErrorList(errors);
     XsltExecutable ex=xc.compile(new StreamSource(new StringReader(input)));phase="evaluate";
     XsltTransformer t=ex.load();t.setInitialContextNode(doc);
     ByteArrayOutputStream out=new ByteArrayOutputStream();Serializer ser=p.newSerializer(out);t.setDestination(ser);t.transform();
     System.out.println(id+"\tTEXT\t"+enc(out.toString(StandardCharsets.UTF_8)));
    }
   } catch(SaxonApiException e) {
    String code=e.getErrorCode()==null?"":e.getErrorCode().getClarkName();
    String msg=e.getMessage();
    for(XmlProcessingError error:errors) if(!error.isWarning()) {
     if(code.isEmpty() && error.getErrorCode()!=null)code=error.getErrorCode().getClarkName();
     msg += " | "+error.getMessage();
    }
    System.out.println(id+"\tERROR\t"+phase+"\t"+enc(code)+"\t"+enc(msg));
   }
  }
 }
}
