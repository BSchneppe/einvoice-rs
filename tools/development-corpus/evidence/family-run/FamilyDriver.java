import de.kosit.validationtool.api.*;
import de.kosit.validationtool.impl.*;
import net.sf.saxon.s9api.Processor;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.*;

/** Single ordered DEVELOPMENT observation pass. Calls unchanged Oracle.evaluate. */
public final class FamilyDriver {
    static Path root;
    static String sha(byte[] bytes) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
    }
    static Path local(String relative) throws Exception {
        Path p = root.resolve(relative).normalize();
        if (!p.startsWith(root) || !p.toRealPath().startsWith(root.toRealPath()))
            throw new IllegalStateException("Nonlocal closure path");
        return p;
    }
    static void verify(Path manifest, String expected) throws Exception {
        byte[] bytes = Files.readAllBytes(manifest);
        if (!sha(bytes).equals(expected)) throw new IllegalStateException("Closure manifest hash");
        for (String row : new String(bytes, StandardCharsets.UTF_8).split("\n")) {
            String[] parts = row.split("\t", -1);
            if (parts.length != 2 || !sha(Files.readAllBytes(local(parts[1]))).equals(parts[0]))
                throw new IllegalStateException("Consumed closure hash");
        }
    }
    static void save(String path, Object value) throws Exception {
        Files.writeString(root.resolve(path), Oracle.json(value)+"\n", StandardCharsets.UTF_8,
            StandardOpenOption.CREATE_NEW);
    }
    static Map<String,Object> loaded(Class<?> type, Path expected) throws Exception {
        Path actual = Path.of(type.getProtectionDomain().getCodeSource().getLocation().toURI()).toRealPath();
        if (!actual.equals(expected.toRealPath())) throw new IllegalStateException("Loaded class location: "+type.getName());
        String resource = "/"+type.getName().replace('.', '/')+".class";
        byte[] bytes;
        try (var stream = type.getResourceAsStream(resource)) {
            if (stream == null) throw new IllegalStateException("Missing loaded class resource");
            bytes = stream.readAllBytes();
        }
        return Oracle.map("class",type.getName(),"code_source_realpath",actual.toString(),
            "resource",type.getResource(resource).toString(),"resource_sha256",sha(bytes));
    }
    public static void main(String[] args) throws Exception {
        root = Path.of(args[0]).toRealPath();
        Path closure = local("runtime-closure.tsv");
        verify(closure,args[1]);
        Path requested = local("requests.tsv");
        if (!sha(Files.readAllBytes(requested)).equals(args[2])) throw new IllegalStateException("Requested order hash");
        if (!sha(Files.readAllBytes(local("snapshots/freeze-v1.json"))).equals(args[3])) throw new IllegalStateException("Freeze hash");
        if (!sha(Files.readAllBytes(local("snapshots/proposed-ledger.json"))).equals(args[4])) throw new IllegalStateException("Ledger hash");
        Path jdk = Path.of(args[5]).toRealPath();
        if (!Path.of(System.getProperty("java.home")).toRealPath().equals(jdk)) throw new IllegalStateException("JDK home");
        String actualCommand = ProcessHandle.current().info().command().orElseThrow();
        if (!Path.of(actualCommand).toRealPath().equals(jdk.resolve("bin/java").toRealPath())) throw new IllegalStateException("Java executable");
        var locations = List.of(loaded(FamilyDriver.class,root.resolve("classes")),
            loaded(Oracle.class,root.resolve("classes")),loaded(DefaultCheck.class,root.resolve("snapshots/validator.jar")),
            loaded(Processor.class,root.resolve("snapshots/validator.jar")),loaded(InputFactory.class,root.resolve("snapshots/validator.jar")));
        save("raw/closure-before.json",Oracle.map("verified",true,"closure_sha256",args[1],
            "requests_sha256",args[2],"freeze_sha256",args[3],"ledger_sha256",args[4],
            "loaded_classes",locations,"java_home",System.getProperty("java.home"),
            "java_home_realpath",jdk.toString(),"java_command",actualCommand,
            "java_command_realpath",Path.of(actualCommand).toRealPath().toString(),
            "java_version",System.getProperty("java.version"),"java_runtime_version",System.getProperty("java.runtime.version"),
            "jvm_arguments",java.lang.management.ManagementFactory.getRuntimeMXBean().getInputArguments()));
        var requests = Files.readAllLines(requested,StandardCharsets.UTF_8);
        if (requests.size()!=60) throw new IllegalStateException("Requested count");
        Path config = local("config/scenarios.xml");
        Processor processor = new Processor(false);
        Configuration configuration = Configuration.load(config.toUri(),config.getParent().toUri())
            .setResolvingMode(ResolvingMode.STRICT_LOCAL).build(processor);
        DefaultCheck check = new DefaultCheck(processor,configuration);
        save("raw/configuration-loaded.json",Oracle.map("configuration",config.toString(),
            "resolving_mode","STRICT_LOCAL","configuration_loads",1,"evaluations_requested",60,
            "warmup",false,"timing_measurements",false));
        for (int index=0;index<requests.size();index++) {
            String row = requests.get(index);
            String[] fields = row.split("\t",-1);
            if (fields.length!=4) throw new IllegalStateException("Request row shape");
            byte[] bytes = Files.readAllBytes(local(fields[2]));
            String hash = sha(bytes);
            if (!hash.equals(fields[3])) throw new IllegalStateException("Input hash: "+fields[0]);
            String prefix = String.format(Locale.ROOT,"raw/%02d-%s",index,fields[0]);
            save(prefix+".input-receipt.json",Oracle.map("ordinal",index,"original_request_row",row,
                "id",fields[0],"group",fields[1],"input",fields[2],"input_bytes",bytes.length,
                "input_sha256",hash,"closure_sha256",args[1],"driver_sha256",sha(Files.readAllBytes(local("FamilyDriver.java"))),
                "freeze_sha256",args[3],"ledger_sha256",args[4]));
            var result = Oracle.evaluate(check,new Oracle.Input(fields[0],fields[1],hash,bytes));
            String raw = Oracle.json(result)+"\n";
            Files.writeString(root.resolve(prefix+".bridge.json"),raw,StandardCharsets.UTF_8,StandardOpenOption.CREATE_NEW);
            System.out.print(raw);
            System.out.flush();
            save(prefix+".output-receipt.json",Oracle.map("ordinal",index,"id",fields[0],
                "input_sha256",hash,"bridge_record_sha256",sha(raw.getBytes(StandardCharsets.UTF_8)),
                "evaluate_returned",true));
        }
        verify(closure,args[1]);
        save("raw/closure-after.json",Oracle.map("verified",true,"closure_sha256",args[1],
            "requests_sha256",sha(Files.readAllBytes(requested)),"records_returned",60,
            "loaded_classes",locations,"timing_measurements",false));
    }
}
