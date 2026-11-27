// mazbri 租户接入协议 v1 —— Go 参考示例（仅供测试 / 联调，纯标准库）。
package main

import (
	"bytes"
	"crypto/aes"
	"crypto/cipher"
	"crypto/hmac"
	"crypto/sha256"
	"crypto/sha512"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"hash"
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
		TenantSecretKeyB64 string `json:"tenant_secret_key_b64"`
	} `json:"keys"`
	Transport []struct {
		Name       string  `json:"name"`
		Direction  string  `json:"direction"`
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

func hkdfSha256(salt, ikm, info []byte, length int) []byte {
	prkMac := hmac.New(sha256.New, salt)
	prkMac.Write(ikm)
	prk := prkMac.Sum(nil)
	var out, prev []byte
	for counter := byte(1); len(out) < length; counter++ {
		h := hmac.New(sha256.New, prk)
		h.Write(prev)
		h.Write(info)
		h.Write([]byte{counter})
		prev = h.Sum(nil)
		out = append(out, prev...)
	}
	return out[:length]
}

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

	masterKey, err := base64.StdEncoding.DecodeString(d.Keys.TenantSecretKeyB64)
	must(err)
	var skipped []string
	nt := 0
	for _, t := range d.Transport {
		if t.MacAlg != nil && macHash(*t.MacAlg) == nil {
			skipped = append(skipped, t.Name)
			continue
		}
		dir := t.Direction
		if dir == "" {
			dir = "c2g"
		}
		dataKey := unhex(t.DataKeyHex)
		derivedEnc := hkdfSha256([]byte(d.Common.TenantID), masterKey,
			[]byte("mazbri-transport-v1|"+d.Common.TenantID+"|"+dir+"|enc"), len(dataKey))
		check(bytes.Equal(dataKey, derivedEnc), t.Name+" hkdf enc")

		var env envelope
		must(json.Unmarshal([]byte(t.Expected.EnvelopeJSON), &env))
		macAlgStr := ""
		if t.MacAlg != nil {
			macAlgStr = *t.MacAlg
		}
		expectedAAD := strings.Join([]string{"v1", dir, d.Common.TenantID, d.Common.HTTPPath,
			t.DataAlg, macAlgStr, strconv.FormatInt(env.Timestamp, 10), env.Nonce, env.IV}, "\n")
		aad := []byte{}
		if t.AADUTF8 != nil {
			check(*t.AADUTF8 == expectedAAD, t.Name+" aad")
			aad = []byte(*t.AADUTF8)
		}
		ct, tag, ok := encrypt(t.DataAlg, dataKey, unhex(t.IVHex), aad, []byte(t.Plaintext))
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
		sig := ""
		if t.MacAlg != nil {
			derivedMac := hkdfSha256([]byte(d.Common.TenantID), masterKey,
				[]byte("mazbri-transport-v1|"+d.Common.TenantID+"|"+dir+"|mac"), 32)
			check(bytes.Equal(unhex(*t.MacKeyHex), derivedMac), t.Name+" hkdf mac")
			canon := strings.Join([]string{expectedAAD, env.Ciphertext, env.Tag}, "\n")
			check(canon == *t.Expected.MacCanonical, t.Name+" canonical")
			sig = hex.EncodeToString(hmacOf(*t.MacAlg, derivedMac, []byte(canon)))
		}
		check(sig == t.Expected.SignatureHex, t.Name+" signature")
		fmt.Println("  ok transport", t.Name)
		nt++
	}

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

	fmt.Printf("PASS transport=%d/%d callback=1 skipped(标准库不支持)=%v\n",
		nt, len(d.Transport), skipped)
}
