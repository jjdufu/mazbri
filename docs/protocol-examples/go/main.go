// mazbri 租户接入协议 v1 —— Go 参考示例（仅供测试 / 联调，纯标准库）。
//
// 读取 docs/protocol-vectors/tenant-integration-v1.json 并独立实现：
//   - 传输信封：aes-*-gcm / chacha 之外的 AEAD 由标准库 AES-GCM 覆盖；aes-*-cbc + HMAC（PKCS#7 填充）
//   - 请求绑定：hmac-v1（hmac-sha256）、rsa-v1（PKCS#1 v1.5，确定性）、ed25519-v1、ecdsa-v1（P1363 r||s，验签）
//   - 回调签名：hex(HMAC-SHA256(secret, "{ts}.{compact json}"))
//
// 标准库没有 SM3 / SM4 / SM2 / ChaCha20，相关向量跳过（可用 github.com/tjfoc/gmsm、golang.org/x/crypto 补齐）。
//
// 用法：go run . [path/to/tenant-integration-v1.json]
package main

import (
	"bytes"
	"crypto"
	"crypto/aes"
	"crypto/cipher"
	"crypto/ecdsa"
	"crypto/ed25519"
	"crypto/hmac"
	"crypto/rsa"
	"crypto/sha256"
	"crypto/sha512"
	"crypto/x509"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"encoding/pem"
	"fmt"
	"hash"
	"math/big"
	"os"
	"strconv"
	"strings"
)

type Doc struct {
	TestOnly bool `json:"test_only"`
	Common   struct {
		HTTPPath string `json:"http_path"`
		TenantID string `json:"tenant_id"`
	} `json:"common"`
	Keys struct {
		HmacBindingKeyB64 string                       `json:"hmac_binding_key_b64"`
		Asymmetric        map[string]map[string]string `json:"asymmetric"`
	} `json:"keys"`
	Transport []struct {
		Name       string  `json:"name"`
		DataAlg    string  `json:"data_alg"`
		MacAlg     *string `json:"mac_alg"`
		DataKeyHex string  `json:"data_key_hex"`
		MacKeyHex  *string `json:"mac_key_hex"`
		IVHex      string  `json:"iv_hex"`
		AADUTF8    *string `json:"aad_utf8"`
		Plaintext  string  `json:"plaintext_utf8"`
		Expected   struct {
			CiphertextB64 string  `json:"ciphertext_b64"`
			TagB64        string  `json:"tag_b64"`
			MacCanonical  *string `json:"mac_canonical_string"`
			SignatureHex  string  `json:"signature_hex"`
			EnvelopeJSON  string  `json:"envelope_json"`
		} `json:"expected"`
	} `json:"transport"`
	Binding []struct {
		Name         string `json:"name"`
		Version      string `json:"signature_version"`
		Alg          string `json:"alg"`
		Kid          string `json:"kid"`
		Key          string `json:"key"`
		Method       string `json:"method"`
		PathAndQuery string `json:"path_and_query"`
		Timestamp    int64  `json:"timestamp"`
		Nonce        string `json:"nonce"`
		Body         string `json:"body_utf8"`
		Expected     struct {
			Canonical    string            `json:"canonical_string"`
			SignatureB64 string            `json:"signature_b64"`
			Headers      map[string]string `json:"headers"`
		} `json:"expected"`
	} `json:"binding"`
	Callback struct {
		Payload   json.RawMessage `json:"payload"`
		Secret    string          `json:"secret_utf8"`
		Timestamp int64           `json:"timestamp"`
		Expected  struct {
			Headers map[string]string `json:"headers"`
		} `json:"expected"`
	} `json:"callback"`
}

type envelope struct {
	KeyVersion string `json:"keyVersion"`
	Timestamp  int64  `json:"timestamp"`
	Nonce      string `json:"nonce"`
	IV         string `json:"iv"`
	Ciphertext string `json:"ciphertext"`
	Tag        string `json:"tag"`
}

func must(err error) {
	if err != nil {
		panic(err)
	}
}

func check(ok bool, what string) {
	if !ok {
		panic("MISMATCH: " + what)
	}
}

