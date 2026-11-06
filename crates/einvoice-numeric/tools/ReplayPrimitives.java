import net.sf.saxon.s9api.*;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.HashSet;
import java.util.HexFormat;

/** Read-only developer replay; never generates or replaces fixture expectations. */
public class ReplayPrimitives {
    private static final String JAR_SHA256 =
        "799e64befca97d4080e03608c80b85dd5a5ecc5f4ae4f35d1116ec2855b9a7c9";

    private static String decode(String text) {
        return new String(HexFormat.of().parseHex(text), StandardCharsets.UTF_8);
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 1) {
            throw new IllegalArgumentException("Expected path to committed primitives.tsv");
        }
        if (!"27.0.1+11-FR".equals(System.getProperty("java.runtime.version"))
                || !"Amazon.com Inc.".equals(System.getProperty("java.vendor"))) {
            throw new IllegalStateException("Replay requires pinned Amazon Corretto 27.0.1.0.11");
        }
        Path jar = Path.of(Processor.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        try (InputStream input = Files.newInputStream(jar)) {
            byte[] buffer = new byte[65536];
            int count;
            while ((count = input.read(buffer)) != -1) digest.update(buffer, 0, count);
        }
        if (!JAR_SHA256.equals(HexFormat.of().formatHex(digest.digest()))) {
            throw new IllegalStateException("Loaded Saxon JAR does not match pinned KoSIT SHA-256");
        }
        Processor processor = new Processor(false);
        if (!"12.9".equals(processor.getSaxonProductVersion())) {
            throw new IllegalStateException("Unexpected Saxon product version");
        }
        XPathCompiler compiler = processor.newXPathCompiler();
        compiler.setLanguageVersion("3.1");
        compiler.declareNamespace("xs", "http://www.w3.org/2001/XMLSchema");
        HashSet<String> ids = new HashSet<>();
        for (String line : Files.readAllLines(Path.of(args[0]), StandardCharsets.UTF_8)) {
            if (line.startsWith("#")) continue;
            String[] fields = line.split("\t", -1);
            if (fields.length != 9 || fields[0].isEmpty() || !ids.add(fields[0])
                    || !(fields[6].equals("OK") || fields[6].equals("ERROR"))) {
                throw new IllegalArgumentException("Malformed or duplicate fixture row");
            }
            String expected = decode(fields[7]);
            try {
                XdmValue result = compiler.compile(decode(fields[8])).load().evaluate();
                if (!fields[6].equals("OK") || result.size() != 1
                        || !result.itemAt(0).getStringValue().equals(expected)) {
                    throw new AssertionError(fields[0] + ": expected " + fields[6] + " "
                        + expected + "; got cardinality=" + result.size() + ", value=" + result);
                }
            } catch (SaxonApiException error) {
                QName code = error.getErrorCode();
                if (!fields[6].equals("ERROR") || code == null
                        || !"http://www.w3.org/2005/xqt-errors".equals(code.getNamespace())
                        || !expected.equals(code.getLocalName())) {
                    throw new AssertionError(fields[0] + ": unexpected error " + code, error);
                }
            }
        }
        if (ids.size() != 57) throw new AssertionError("Expected 57 rows, got " + ids.size());
        System.out.println("PASS: 57 expressions; fresh singleton values/error codes match; "
            + "Saxon 12.9, pinned KoSIT JAR and Corretto verified");
    }
}
