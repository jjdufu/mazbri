#!/usr/bin/env python3
"""mazbri 租户接入协议 v1 —— Python 参考示例（仅供测试 / 联调）。

读取 docs/protocol-vectors/tenant-integration-v1.json，用 `cryptography` 独立实现：
- 传输信封：加密 + AAD + 外挂 MAC（canonical string）→ 与向量的 ciphertext / tag / signature 逐字节比对
- 回调签名：HMAC-SHA256("{ts}.{compact json}")

用法：python3 verify_vectors.py [path/to/tenant-integration-v1.json]
"""
from __future__ import annotations

import base64
import hashlib
import hmac
import json
import sys
from pathlib import Path

from cryptography.hazmat.primitives import hashes, padding as sympad
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives.ciphers.aead import AESGCM, ChaCha20Poly1305

try:
    from cryptography.hazmat.decrepit.ciphers.algorithms import TripleDES
except ImportError:  # pragma: no cover
    TripleDES = algorithms.TripleDES  # type: ignore[attr-defined]

VECTORS = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[2] / "protocol-vectors" / "tenant-integration-v1.json"
b64 = lambda b: base64.b64encode(b).decode()  # noqa: E731


def encrypt(alg: str, key: bytes, iv: bytes, aad: bytes, pt: bytes) -> tuple[bytes, bytes]:
    if alg.endswith("-gcm"):
        out = AESGCM(key).encrypt(iv, pt, aad)
        return out[:-16], out[-16:]
    if alg == "chacha20-poly1305":
        out = ChaCha20Poly1305(key).encrypt(iv, pt, aad)
        return out[:-16], out[-16:]
    if alg.startswith("sm4"):
        cipher_alg = algorithms.SM4(key)
    elif alg.startswith("des"):
        cipher_alg = TripleDES(key)
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


def hkdf_sha256(salt: bytes, ikm: bytes, info: bytes, length: int) -> bytes:
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    out, t, counter = bytearray(), b"", 1
    while len(out) < length:
        t = hmac.new(prk, t + info + bytes([counter]), hashlib.sha256).digest()
        out.extend(t)
        counter += 1
    return bytes(out[:length])


def check_transport(doc: dict) -> int:
    c = doc["common"]
    master_key = base64.b64decode(doc["keys"]["tenant_secret_key_b64"])
    n = 0
    for t in doc["transport"]:
        e = t["expected"]
        direction = t.get("direction", "c2g")
        key, iv = bytes.fromhex(t["data_key_hex"]), bytes.fromhex(t["iv_hex"])
        expected_enc = hkdf_sha256(
            c["tenant_id"].encode(),
            master_key,
            f"mazbri-transport-v1|{c['tenant_id']}|{direction}|enc".encode(),
            len(key),
        )
        assert key == expected_enc, (t["name"], "hkdf enc key")
        env = json.loads(e["envelope_json"])
        mac_alg_str = t["mac_alg"] or ""
        expected_aad = "\n".join([
            "v1", direction, c["tenant_id"], c["http_path"],
            t["data_alg"], mac_alg_str, str(env["timestamp"]), env["nonce"], e["iv_b64"],
        ])
        aead = t["data_alg"].endswith("-gcm") or t["data_alg"] == "chacha20-poly1305"
        if aead:
            assert t["aad_utf8"] == expected_aad, (t["name"], "aad")
        ct, tag = encrypt(t["data_alg"], key, iv, (t["aad_utf8"] or "").encode(), t["plaintext_utf8"].encode())
        assert b64(iv) == e["iv_b64"], t["name"]
        assert b64(ct) == e["ciphertext_b64"], (t["name"], "ciphertext")
        assert (b64(tag) if aead else "") == e["tag_b64"], (t["name"], "tag")
        sig = ""
        if t["mac_alg"]:
            expected_mac = hkdf_sha256(
                c["tenant_id"].encode(),
                master_key,
                f"mazbri-transport-v1|{c['tenant_id']}|{direction}|mac".encode(),
                32,
            )
            assert bytes.fromhex(t["mac_key_hex"]) == expected_mac, (t["name"], "hkdf mac key")
            canon = "\n".join([
                expected_aad, env["ciphertext"], env["tag"],
            ])
            assert canon == e["mac_canonical_string"], t["name"]
            sig = mac(t["mac_alg"], expected_mac, canon.encode()).hex()
        assert sig == e["signature_hex"], (t["name"], "signature")
        n += 1
        print(f"  ok transport {t['name']}")
    return n


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
    check_callback(doc)
    print(f"PASS transport={t}/{len(doc['transport'])} callback=1")
    return 0


if __name__ == "__main__":
    sys.exit(main())
