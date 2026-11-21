//! 依赖升级回归：`tests/interop/upgrade_golden.json` 由升级前的旧依赖
//! （hmac 0.12、sha2 0.10、sm3 0.4、md-5 0.10、rsa 0.9、p256/p384 0.13、
//! cipher 0.4、aes 0.8、sm4 0.5、des 0.8、aes-gcm 0.10、chacha20poly1305 0.10）生成，
//! 这里断言新依赖算出的 MAC / 摘要 / 确定性签名 / 对称与 AEAD 密文与旧值逐字节一致，
//! 旧签名、旧密文仍能验签 / 解密，保证线上格式不变。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mazbri_crypto::{
    AsymBox, AsymBoxAlgorithm, Digest, DigestAlgorithm, GmDigestAlgorithm, GmMacAlgorithm,
    KeyPurpose, KeyRef, Mac, MacAlgorithm, asym_signer, key_ref_from_text, legacy_algs,
    validate_sign_public_key,
};
use serde_json::Value;

fn fixture() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/interop/upgrade_golden.json"
    ))
    .expect("fixture");
    serde_json::from_str(&raw).expect("json")
}

fn b64(v: &Value, field: &str) -> Vec<u8> {
    STANDARD
        .decode(v[field].as_str().unwrap_or_else(|| panic!("{field}")))
        .unwrap_or_else(|e| panic!("{field} b64: {e}"))
}

fn text<'a>(v: &'a Value, field: &str) -> &'a str {
    v[field].as_str().unwrap_or_else(|| panic!("{field}"))
}

fn sign_mac(alg: &str, key: &KeyRef, msg: &[u8]) -> Vec<u8> {
    match alg {
        "hmac-sha256" => MacAlgorithm::HmacSha256.sign(key, msg),
        "hmac-sha384" => MacAlgorithm::HmacSha384.sign(key, msg),
        "hmac-sha512" => MacAlgorithm::HmacSha512.sign(key, msg),
        "hmac-sm3" => MacAlgorithm::HmacSm3.sign(key, msg),
        "gm-hmac-sm3" => GmMacAlgorithm::HmacSm3.sign(key, msg),
        "hmac-md5" => legacy_algs::hmac_md5_sign(true, key, msg),
        other => panic!("unknown mac {other}"),
    }
    .unwrap_or_else(|e| panic!("{alg}: {e}"))
}

#[test]
fn mac_tags_match_pre_upgrade_golden() {
    let f = fixture();
    let msg = b64(&f, "message_b64");
    let cases = f["mac"].as_array().expect("mac");
    assert_eq!(cases.len(), 18);
    for case in cases {
        let alg = text(case, "alg");
        let key = KeyRef::raw(KeyPurpose::Mac, b64(case, "key_b64"));
        let expected = b64(case, "tag_b64");
        assert_eq!(sign_mac(alg, &key, &msg), expected, "{alg}");
        match alg {
            "hmac-sha256" => MacAlgorithm::HmacSha256.verify(&key, &msg, &expected),
            "hmac-sha384" => MacAlgorithm::HmacSha384.verify(&key, &msg, &expected),
            "hmac-sha512" => MacAlgorithm::HmacSha512.verify(&key, &msg, &expected),
            "hmac-sm3" => MacAlgorithm::HmacSm3.verify(&key, &msg, &expected),
            "gm-hmac-sm3" => GmMacAlgorithm::HmacSm3.verify(&key, &msg, &expected),
            _ => legacy_algs::hmac_md5_verify(true, &key, &msg, &expected),
        }
        .unwrap_or_else(|e| panic!("{alg} verify: {e}"));
    }
}

