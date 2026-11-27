// mazbri 租户接入协议 v1 —— Java 参考示例（仅供测试 / 联调，仅 JDK 17+ 标准库）。
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import javax.crypto.Cipher;
import javax.crypto.Mac;
import javax.crypto.spec.*;

public final class VerifyVectors {
    public static void main(String[] args) throws Exception {
        String path = args.length > 0 ? args[0] : "../../protocol-vectors/tenant-integration-v1.json";
        Map<String, Object> doc = obj(new Json(Files.readString(Path.of(path))).value());
        check(Boolean.TRUE.equals(doc.get("test_only")), "test_only");
        Map<String, Object> common = obj(doc.get("common"));
        Map<String, Object> keys = obj(doc.get("keys"));
        List<String> skipped = new ArrayList<>();

        byte[] masterKey = Base64.getDecoder().decode((String) keys.get("tenant_secret_key_b64"));
        int nt = 0;
        for (Object o : (List<?>) doc.get("transport")) {
            Map<String, Object> t = obj(o);
            String alg = (String) t.get("data_alg"), macAlg = (String) t.get("mac_alg");
            String dir = t.get("direction") != null ? (String) t.get("direction") : "c2g";
            Map<String, Object> e = obj(t.get("expected"));
            byte[] key = hex((String) t.get("data_key_hex")), iv = hex((String) t.get("iv_hex"));
            byte[] derivedEnc = hkdfSha256(
                    ((String) common.get("tenant_id")).getBytes(StandardCharsets.UTF_8),
                    masterKey,
                    ("mazbri-transport-v1|" + common.get("tenant_id") + "|" + dir + "|enc").getBytes(StandardCharsets.UTF_8),
                    key.length);
            check(Arrays.equals(key, derivedEnc), t.get("name") + " hkdf enc");
            Map<String, Object> env = obj(new Json((String) e.get("envelope_json")).value());
            String expectedAad = String.join("\n", "v1", dir, (String) common.get("tenant_id"),
                    (String) common.get("http_path"), alg,
                    macAlg != null ? macAlg : "", String.valueOf(((Number) env.get("timestamp")).longValue()),
                    (String) env.get("nonce"), (String) env.get("iv"));
            byte[] pt = ((String) t.get("plaintext_utf8")).getBytes(StandardCharsets.UTF_8);
            String ct, tag = "";
            if (alg.startsWith("aes-") && alg.endsWith("-gcm")) {
                check(expectedAad.equals(t.get("aad_utf8")), t.get("name") + " aad");
                Cipher c = Cipher.getInstance("AES/GCM/NoPadding");
                c.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(key, "AES"), new GCMParameterSpec(128, iv));
                c.updateAAD(((String) t.get("aad_utf8")).getBytes(StandardCharsets.UTF_8));
                byte[] out = c.doFinal(pt);
                ct = b64(Arrays.copyOf(out, out.length - 16));
                tag = b64(Arrays.copyOfRange(out, out.length - 16, out.length));
            } else if (alg.startsWith("aes-") && alg.endsWith("-cbc")) {
                Cipher c = Cipher.getInstance("AES/CBC/PKCS5Padding");
                c.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(key, "AES"), new IvParameterSpec(iv));
                ct = b64(c.doFinal(pt));
            } else {
                skipped.add((String) t.get("name"));
                continue;
            }
            check(ct.equals(e.get("ciphertext_b64")) && tag.equals(e.get("tag_b64")), t.get("name") + " cipher");
            String sig = "";
            if (macAlg != null) {
                byte[] derivedMac = hkdfSha256(
                        ((String) common.get("tenant_id")).getBytes(StandardCharsets.UTF_8),
                        masterKey,
                        ("mazbri-transport-v1|" + common.get("tenant_id") + "|" + dir + "|mac").getBytes(StandardCharsets.UTF_8),
                        32);
                check(Arrays.equals(hex((String) t.get("mac_key_hex")), derivedMac), t.get("name") + " hkdf mac");
                String canon = String.join("\n", expectedAad, (String) env.get("ciphertext"), (String) env.get("tag"));
                check(canon.equals(e.get("mac_canonical_string")), t.get("name") + " canonical");
                sig = toHex(hmac(macAlg, derivedMac, canon.getBytes(StandardCharsets.UTF_8)));
            }
            check(sig.equals(e.get("signature_hex")), t.get("name") + " signature");
            System.out.println("  ok transport " + t.get("name"));
            nt++;
        }

        Map<String, Object> cb = obj(doc.get("callback"));
        Map<String, Object> ce = obj(cb.get("expected"));
        String signed = ((Number) cb.get("timestamp")).longValue() + "." + ce.get("payload_compact");
        String cbSig = toHex(hmac("hmac-sha256", ((String) cb.get("secret_utf8")).getBytes(StandardCharsets.UTF_8), signed.getBytes(StandardCharsets.UTF_8)));
        check(cbSig.equals(obj(ce.get("headers")).get("X-Mazbri-Signature")), "callback");
        System.out.println("  ok callback");
        System.out.printf("PASS transport=%d callback=1 skipped=%s%n", nt, skipped);
    }

    static byte[] hkdfSha256(byte[] salt, byte[] ikm, byte[] info, int length) throws Exception {
        byte[] prk = hmac("hmac-sha256", salt, ikm);
        byte[] out = new byte[length];
        byte[] prev = new byte[0];
        int offset = 0;
        for (int counter = 1; offset < length; counter++) {
            Mac m = Mac.getInstance("HmacSHA256");
            m.init(new SecretKeySpec(prk, "HmacSHA256"));
            m.update(prev);
            m.update(info);
            m.update((byte) counter);
            prev = m.doFinal();
            int copy = Math.min(prev.length, length - offset);
            System.arraycopy(prev, 0, out, offset, copy);
            offset += copy;
        }
        return out;
    }

    static byte[] hmac(String alg, byte[] key, byte[] msg) throws Exception {
        String jca = switch (alg) {
            case "hmac-sha256" -> "HmacSHA256";
            case "hmac-sha384" -> "HmacSHA384";
            case "hmac-sha512" -> "HmacSHA512";
            case "hmac-md5" -> "HmacMD5";
            default -> throw new IllegalArgumentException(alg);
        };
        Mac m = Mac.getInstance(jca);
        m.init(new SecretKeySpec(key, jca));
        return m.doFinal(msg);
    }

    static byte[] hex(String s) { return HexFormat.of().parseHex(s); }
    static String toHex(byte[] b) { return HexFormat.of().formatHex(b); }
    static String b64(byte[] b) { return Base64.getEncoder().encodeToString(b); }
    static void check(boolean ok, String what) { if (!ok) throw new AssertionError("MISMATCH: " + what); }
    @SuppressWarnings("unchecked") static Map<String, Object> obj(Object o) { return (Map<String, Object>) o; }

    static final class Json {
        private final String s; private int i;
        Json(String s) { this.s = s; }
        Object value() {
            ws();
            char c = s.charAt(i);
            if (c == '{') { i++; Map<String, Object> m = new LinkedHashMap<>(); ws();
                if (s.charAt(i) == '}') { i++; return m; }
                while (true) { ws(); String k = str(); ws(); i++; m.put(k, value()); ws(); if (s.charAt(i++) == '}') return m; } }
            if (c == '[') { i++; List<Object> l = new ArrayList<>(); ws();
                if (s.charAt(i) == ']') { i++; return l; }
                while (true) { l.add(value()); ws(); if (s.charAt(i++) == ']') return l; } }
            if (c == '"') return str();
            if (s.startsWith("true", i)) { i += 4; return true; }
            if (s.startsWith("false", i)) { i += 5; return false; }
            if (s.startsWith("null", i)) { i += 4; return null; }
            int st = i; while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
            return Double.parseDouble(s.substring(st, i));
        }
        private String str() {
            StringBuilder b = new StringBuilder(); i++;
            while (true) { char c = s.charAt(i++);
                if (c == '"') return b.toString();
                if (c != '\\') { b.append(c); continue; }
                char e = s.charAt(i++);
                switch (e) { case 'n' -> b.append('\n'); case 't' -> b.append('\t'); case 'r' -> b.append('\r');
                    case 'b' -> b.append('\b'); case 'f' -> b.append('\f');
                    case 'u' -> { b.append((char) Integer.parseInt(s.substring(i, i + 4), 16)); i += 4; }
                    default -> b.append(e); } }
        }
        private void ws() { while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++; }
    }
}
