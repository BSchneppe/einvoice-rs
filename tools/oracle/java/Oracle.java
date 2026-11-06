import de.kosit.validationtool.api.*;
import de.kosit.validationtool.impl.*;
import net.sf.saxon.s9api.Processor;
import org.w3c.dom.*;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;

/** Development bridge to the pinned KoSIT public API. No shipping dependency. */
public final class Oracle {
    static final String REP = "http://www.xoev.de/de/validator/varl/1";
    static final String SCENARIO = "http://www.xoev.de/de/validator/framework/1/scenarios";
    static volatile int sink;
    record Input(String id, String group, String hash, byte[] bytes) {}

    static Map<String,Object> map(Object... pairs) {
        Map<String,Object> m = new LinkedHashMap<>();
        for (int i=0;i<pairs.length;i+=2) m.put((String)pairs[i],pairs[i+1]);
        return m;
    }
    static String json(Object value) {
        if (value == null) return "null";
        if (value instanceof Boolean || value instanceof Number) return value.toString();
        if (value instanceof Map<?,?> m) {
            var parts = new ArrayList<String>();
            m.forEach((k,v)->parts.add(json(k.toString())+":"+json(v)));
            return "{"+String.join(",",parts)+"}";
        }
        if (value instanceof Collection<?> c) return "["+String.join(",",c.stream().map(Oracle::json).toList())+"]";
        StringBuilder s = new StringBuilder("\"");
        for (char ch : value.toString().toCharArray()) {
            if (ch=='"' || ch=='\\') s.append('\\').append(ch);
            else if (ch<32) s.append(String.format("\\u%04x",(int)ch));
            else s.append(ch);
        }
        return s.append('"').toString();
    }
    // Walk DOM children directly: the Saxon DOM facade's tag lookup returns empty
    // lists for this processor/name-pool combination. Namespace matching is explicit.
    static List<Element> elements(Node root, String namespace, String local) {
        var found = new ArrayList<Element>();
        var pending = new ArrayDeque<Node>();
        for(Node n=root.getLastChild();n!=null;n=n.getPreviousSibling()) pending.push(n);
        while(!pending.isEmpty()) {
            Node n=pending.pop();
            if(n instanceof Element e && namespace.equals(e.getNamespaceURI()) && local.equals(e.getLocalName())) found.add(e);
            for(Node child=n.getLastChild();child!=null;child=child.getPreviousSibling()) pending.push(child);
        }
        return found;
    }
    static String attr(Element e, String key) {return e.hasAttribute(key) ? e.getAttribute(key) : null;}
    static Map<String,Object> evaluate(DefaultCheck check, Input input) {
        var out = map("format",1,"id",input.id(),"sha256",input.hash(),
            "report_html_elements",null,"engine","KoSIT 1.6.3 / Saxon HE 12.9","target","xrechnung-3.0.2-cii-2026-08-31");
        try {
            Result result = check.checkInput(InputFactory.read(input.bytes(),input.id()));
            Document report = result.getReportDocument();
            out.put("report_html_elements",elements(report,"http://www.w3.org/1999/xhtml","html").size());
            var stages = new LinkedHashMap<String,Object>();
            var findings = new ArrayList<Object>();
            var structural = new ArrayList<Object>();
            var overrides = new HashMap<String,String>();
            var custom = elements(report,SCENARIO,"customLevel");
            for (int i=0;i<custom.size();i++) {
                Element e=(Element)custom.get(i);
                for (String id:e.getTextContent().trim().split("\\s+")) overrides.put(id,e.getAttribute("level"));
            }
            String scenario=null;
            var matches=elements(report,REP,"scenarioMatched");
            if(matches.size()>0) {
                var names=elements(matches.get(0),SCENARIO,"name");
                if(names.size()>0) scenario=names.get(0).getTextContent();
            }
            var steps=elements(report,REP,"validationStepResult");
            for(int i=0;i<steps.size();i++) {
                Element step=(Element)steps.get(i);
                String stage=step.getAttribute("id");
                stages.put(stage,step.getAttribute("valid").equals("true")?"valid":"invalid");
                var messages=elements(step,REP,"message");
                for(int j=0;j<messages.size();j++) {
                    Element m=(Element)messages.get(j);
                    String code=attr(m,"code"), level=attr(m,"level");
                    var diagnostic=map("id",code,"severity",level,"effective_severity",overrides.getOrDefault(code,level),
                        "location",attr(m,"xpathLocation"),"line",attr(m,"lineNumber"),"column",attr(m,"columnNumber"),
                        "message",m.getTextContent(),"stage",stage);
                    (stage.startsWith("val-sch")?findings:structural).add(diagnostic);
                }
            }
            boolean target="EN16931 XRechnung (CII)".equals(scenario);
            boolean complete = "valid".equals(stages.get("val-xml")) && "valid".equals(stages.get("val-xsd"))
                && stages.containsKey("val-sch.1") && stages.containsKey("val-sch.2");
            boolean success=result.isProcessingSuccessful();
            out.putAll(map("processing",success?"completed":"failed","processing_errors",result.getProcessingErrors(),
                "scenario",scenario,"target_matched",target,"stages",stages,
                "schema",stages.getOrDefault("val-xsd","not_evaluated"),
                "business",stages.containsKey("val-sch.1")&&stages.containsKey("val-sch.2")?"evaluated":"not_evaluated",
                "findings",findings,"structural_findings",structural,
                "reference_acceptance",result.getAcceptRecommendation().name().toLowerCase(Locale.ROOT),
                "acceptance",!success?"indeterminate":target&&complete&&result.isAcceptable()?"accepted":"rejected"));
        } catch(Exception e) {
            out.putAll(map("processing","failed","processing_errors",List.of(e.getClass().getName()+": "+e.getMessage()),
                "schema","not_evaluated","business","not_evaluated","findings",List.of(),"structural_findings",List.of(),
                "stages",Map.of(),"scenario",null,"target_matched",false,"reference_acceptance","undefined","acceptance","indeterminate"));
        }
        return out;
    }
    public static void main(String[] args) throws Exception {
        long start=System.nanoTime();
        Path config=Path.of(args[0]);
        Processor processor=new Processor(false);
        Configuration configuration=Configuration.load(config.toUri(),config.getParent().toUri())
            .setResolvingMode(ResolvingMode.STRICT_LOCAL).build(processor);
        DefaultCheck check=new DefaultCheck(processor,configuration);
        long init=System.nanoTime()-start;
        var inputs=new ArrayList<Input>();
        long ioStart=System.nanoTime();
        for(String line:Files.readAllLines(Path.of(args[1]))) {
            String[] row=line.split("\t");
            byte[] bytes=Files.readAllBytes(Path.of(row[2]));
            String hash=HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
            if(!hash.equals(row[3])) throw new IllegalArgumentException("Input hash mismatch: "+row[0]);
            inputs.add(new Input(row[0],row[1],hash,bytes));
        }
        long io=System.nanoTime()-ioStart;
        int warmup=Integer.parseInt(args[2]), runs=Integer.parseInt(args[3]), repeats=Integer.parseInt(args[4]);
        var expected=new HashMap<String,String>();
        for(Input input:inputs) {
            String outcome=json(evaluate(check,input)); expected.put(input.id(),outcome);
            System.out.println(outcome);
        }
        for(int w=0;w<warmup;w++) for(Input input:inputs) sink^=json(evaluate(check,input)).hashCode();
        for(int run=0;run<runs;run++) {
            var timings=new LinkedHashMap<String,List<Long>>();
            var order=new ArrayList<>(inputs); Collections.shuffle(order,new Random(271828L+run));
            for(int repeat=0;repeat<repeats;repeat++) for(Input input:order) {
                long before=System.nanoTime();
                String outcome=json(evaluate(check,input));
                long elapsed=System.nanoTime()-before;
                if(!outcome.equals(expected.get(input.id()))) throw new IllegalStateException("Non-deterministic outcome: "+input.id());
                timings.computeIfAbsent(input.group(),k->new ArrayList<>()).add(elapsed);
                sink^=outcome.hashCode();
            }
            System.out.println(json(map("type","benchmark_run","run",run,"latencies_ns",timings)));
        }
        var memory=java.lang.management.ManagementFactory.getMemoryPoolMXBeans().stream()
            .filter(p->p.getType()==java.lang.management.MemoryType.HEAP)
            .mapToLong(p->p.getPeakUsage().getUsed()).sum();
        System.out.println(json(map("type","metadata","initialization_ns",init,"input_io_ns",io,
            "java",System.getProperty("java.runtime.version"),"vm",System.getProperty("java.vm.name"),
            "os",System.getProperty("os.name"),"arch",System.getProperty("os.arch"),
            "processors",Runtime.getRuntime().availableProcessors(),"jvm_args",java.lang.management.ManagementFactory.getRuntimeMXBean().getInputArguments(),
            "peak_heap_pool_sum_bytes",memory,"warmup_passes",warmup,"runs",runs,"repeats",repeats)));
    }
}