func unhex(s string) []byte { b, err := hex.DecodeString(s); must(err); return b }
func b64(b []byte) string   { return base64.StdEncoding.EncodeToString(b) }

func macHash(alg string) func() hash.Hash {
	switch alg {
	case "hmac-sha256":
		return sha256.New
	case "hmac-sha384":
		return sha512.New384
	case "hmac-sha512":
		return sha512.New
	}
	return nil
}

func hmacOf(alg string, key, msg []byte) []byte {
	m := hmac.New(macHash(alg), key)
	m.Write(msg)
	return m.Sum(nil)
}

// encrypt 返回 (ciphertext 不含 tag, tag)；ok=false 表示标准库不支持该算法。
func encrypt(alg string, key, iv, aad, pt []byte) (ct, tag []byte, ok bool) {
	block, err := aes.NewCipher(key)
	if err != nil || !strings.HasPrefix(alg, "aes-") {
		return nil, nil, false
	}
	switch {
	case strings.HasSuffix(alg, "-gcm"):
		g, err := cipher.NewGCM(block)
		must(err)
		out := g.Seal(nil, iv, pt, aad)
		return out[:len(out)-16], out[len(out)-16:], true
	case strings.HasSuffix(alg, "-cbc"):
		pad := aes.BlockSize - len(pt)%aes.BlockSize
		p := append(append([]byte{}, pt...), bytes.Repeat([]byte{byte(pad)}, pad)...)
		out := make([]byte, len(p))
		cipher.NewCBCEncrypter(block, iv).CryptBlocks(out, p)
		return out, nil, true
	case strings.HasSuffix(alg, "-ctr"):
		out := make([]byte, len(pt))
		cipher.NewCTR(block, iv).XORKeyStream(out, pt)
		return out, nil, true
	}
	return nil, nil, false
}

func parsePriv(p string) any {
	blk, _ := pem.Decode([]byte(p))
	k, err := x509.ParsePKCS8PrivateKey(blk.Bytes)
	must(err)
	return k
}

func parsePub(p string) any {
	blk, _ := pem.Decode([]byte(p))
	k, err := x509.ParsePKIXPublicKey(blk.Bytes)
	must(err)
	return k
}