#[test]
fn digests_match_pre_upgrade_golden() {
    let f = fixture();
    let msg = b64(&f, "message_b64");
    let cases = f["digest"].as_array().expect("digest");
    assert_eq!(cases.len(), 7);
    for case in cases {
        let alg = text(case, "alg");
        let got = match alg {
            "sha-256" => DigestAlgorithm::Sha256.hash(&msg),
            "sha-384" => DigestAlgorithm::Sha384.hash(&msg),
            "sha-512" => DigestAlgorithm::Sha512.hash(&msg),
            "sm3" => DigestAlgorithm::Sm3.hash(&msg),
            "blake3" => DigestAlgorithm::Blake3.hash(&msg),
            "gm-sm3" => GmDigestAlgorithm::Sm3.hash(&msg),
            "md5" => legacy_algs::md5_hash(true, &msg).map(|d| d.to_vec()),
            other => panic!("unknown digest {other}"),
        }
        .unwrap_or_else(|e| panic!("{alg}: {e}"));
        assert_eq!(got, b64(case, "digest_b64"), "{alg}");
    }
}

#[test]
fn deterministic_signatures_match_and_old_signatures_verify() {
    let f = fixture();
    let msg = b64(&f, "message_b64");
    let cases = f["sign"].as_array().expect("sign");
    assert_eq!(cases.len(), 6);
    for case in cases {
        let alg = text(case, "alg").parse().expect("alg id");
        let private =
            key_ref_from_text(KeyPurpose::AsymSign, text(case, "private_pem")).expect("sk");
        let public_pem = text(case, "public_pem");
        let public = key_ref_from_text(KeyPurpose::AsymSign, public_pem).expect("pk");
        let old_sig = b64(case, "signature_b64");
        validate_sign_public_key(alg, public_pem).unwrap_or_else(|e| panic!("{alg} pub: {e}"));
        let signer = asym_signer(alg).expect("signer");
        signer
            .verify(&public, &msg, &old_sig)
            .unwrap_or_else(|e| panic!("{alg} old signature: {e}"));
        assert!(
            signer.verify(&public, b"tampered", &old_sig).is_err(),
            "{alg}"
        );
        let new_sig = signer.sign(&private, &msg).expect("sign");
        assert_eq!(new_sig, old_sig, "{alg} deterministic signature changed");
    }
}

#[test]
fn old_rsa_ciphertexts_still_decrypt() {
    let f = fixture();
    let msg = b64(&f, "message_b64");
    for case in f["asym_box"].as_array().expect("asym_box") {
        let alg = match text(case, "alg") {
            "rsa-oaep-sha256" => AsymBoxAlgorithm::RsaOaepSha256,
            "rsa-pkcs1v15-encrypt" => AsymBoxAlgorithm::RsaPkcs1v15Encrypt,
            other => panic!("unknown box {other}"),
        };
        let private = KeyRef::pem(KeyPurpose::AsymBox, text(case, "private_pem"));
        let public = KeyRef::pem(KeyPurpose::AsymBox, text(case, "public_pem"));
        assert_eq!(
            alg.open(&private, &b64(case, "ciphertext_b64"))
                .expect("open old"),
            msg
        );
        let fresh = alg.seal(&public, &msg).expect("seal");
        assert_eq!(alg.open(&private, &fresh).expect("open new"), msg);
    }

    let pe = &f["rsa_private_encrypt"];
    let private = KeyRef::pem(KeyPurpose::AsymBox, text(pe, "private_pem"));
    let public = KeyRef::pem(KeyPurpose::AsymBox, text(pe, "public_pem"));
    let plaintext = b64(pe, "plaintext_b64");
    let old_ct = b64(pe, "ciphertext_b64");
    assert_eq!(
        legacy_algs::rsa_pkcs1v15_private_encrypt(true, &private, &plaintext).expect("pe"),
        old_ct
    );
    assert_eq!(
        legacy_algs::rsa_pkcs1v15_public_decrypt(true, &public, &old_ct).expect("pd"),
        plaintext
    );
}

