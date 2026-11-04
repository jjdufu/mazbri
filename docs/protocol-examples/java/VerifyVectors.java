// mazbri 租户接入协议 v1 —— Java 参考示例（仅供测试 / 联调，仅 JDK 17+ 标准库）。
//
// 读取 docs/protocol-vectors/tenant-integration-v1.json 并独立实现：
//   - 传输信封：aes-*-gcm（AAD = tenant_id）、aes-*-cbc + HMAC（PKCS#5/7 填充）→ 与向量比对
//   - 请求绑定：hmac-v1（HmacSHA256/384/512）、rsa-v1（SHA256withRSA = PKCS#1 v1.5，确定性，不是 PSS）、
//     ed25519-v1（Ed25519）、ecdsa-v1（SHA256withECDSAinP1363Format，非确定性 → 验签）
//   - 回调签名：hex(HmacSHA256(secret, "{ts}.{compact json}"))
// JDK 无 SM2/SM3/SM4，可用 BouncyCastle 补齐。
//
// 用法：javac VerifyVectors.java && java VerifyVectors [path/to/tenant-integration-v1.json]
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.*;
import java.security.spec.*;
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
        Map<String, Object> asym = obj(keys.get("asymmetric"));
        List<String> skipped = new ArrayList<>();

        int nt = 0;
        for (Object o : (List<?>) doc.get("transport")) {
            Map<String, Object> t = obj(o);
            String alg = (String) t.get("data_alg"), macAlg = (String) t.get("mac_alg");
            Map<String, Object> e = obj(t.get("expected"));
            byte[] key = hex((String) t.get("data_key_hex")), iv = hex((String) t.get("iv_hex"));
            byte[] pt = ((String) t.get("plaintext_utf8")).getBytes(StandardCharsets.UTF_8);
            String ct, tag = "";
            if (alg.startsWith("aes-") && alg.endsWith("-gcm")) {
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
            Map<String, Object> env = obj(new Json((String) e.get("envelope_json")).value());
            String sig = "";
            if (macAlg != null) {
                String canon = String.join("\n", (String) common.get("tenant_id"), (String) common.get("http_path"),
                        (String) env.get("keyVersion"), String.valueOf(((Number) env.get("timestamp")).longValue()),
                        (String) env.get("nonce"), (String) env.get("iv"), (String) env.get("ciphertext"), (String) env.get("tag"));
                check(canon.equals(e.get("mac_canonical_string")), t.get("name") + " canonical");
                sig = toHex(hmac(macAlg, hex((String) t.get("mac_key_hex")), canon.getBytes(StandardCharsets.UTF_8)));
            }
            check(sig.equals(e.get("signature_hex")), t.get("name") + " signature");
            System.out.println("  ok transport " + t.get("name"));
            nt++;
        }

        int nb = 0;
        for (Object o : (List<?>) doc.get("binding")) {
            Map<String, Object> b = obj(o);
            Map<String, Object> e = obj(b.get("expected"));
            String sv = (String) b.get("signature_version"), alg = (String) b.get("alg");
            String canon = String.join("\n", (String) b.get("method"), (String) b.get("path_and_query"),
                    String.valueOf(((Number) b.get("timestamp")).longValue()), (String) b.get("nonce"), (String) b.get("body_utf8"));
            check(canon.equals(e.get("canonical_string")), b.get("name") + " canonical");
            byte[] msg = canon.getBytes(StandardCharsets.UTF_8);
            byte[] want = Base64.getDecoder().decode((String) e.get("signature_b64"));
            String keyName = ((String) b.get("key")).replace("keys.asymmetric.", "");
            Map<String, Object> kp = asym.containsKey(keyName) ? obj(asym.get(keyName)) : null;
            if (sv.equals("hmac-v1") && !alg.equals("hmac-sm3")) {
                byte[] k = Base64.getDecoder().decode((String) keys.get("hmac_binding_key_b64"));
                check(Arrays.equals(hmac(alg, k, msg), want), (String) b.get("name"));
            } else if (sv.equals("rsa-v1")) {
                String jca = "SHA" + alg.substring(alg.lastIndexOf("sha") + 3) + "withRSA"; // PKCS#1 v1.5
                check(Arrays.equals(sign(jca, "RSA", (String) kp.get("private_key_pem"), msg), want), (String) b.get("name"));
            } else if (sv.equals("ed25519-v1")) {
                check(Arrays.equals(sign("Ed25519", "Ed25519", (String) kp.get("private_key_pem"), msg), want), (String) b.get("name"));
            } else if (sv.equals("ecdsa-v1")) {
                Signature v = Signature.getInstance(alg.endsWith("sha256") ? "SHA256withECDSAinP1363Format" : "SHA384withECDSAinP1363Format");
                v.initVerify(KeyFactory.getInstance("EC").generatePublic(new X509EncodedKeySpec(pem((String) kp.get("public_key_pem")))));
                v.update(msg);
                check(v.verify(want), (String) b.get("name"));
            } else {
                skipped.add((String) b.get("name"));
                continue;
            }
            Map<String, Object> h = obj(e.get("headers"));
            check(h.size() == 5 && h.get("x-mazbri-signature-input").equals(alg + "=v1")
                    && h.get("x-mazbri-key-id").equals(b.get("kid")) && h.get("x-mazbri-nonce").equals(b.get("nonce"))
                    && h.get("x-mazbri-signature").equals(e.get("signature_b64"))
                    && h.get("x-mazbri-timestamp").equals(String.valueOf(((Number) b.get("timestamp")).longValue())), b.get("name") + " headers");
            System.out.println("  ok binding " + b.get("name"));
            nb++;
        }

        Map<String, Object> cb = obj(doc.get("callback"));
        Map<String, Object> ce = obj(cb.get("expected"));
        String signed = ((Number) cb.get("timestamp")).longValue() + "." + ce.get("payload_compact"); // 紧凑 JSON、键按字典序
        String cbSig = toHex(hmac("hmac-sha256", ((String) cb.get("secret_utf8")).getBytes(StandardCharsets.UTF_8), signed.getBytes(StandardCharsets.UTF_8)));
        check(cbSig.equals(obj(ce.get("headers")).get("X-Mazbri-Signature")), "callback");
        System.out.println("  ok callback");
        System.out.printf("PASS transport=%d binding=%d callback=1 skipped=%s%n", nt, nb, skipped);
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

    static byte[] sign(String jca, String kty, String privPem, byte[] msg) throws Exception {
        Signature s = Signature.getInstance(jca);
        s.initSign(KeyFactory.getInstance(kty).generatePrivate(new PKCS8EncodedKeySpec(pem(privPem))));
        s.update(msg);
        return s.sign();
    }

    static byte[] pem(String p) {
        return Base64.getDecoder().decode(p.replaceAll("-----[A-Z ]+-----", "").replaceAll("\\s", ""));
    }

    static byte[] hex(String s) { return HexFormat.of().parseHex(s); }
    static String toHex(byte[] b) { return HexFormat.of().formatHex(b); }
    static String b64(byte[] b) { return Base64.getEncoder().encodeToString(b); }
    static void check(boolean ok, String what) { if (!ok) throw new AssertionError("MISMATCH: " + what); }
    @SuppressWarnings("unchecked") static Map<String, Object> obj(Object o) { return (Map<String, Object>) o; }

    /** 极简 JSON 解析（仅为示例自包含；生产请用 Jackson/Gson）。 */
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