func main() {
	path := "../../protocol-vectors/tenant-integration-v1.json"
	if len(os.Args) > 1 {
		path = os.Args[1]
	}
	raw, err := os.ReadFile(path)
	must(err)
	var d Doc
	must(json.Unmarshal(raw, &d))
	check(d.TestOnly, "test_only")

	var skipped []string
	nt := 0
	for _, t := range d.Transport {
		if t.MacAlg != nil && macHash(*t.MacAlg) == nil {
			skipped = append(skipped, t.Name)
			continue
		}
		aad := []byte{}
		if t.AADUTF8 != nil {
			aad = []byte(*t.AADUTF8)
		}
		ct, tag, ok := encrypt(t.DataAlg, unhex(t.DataKeyHex), unhex(t.IVHex), aad, []byte(t.Plaintext))
		if !ok {
			skipped = append(skipped, t.Name)
			continue
		}
		check(b64(ct) == t.Expected.CiphertextB64, t.Name+" ciphertext")
		tagB64 := ""
		if tag != nil {
			tagB64 = b64(tag)
		}
		check(tagB64 == t.Expected.TagB64, t.Name+" tag")
		var env envelope
		must(json.Unmarshal([]byte(t.Expected.EnvelopeJSON), &env))
		sig := ""
		if t.MacAlg != nil {
			canon := strings.Join([]string{d.Common.TenantID, d.Common.HTTPPath, env.KeyVersion,
				strconv.FormatInt(env.Timestamp, 10), env.Nonce, env.IV, env.Ciphertext, env.Tag}, "\n")
			check(canon == *t.Expected.MacCanonical, t.Name+" canonical")
			sig = hex.EncodeToString(hmacOf(*t.MacAlg, unhex(*t.MacKeyHex), []byte(canon)))
		}
		check(sig == t.Expected.SignatureHex, t.Name+" signature")
		fmt.Println("  ok transport", t.Name)
		nt++
	}

	nb := 0
	for _, b := range d.Binding {
		canon := strings.Join([]string{b.Method, b.PathAndQuery, strconv.FormatInt(b.Timestamp, 10), b.Nonce, b.Body}, "\n")
		check(canon == b.Expected.Canonical, b.Name+" canonical")
		msg := []byte(canon)
		want, err := base64.StdEncoding.DecodeString(b.Expected.SignatureB64)
		must(err)
		keyName := strings.TrimPrefix(b.Key, "keys.asymmetric.")
		switch {
		case b.Version == "hmac-v1" && macHash(b.Alg) != nil:
			key, err := base64.StdEncoding.DecodeString(d.Keys.HmacBindingKeyB64)
			must(err)
			check(bytes.Equal(hmacOf(b.Alg, key, msg), want), b.Name)
		case b.Version == "rsa-v1": // PKCS#1 v1.5，不是 PSS
			h := map[string]crypto.Hash{"sha256": crypto.SHA256, "sha384": crypto.SHA384, "sha512": crypto.SHA512}[b.Alg[strings.LastIndex(b.Alg, "-")+1:]]
			hh := h.New()
			hh.Write(msg)
			got, err := rsa.SignPKCS1v15(nil, parsePriv(d.Keys.Asymmetric[keyName]["private_key_pem"]).(*rsa.PrivateKey), h, hh.Sum(nil))
			must(err)
			check(bytes.Equal(got, want), b.Name)
		case b.Version == "ed25519-v1":
			got := ed25519.Sign(parsePriv(d.Keys.Asymmetric[keyName]["private_key_pem"]).(ed25519.PrivateKey), msg)
			check(bytes.Equal(got, want), b.Name)
		case b.Version == "ecdsa-v1": // P1363 r||s，随机 k → 只验签
			pub := parsePub(d.Keys.Asymmetric[keyName]["public_key_pem"]).(*ecdsa.PublicKey)
			var digest []byte
			if strings.HasSuffix(b.Alg, "sha256") {
				s := sha256.Sum256(msg)
				digest = s[:]
			} else {
				s := sha512.Sum384(msg)
				digest = s[:]
			}
			half := len(want) / 2
			r, s := new(big.Int).SetBytes(want[:half]), new(big.Int).SetBytes(want[half:])
			check(ecdsa.Verify(pub, digest, r, s), b.Name)
			check(!ecdsa.Verify(pub, digest, r, new(big.Int).Add(s, big.NewInt(1))), b.Name+" tamper")
		default:
			skipped = append(skipped, b.Name)
			continue
		}
		hdr := map[string]string{
			"x-mazbri-timestamp": strconv.FormatInt(b.Timestamp, 10), "x-mazbri-nonce": b.Nonce,
			"x-mazbri-signature": b.Expected.SignatureB64, "x-mazbri-signature-input": b.Alg + "=v1",
			"x-mazbri-key-id": b.Kid,
		}
		for k, v := range hdr {
			check(b.Expected.Headers[k] == v, b.Name+" header "+k)
		}
		check(len(hdr) == len(b.Expected.Headers), b.Name+" header count")
		fmt.Println("  ok binding", b.Name)
		nb++
	}

	// 回调：payload 键按字典序紧凑序列化（Go map 编码天然按键排序）
	var payload map[string]any
	must(json.Unmarshal(d.Callback.Payload, &payload))
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	must(enc.Encode(payload))
	compact := strings.TrimSuffix(buf.String(), "\n")
	signed := strconv.FormatInt(d.Callback.Timestamp, 10) + "." + compact
	cb := hex.EncodeToString(hmacOf("hmac-sha256", []byte(d.Callback.Secret), []byte(signed)))
	check(cb == d.Callback.Expected.Headers["X-Mazbri-Signature"], "callback")
	fmt.Println("  ok callback")

	fmt.Printf("PASS transport=%d/%d binding=%d/%d callback=1 skipped(标准库不支持)=%v\n",
		nt, len(d.Transport), nb, len(d.Binding), skipped)
}
