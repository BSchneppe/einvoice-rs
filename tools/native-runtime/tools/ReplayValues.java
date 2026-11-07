import net.sf.saxon.s9api.*;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;

/** Read-only scalar development replay. Never writes fixture expectations. */
public class ReplayValues {
    private static final String JAR_SHA256 =
        "799e64befca97d4080e03608c80b85dd5a5ecc5f4ae4f35d1116ec2855b9a7c9";
    private static String decode(String s) {
        return new String(HexFormat.of().parseHex(s), StandardCharsets.UTF_8);
    }
    public static void main(String[] args) throws Exception {
        if (args.length == 0) throw new IllegalArgumentException("Supply fixture TSV paths");
        if (!"27.0.1+11-FR".equals(System.getProperty("java.runtime.version"))
                || !"Amazon.com Inc.".equals(System.getProperty("java.vendor"))) {
            throw new IllegalStateException("Requires Amazon Corretto 27.0.1.0.11");
        }
        Path jar = Path.of(Processor.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        try (InputStream in = Files.newInputStream(jar)) {
            byte[] buf = new byte[65536]; int n;
            while ((n = in.read(buf)) != -1) digest.update(buf, 0, n);
        }
        if (!JAR_SHA256.equals(HexFormat.of().formatHex(digest.digest()))) {
            throw new IllegalStateException("Loaded reference JAR hash mismatch");
        }
        Processor p = new Processor(false);
        if (!"12.9".equals(p.getSaxonProductVersion())) throw new IllegalStateException("Saxon version mismatch");
        Set<String> ids = new HashSet<>();
        for (String path : args) {
            for (String line : Files.readAllLines(Path.of(path), StandardCharsets.UTF_8)) {
                if (line.startsWith("#")) continue;
                String[] f = line.split("\t", -1);
                if (f.length != 12 || f[0].isEmpty() || !ids.add(f[0])
                        || !(f[8].equals("OK") || f[8].equals("ERROR"))) {
                    throw new IllegalArgumentException("Malformed/duplicate fixture row");
                }
                XPathCompiler c = p.newXPathCompiler();
                c.setLanguageVersion(f[1].equals("typed280") ? "2.0" : "3.1");
                try {
                    XdmValue v = c.compile(decode(f[11])).load().evaluate();
                    if (!f[8].equals("OK") || v.size() != 1
                            || !(v.itemAt(0) instanceof XdmAtomicValue)) {
                        throw new AssertionError(f[0] + ": status/cardinality mismatch: " + v);
                    }
                    XdmAtomicValue a = (XdmAtomicValue) v.itemAt(0);
                    if (!a.getPrimitiveTypeName().getClarkName().equals(f[9])
                            || !a.getStringValue().equals(decode(f[10]))) {
                        throw new AssertionError(f[0] + ": type/value mismatch: "
                            + a.getPrimitiveTypeName() + ": " + a.getStringValue());
                    }
                } catch (SaxonApiException e) {
                    if (!f[8].equals("ERROR") || e.getErrorCode() == null
                            || !e.getErrorCode().getClarkName().equals(f[9])) {
                        throw new AssertionError(f[0] + ": unexpected error " + e.getErrorCode(), e);
                    }
                }
            }
        }
        if (ids.size() != 725) throw new AssertionError("Expected 725 development rows, got " + ids.size());
        System.out.println("PASS: 725 scalar development expressions; exact singleton type/value or fault QName; "
            + "loaded JAR, Saxon HE 12.9 and Corretto pins verified. No XPath/invoice parity claim.");
    }
}
