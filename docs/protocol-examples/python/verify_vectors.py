#!/usr/bin/env python3
"""mazbri 租户接入协议 v1 —— Python 参考示例（仅供测试 / 联调）。

读取 docs/protocol-vectors/tenant-integration-v1.json，用 `cryptography` 独立实现：
- 传输信封：加密 + AAD + 外挂 MAC（canonical string）→ 与向量的 ciphertext / tag / signature 逐字节比对
- 请求绑定：hmac-v1、rsa-v1（PKCS#1 v1.5，确定性）、ed25519-v1 → 签名比对；ecdsa-v1 → 验签（非确定性）
- 回调签名：HMAC-SHA256("{ts}.{compact json}")

依赖：pip install cryptography（>= 42）。
sm2-v1 需要 SM2 签名实现（`cryptography` 不支持），本示例跳过；可用 gmssl 等国密库按同一 canonical string 验签。

用法：python3 verify_vectors.py [path/to/tenant-integration-v1.json]
"""
from __future__ import annotations

import base64
import hashlib
import hmac
import json
import sys
from pathlib import Path

from cryptography.hazmat.primitives import hashes, padding as sympad, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519, padding, rsa
from cryptography.hazmat.primitives.asymmetric.utils import encode_dss_signature
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives.ciphers.aead import AESGCM, ChaCha20Poly1305

try:  # cryptography >= 43 把 DES/TripleDES 挪到 decrepit
    from cryptography.hazmat.decrepit.ciphers.algorithms import TripleDES
except ImportError:  # pragma: no cover
    TripleDES = algorithms.TripleDES  # type: ignore[attr-defined]

VECTORS = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[2] / "protocol-vectors" / "tenant-integration-v1.json"
b64 = lambda b: base64.b64encode(b).decode()  # noqa: E731  标准 base64（带 padding）


def b64url_nopad(b: bytes) -> str:
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()


# ---------------------------------------------------------------- 传输信封
def encrypt(alg: str, key: bytes, iv: bytes, aad: bytes, pt: bytes) -> tuple[bytes, bytes]:
    """返回 (ciphertext 不含 tag, tag)；非 AEAD 的 tag 为空。"""
    if alg.endswith("-gcm"):
        out = AESGCM(key).encrypt(iv, pt, aad)
        return out[:-16], out[-16:]
    if alg == "chacha20-poly1305":
        out = ChaCha20Poly1305(key).encrypt(iv, pt, aad)
        return out[:-16], out[-16:]
    if alg.startswith("sm4"):
        cipher_alg = algorithms.SM4(key)
    elif alg.startswith("des"):
        cipher_alg = TripleDES(key)  # 8 字节密钥 = 单 DES
    else:
        cipher_alg = algorithms.AES(key)
    if "-cbc" in alg or "-ecb" in alg:
        p = sympad.PKCS7(cipher_alg.block_size).padder()
        pt = p.update(pt) + p.finalize()
        mode = modes.CBC(iv) if "-cbc" in alg else modes.ECB()
    elif alg.endswith("-ctr"):
        mode = modes.CTR(iv)
    elif alg.endswith("-cfb"):
        mode = modes.CFB(iv)
    else:
        raise ValueError(f"unknown data_alg {alg}")
    enc = Cipher(cipher_alg, mode).encryptor()
    return enc.update(pt) + enc.finalize(), b""


HASHES = {"hmac-sha256": hashlib.sha256, "hmac-sha384": hashlib.sha384, "hmac-sha512": hashlib.sha512,
          "hmac-md5": hashlib.md5}


def mac(alg: str, key: bytes, msg: bytes) -> bytes:
    if alg == "hmac-sm3":
        h = __import__("cryptography.hazmat.primitives.hmac", fromlist=["HMAC"]).HMAC(key, hashes.SM3())
        h.update(msg)
        return h.finalize()
    return hmac.new(key, msg, HASHES[alg]).digest()


def check_transport(doc: dict) -> int:
    c = doc["common"]
    n = 0
    for t in doc["transport"]:
        e = t["expected"]
        key, iv = bytes.fromhex(t["data_key_hex"]), bytes.fromhex(t["iv_hex"])
        aead = t["data_alg"].endswith("-gcm") or t["data_alg"] == "chacha20-poly1305"
        ct, tag = encrypt(t["data_alg"], key, iv, (t["aad_utf8"] or "").encode(), t["plaintext_utf8"].encode())
        assert b64(iv) == e["iv_b64"], t["name"]
        assert b64(ct) == e["ciphertext_b64"], (t["name"], "ciphertext")
        assert (b64(tag) if aead else "") == e["tag_b64"], (t["name"], "tag")
        env = json.loads(e["envelope_json"])
        sig = ""
        if t["mac_alg"]:
            canon = "\n".join([c["tenant_id"], c["http_path"], env["keyVersion"], str(env["timestamp"]),
                               env["nonce"], env["iv"], env["ciphertext"], env["tag"]])
            assert canon == e["mac_canonical_string"], t["name"]
            sig = mac(t["mac_alg"], bytes.fromhex(t["mac_key_hex"]), canon.encode()).hex()
        assert sig == e["signature_hex"], (t["name"], "signature")
        n += 1
        print(f"  ok transport {t['name']}")
    return n