#[test]
fn symmetric_and_aead_ciphertexts_match_pre_upgrade_golden() {
    use mazbri_crypto::{
        AeadAlgorithm, AeadSeal, BlockCipher, CfbParams, Sm4Algorithm, SymmetricAlgorithm,
        decrypt_cfb, encrypt_cfb, sm,
    };

    let f = fixture();
    let aead_cases = f["aead"].as_array().expect("aead");
    assert_eq!(aead_cases.len(), 24);
    for case in aead_cases {
        let alg_str = text(case, "alg");
        let alg = AeadAlgorithm::from_id(alg_str.parse().expect("id")).expect("aead alg");
        let kr = KeyRef::raw(KeyPurpose::DataEncryption, b64(case, "key_b64"));
        let nonce = b64(case, "nonce_b64");
        let aad = b64(case, "aad_b64");
        let pt = b64(case, "plaintext_b64");
        let expected_ct = b64(case, "ciphertext_b64");
        assert_eq!(alg.seal(&kr, &nonce, &aad, &pt).expect("seal"), expected_ct, "{alg_str}");
        assert_eq!(alg.open(&kr, &nonce, &aad, &expected_ct).expect("open"), pt, "{alg_str}");
    }

    let sym_cases = f["symmetric"].as_array().expect("symmetric");
    assert_eq!(sym_cases.len(), 42);
    for case in sym_cases {
        let alg_str = text(case, "alg");
        let case_name = text(case, "case");
        let kr = KeyRef::raw(KeyPurpose::DataEncryption, b64(case, "key_b64"));
        let iv = b64(case, "iv_b64");
        let pt = b64(case, "plaintext_b64");
        let expected_ct = b64(case, "ciphertext_b64");
        let (encrypted, decrypted) = match alg_str {
            "aes-128-cfb8" => (
                encrypt_cfb(SymmetricAlgorithm::Aes128Cfb, &kr, &iv, &pt, CfbParams::eight_bit()).unwrap(),
                decrypt_cfb(SymmetricAlgorithm::Aes128Cfb, &kr, &iv, &expected_ct, CfbParams::eight_bit()).unwrap(),
            ),
            "aes-256-cfb8" => (
                encrypt_cfb(SymmetricAlgorithm::Aes256Cfb, &kr, &iv, &pt, CfbParams::eight_bit()).unwrap(),
                decrypt_cfb(SymmetricAlgorithm::Aes256Cfb, &kr, &iv, &expected_ct, CfbParams::eight_bit()).unwrap(),
            ),
            "sm4-cbc-pkcs7" | "sm4-ctr" => {
                let alg = Sm4Algorithm::from_id(alg_str.parse().unwrap()).unwrap();
                (alg.encrypt(&kr, &iv, &pt).unwrap(), alg.decrypt(&kr, &iv, &expected_ct).unwrap())
            }
            "aes-128-ecb" => (
                legacy_algs::aes128_ecb_encrypt(true, &kr, &pt).unwrap(),
                legacy_algs::aes128_ecb_decrypt(true, &kr, &expected_ct).unwrap(),
            ),
            "aes-256-ecb" => (
                legacy_algs::aes256_ecb_encrypt(true, &kr, &pt).unwrap(),
                legacy_algs::aes256_ecb_decrypt(true, &kr, &expected_ct).unwrap(),
            ),
            "sm4-ecb-pkcs7" => (
                sm::sm4_ecb_encrypt(true, &kr, &pt).unwrap(),
                sm::sm4_ecb_decrypt(true, &kr, &expected_ct).unwrap(),
            ),
            "des-cbc-pkcs5" => (
                legacy_algs::des_cbc_encrypt(true, &kr, &iv, &pt).unwrap(),
                legacy_algs::des_cbc_decrypt(true, &kr, &iv, &expected_ct).unwrap(),
            ),
            other => {
                let alg = SymmetricAlgorithm::from_id(other.parse().unwrap()).unwrap();
                (alg.encrypt(&kr, &iv, &pt).unwrap(), alg.decrypt(&kr, &iv, &expected_ct).unwrap())
            }
        };
        assert_eq!(encrypted, expected_ct, "{alg_str}/{case_name} encrypt mismatch");
        assert_eq!(decrypted, pt, "{alg_str}/{case_name} decrypt mismatch");
    }
}
