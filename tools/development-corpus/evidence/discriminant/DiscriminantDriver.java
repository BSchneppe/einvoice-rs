import de.kosit.validationtool.api.*;
import de.kosit.validationtool.impl.*;
import net.sf.saxon.s9api.Processor;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.HexFormat;

public final class DiscriminantDriver {
    static String sha(byte[] data) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(data));
    }
    static void verifyClosure(Path root, Path manifest) throws Exception {
        for (String line : Files.readAllLines(manifest, StandardCharsets.UTF_8)) {
            String[] parts = line.split("\t", 2);
            Path p = root.resolve(parts[1]).normalize();
            if (!p.startsWith(root) || !sha(Files.readAllBytes(p)).equals(parts[0]))
                throw new IllegalStateException("Snapshot closure mismatch");
        }
    }
    public static void main(String[] args) throws Exception {
        Path root = Path.of(args[0]).toAbsolutePath().normalize();
        Path manifest = root.resolve(args[1]);
        byte[] manifestBytes = Files.readAllBytes(manifest);
        if (!sha(manifestBytes).equals(args[2])) throw new IllegalStateException("Closure manifest mismatch");
        verifyClosure(root, manifest);
        Path jar = Path.of(DefaultCheck.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        if (!jar.equals(root.resolve("snapshots/validator.jar"))) throw new IllegalStateException("Loaded jar mismatch");
        Path classes = Path.of(Oracle.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        if (!classes.equals(root.resolve("classes"))) throw new IllegalStateException("Loaded bridge mismatch");
        if (!System.getProperty("java.home").equals(args[7])) throw new IllegalStateException("JDK home mismatch");
        byte[] input = Files.readAllBytes(root.resolve(args[4]));
        String inputHash = sha(input);
        if (!inputHash.equals(args[5])) throw new IllegalStateException("Input hash mismatch");
        Path config = root.resolve("config/scenarios.xml");
        Processor processor = new Processor(false);
        Configuration configuration = Configuration.load(config.toUri(), config.getParent().toUri())
            .setResolvingMode(ResolvingMode.STRICT_LOCAL).build(processor);
        DefaultCheck check = new DefaultCheck(processor, configuration);
        System.out.println(Oracle.json(Oracle.evaluate(check, new Oracle.Input(args[3], "development-discriminant", inputHash, input))));
        verifyClosure(root, manifest);
        if (!sha(Files.readAllBytes(manifest)).equals(args[2])) throw new IllegalStateException("Closure manifest drift");
        Files.writeString(root.resolve(args[6]), Oracle.json(Oracle.map(
            "input_sha256",inputHash,"closure_sha256",args[2],"closure_pre_post_verified",true,
            "loaded_jar",jar.toString(),"loaded_oracle_classes",classes.toString(),
            "java_home",System.getProperty("java.home"),"java_version",System.getProperty("java.version"),
            "java_runtime_version",System.getProperty("java.runtime.version")))+"\n", StandardCharsets.UTF_8,
            StandardOpenOption.CREATE_NEW);
    }
}
