import net.sf.saxon.s9api.*;
import javax.xml.transform.stream.StreamSource;
import java.io.StringReader;
import java.nio.file.*;
import java.util.Base64;
import java.nio.charset.StandardCharsets;

public final class XPathProbe {
    public static void main(String[] args) throws Exception {
        Processor processor = new Processor(false);
        XPathCompiler compiler = processor.newXPathCompiler();
        compiler.setLanguageVersion("3.1");
        compiler.declareNamespace("r", "urn:probe");
        compiler.declareNamespace("u", "utils");
        XdmNode document = processor.newDocumentBuilder().build(new StreamSource(new StringReader(
            "<root xmlns:r='urn:probe'><r:a n='1'><r:b>42</r:b></r:a><r:a n='2'/></root>")));
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            String[] parts = line.split("\t");
            String expression = new String(Base64.getDecoder().decode(parts[1]), StandardCharsets.UTF_8);
            try {
                XPathSelector selector = compiler.compile("string-join(("+expression+") ! string(.), '|')").load();
                selector.setContextItem(document);
                System.out.println(Oracle.json(Oracle.map("id",parts[0],"value",selector.evaluateSingle().getStringValue(),"error",false)));
            } catch (SaxonApiException error) {
                System.out.println(Oracle.json(Oracle.map("id",parts[0],"value",null,"error",true,"detail",error.getMessage())));
            }
        }
    }
}