# ---------------------------------------------------------------- 请求绑定
def resolve_key(doc: dict, ref: str):
    node = doc
    for part in ref.split(".")[:2]:
        node = node[part]
    rest = ref.split(".", 2)[2:] if ref.count(".") >= 2 else []
    return node[rest[0]] if rest else node


def load_ed25519(pem: str) -> ed25519.Ed25519PrivateKey:
    """mazbri 生成的 Ed25519 私钥是 PKCS#8 v2（RFC 5958，带 [1] publicKey）。

    `cryptography` 只解析 v1 PrivateKeyInfo；失败时从 DER 取出 32 字节种子
    （OCTET STRING 内层的 04 20 <seed>）。也可先 `openssl pkey -in k.pem -out k1.pem` 转成 v1。
    """
    try:
        key = serialization.load_pem_private_key(pem.encode(), None)
        assert isinstance(key, ed25519.Ed25519PrivateKey)
        return key
    except ValueError:
        der = base64.b64decode("".join(l for l in pem.splitlines() if not l.startswith("-----")))
        i = der.index(b"\x04\x22\x04\x20")
        return ed25519.Ed25519PrivateKey.from_private_bytes(der[i + 4:i + 36])


def check_binding(doc: dict) -> tuple[int, list[str]]:
    n, skipped = 0, []
    for b in doc["binding"]:
        e = b["expected"]
        canon = "\n".join([b["method"], b["path_and_query"], str(b["timestamp"]), b["nonce"], b["body_utf8"]])
        assert canon == e["canonical_string"], b["name"]
        msg = canon.encode()
        sv, alg = b["signature_version"], b["alg"]
        sig_expected = base64.b64decode(e["signature_b64"])
        if sv == "hmac-v1":
            key = base64.b64decode(resolve_key(doc, b["key"]))
            assert mac(alg, key, msg) == sig_expected, b["name"]
        elif sv == "rsa-v1":  # PKCS#1 v1.5（不是 PSS），确定性
            priv = serialization.load_pem_private_key(resolve_key(doc, b["key"])["private_key_pem"].encode(), None)
            assert isinstance(priv, rsa.RSAPrivateKey)
            h = {"sha256": hashes.SHA256(), "sha384": hashes.SHA384(), "sha512": hashes.SHA512()}[alg.rsplit("-", 1)[1]]
            assert priv.sign(msg, padding.PKCS1v15(), h) == sig_expected, b["name"]
        elif sv == "ed25519-v1":
            priv = load_ed25519(resolve_key(doc, b["key"])["private_key_pem"])
            assert priv.sign(msg) == sig_expected, b["name"]
            pub = serialization.load_pem_public_key(resolve_key(doc, b["key"])["public_key_pem"].encode())
            assert isinstance(pub, ed25519.Ed25519PublicKey)
            pub.verify(sig_expected, msg)
        elif sv == "ecdsa-v1":  # P1363 r||s，非确定性 → 只验签
            pub = serialization.load_pem_public_key(resolve_key(doc, b["key"])["public_key_pem"].encode())
            assert isinstance(pub, ec.EllipticCurvePublicKey)
            half = len(sig_expected) // 2
            der = encode_dss_signature(int.from_bytes(sig_expected[:half], "big"), int.from_bytes(sig_expected[half:], "big"))
            h = hashes.SHA256() if alg.endswith("sha256") else hashes.SHA384()
            pub.verify(der, msg, ec.ECDSA(h))
        else:
            skipped.append(b["name"])
            print(f"  skip binding {b['name']}（cryptography 无 SM2 签名）")
            continue
        expect_headers = {"x-mazbri-timestamp": str(b["timestamp"]), "x-mazbri-nonce": b["nonce"],
                          "x-mazbri-signature": e["signature_b64"], "x-mazbri-signature-input": f"{alg}=v1",
                          "x-mazbri-key-id": b["kid"]}
        assert expect_headers == e["headers"], b["name"]
        n += 1
        print(f"  ok binding {b['name']}")
    return n, skipped


def check_callback(doc: dict) -> None:
    cb = doc["callback"]
    compact = json.dumps(cb["payload"], separators=(",", ":"), sort_keys=True, ensure_ascii=False)
    assert compact == cb["expected"]["payload_compact"]
    signed = f"{cb['timestamp']}.{compact}"
    sig = hmac.new(cb["secret_utf8"].encode(), signed.encode(), hashlib.sha256).hexdigest()
    assert sig == cb["expected"]["headers"]["X-Mazbri-Signature"]
    print("  ok callback")


def main() -> int:
    doc = json.loads(VECTORS.read_text())
    assert doc["test_only"] is True
    t = check_transport(doc)
    b, skipped = check_binding(doc)
    check_callback(doc)
    print(f"PASS transport={t}/{len(doc['transport'])} binding={b}/{len(doc['binding'])} callback=1 skipped={skipped}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
