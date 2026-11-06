import net.sf.saxon.s9api.*;
import javax.xml.transform.stream.StreamSource;
import java.io.StringReader;
import java.nio.file.*;
import java.util.*;

/** Executes official rule test containers without asserting whole-invoice validity. */
public final class ScopedOracle {
    public static void main(String[] args) throws Exception {
        Processor processor=new Processor(false);
        XPathCompiler xpath=processor.newXPathCompiler();
        xpath.declareNamespace("t","http://difi.no/xsd/vefa/validator/1.0");
        xpath.declareNamespace("svrl","http://purl.oclc.org/dsdl/svrl");
        XsltExecutable rules=processor.newXsltCompiler().compile(new StreamSource(Path.of(args[0]).toFile()));
        for(int file=1;file<args.length;file++) {
            Path path=Path.of(args[file]);
            XdmNode container=processor.newDocumentBuilder().build(new StreamSource(path.toFile()));
            var scope=new ArrayList<String>();
            for(XdmItem id:xpath.evaluate("/t:testSet/t:assert/t:scope",container)) scope.add(id.getStringValue());
            int index=0;
            for(XdmItem test:xpath.evaluate("/t:testSet/t:test",container)) {
                var out=Oracle.map("format",1,"id",path.getFileName()+":"+index++,"scope","rule_test_fragment",
                    "rule_ids",scope,"schema","not_evaluated","acceptance","not_applicable");
                try {
                    XdmItem fragment=xpath.evaluateSingle("*[local-name()='CrossIndustryInvoice']",test);
                    XdmNode invoice=processor.newDocumentBuilder().build(new StreamSource(new StringReader(fragment.toString())));
                    XsltTransformer transform=rules.load();
                    transform.setInitialContextNode(invoice);
                    XdmDestination destination=new XdmDestination(); transform.setDestination(destination); transform.transform();
                    var failed=new HashSet<String>();
                    var findings=new ArrayList<Object>();
                    for(XdmItem item:xpath.evaluate("//svrl:failed-assert",destination.getXdmNode())) {
                        XdmNode node=(XdmNode)item;
                        String id=node.getAttributeValue(new QName("id")); failed.add(id);
                        findings.add(Oracle.map("id",id,"severity",node.getAttributeValue(new QName("flag")),
                            "location",node.getAttributeValue(new QName("location")),"message",node.getStringValue().trim()));
                    }
                    boolean expectations=true;
                    for(XdmItem id:xpath.evaluate("t:assert/t:success",test)) expectations &= !failed.contains(id.getStringValue());
                    for(XdmItem id:xpath.evaluate("t:assert/t:error",test)) expectations &= failed.contains(id.getStringValue());
                    out.putAll(Oracle.map("processing","completed","expectations_met",expectations,"findings",findings));
                } catch(Exception error) {
                    out.putAll(Oracle.map("processing","failed","expectations_met",false,"error",error.toString()));
                }
                System.out.println(Oracle.json(out));
            }
        }
    }
}
