//! 国密 SM3 / HMAC-SM3 / SM4 / SM2 向量与互通约定。
//!
//! # SM2 密文格式
//!
//! - `libsm` 原生：C1‖C2‖C3（C1 = 65 字节未压缩点，C3 = 32 字节 SM3，C2 = 明文长）
//! - 本库默认对外：[`Sm2CipherMode::C1C3C2`]（Hutool / 新版 `BouncyCastle` 常见默认）
//! - 签名默认 `user_id` = [`SM2_DEFAULT_USER_ID`]（`1234567812345678`）
//!
//! # SM4-GCM
//!
//! 不提供；请用套件 `suite/gm-sm4-hmac-sm3`。

use mazbri_crypto::{
    parse_algorithm_id, suite_data_alg, suite_mac_alg, AsymBox, AsymSign, BlockCipher, Digest,
    GmDigestAlgorithm, GmMacAlgorithm, KeyPurpose, KeyRef, Mac, MacAlgorithm, Sm2CipherMode,
    Sm2EncryptAlgorithm, Sm2SignAlgorithm, Sm4Algorithm, AlgorithmId, AlgorithmRegistry,
    CryptoError, SUITE_GM_SM4_HMAC_SM3, SM2_DEFAULT_USER_ID,
};

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s.replace([' ', '\n', '_'], "")).expect("hex")
}

#[test]
fn sm3_abc_openssl() {
    let digest = GmDigestAlgorithm::Sm3
        .hash(b"abc")
        .expect("sm3");
    assert_eq!(
        digest,
        unhex("66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0")
    );
    let via_registry = AlgorithmRegistry::default_registry()
        .digest(AlgorithmId::Sm3)
        .expect("reg")
        .hash(b"abc")
        .expect("h");
    assert_eq!(digest, via_registry);
}

#[test]
fn hmac_sm3_openssl() {
    // key = ASCII "key", msg = "abc"；OpenSSL: dgst -sm3 -mac HMAC -macopt hexkey:6b6579
    let key = KeyRef::raw(KeyPurpose::Mac, b"key".to_vec());
    let tag = GmMacAlgorithm::HmacSm3.sign(&key, b"abc").expect("sign");
    assert_eq!(
        tag,
        unhex("28e63256e7c5a087b1f073265dc53092163f7b82729735d06f28f10af9d52393")
    );
    MacAlgorithm::HmacSm3
        .verify(&key, b"abc", &tag)
        .expect("verify");
}

#[test]
fn sm4_cbc_ctr_openssl() {
    let key = KeyRef::raw(
        KeyPurpose::DataEncryption,
        unhex("0123456789abcdeffedcba9876543210"),
    );
    let iv = unhex("000102030405060708090a0b0c0d0e0f");
    let pt = b"HelloSM4!";

    let cbc = Sm4Algorithm::Sm4CbcPkcs7
        .encrypt(&key, &iv, pt)
        .expect("cbc enc");
    assert_eq!(cbc, unhex("b3c4f0e74d54d7ee1c26112cee3750c9"));
    assert_eq!(
        Sm4Algorithm::Sm4CbcPkcs7
            .decrypt(&key, &iv, &cbc)
            .expect("cbc dec"),
        pt
    );

    let ctr = Sm4Algorithm::Sm4Ctr
        .encrypt(&key, &iv, pt)
        .expect("ctr enc");
    assert_eq!(ctr, unhex("4efdf00d52f525990b"));
    assert_eq!(
        Sm4Algorithm::Sm4Ctr
            .decrypt(&key, &iv, &ctr)
            .expect("ctr dec"),
        pt
    );
}

#[test]
fn sm2_encrypt_roundtrip_both_modes() {
    use libsm::sm2::signature::SigCtx;

    let ctx = SigCtx::new();
    let (pk, sk) = ctx.new_keypair().expect("keypair");
    let pk_bytes = ctx.serialize_pubkey(&pk, false).expect("pk");
    let sk_bytes = ctx.serialize_seckey(&sk).expect("sk");
    let recipient = KeyRef::raw(KeyPurpose::AsymBox, pk_bytes);
    let private = KeyRef::raw(KeyPurpose::AsymBox, sk_bytes);
    let msg = b"sm2-envelope-payload";

    for mode in [Sm2CipherMode::C1C3C2, Sm2CipherMode::C1C2C3] {
        let alg = Sm2EncryptAlgorithm::with_mode(mode);
        let ct = alg.seal(&recipient, msg).expect("seal");
        // C1(65) + C3(32) + C2(msg) or C1+C2+C3
        assert_eq!(ct.len(), 65 + 32 + msg.len());
        let pt = alg.open(&private, &ct).expect("open");
        assert_eq!(pt, msg);
    }
}

#[test]
fn sm2_sign_verify_default_user_id() {
    use libsm::sm2::signature::SigCtx;

    let ctx = SigCtx::new();
    let (pk, sk) = ctx.new_keypair().expect("keypair");
    let pk_bytes = ctx.serialize_pubkey(&pk, false).expect("pk");
    let sk_bytes = ctx.serialize_seckey(&sk).expect("sk");
    let signer = KeyRef::raw(KeyPurpose::AsymSign, sk_bytes);
    let verifier = KeyRef::raw(KeyPurpose::AsymSign, pk_bytes);
    let alg = Sm2SignAlgorithm::default();
    assert_eq!(alg.user_id, SM2_DEFAULT_USER_ID);

    let msg = b"sign-me-please";
    let sig = alg.sign(&signer, msg).expect("sign");
    alg.verify(&verifier, msg, &sig).expect("verify");
    let err = alg
        .verify(&verifier, b"tampered", &sig)
        .expect_err("tamper");
    assert!(matches!(err, CryptoError::AuthenticationFailed));
}

#[test]
fn suite_gm_sm4_hmac_sm3_mapping() {
    assert_eq!(
        suite_data_alg(SUITE_GM_SM4_HMAC_SM3),
        Some(AlgorithmId::Sm4CbcPkcs7)
    );
    assert_eq!(
        suite_mac_alg(SUITE_GM_SM4_HMAC_SM3),
        Some(AlgorithmId::HmacSm3)
    );
    assert!(require_mac_registered());
}

fn require_mac_registered() -> bool {
    use mazbri_crypto::require_mac_for;
    require_mac_for(AlgorithmId::Sm4CbcPkcs7) && require_mac_for(AlgorithmId::Sm4Ctr)
}

#[test]
fn sm9_is_unsupported() {
    let err = parse_algorithm_id("sm9-encrypt").expect_err("sm9");
    assert!(matches!(err, CryptoError::Unsupported { .. }));
    let err2 = parse_algorithm_id("sm9").expect_err("sm9");
    assert!(matches!(err2, CryptoError::Unsupported { .. }));
}
