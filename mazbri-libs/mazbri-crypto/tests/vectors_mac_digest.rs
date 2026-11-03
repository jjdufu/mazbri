//! HMAC / SHA / BLAKE3 已知答案与注册表查询。

use mazbri_crypto::{
    require_mac_for, AlgorithmId, AlgorithmRegistry, Digest, DigestAlgorithm, KeyPurpose, KeyRef,
    Mac, MacAlgorithm, SecurityClass,
};

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s.replace([' ', '\n', '_'], "")).expect("hex")
}

fn mac_key(bytes: &[u8]) -> KeyRef {
    KeyRef::raw(KeyPurpose::Mac, bytes.to_vec())
}

#[test]
fn hmac_sha_rfc4231_case2() {
    let key = mac_key(b"Jefe");
    let msg = b"what do ya want for nothing?";
    let registry = AlgorithmRegistry::default_registry();

    let tag256 = registry
        .mac(AlgorithmId::HmacSha256)
        .expect("mac")
        .sign(&key, msg)
        .expect("sign");
    assert_eq!(
        tag256,
        unhex("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
    );

    let tag384 = MacAlgorithm::HmacSha384.sign(&key, msg).expect("sign");
    assert_eq!(
        tag384,
        unhex(
            "af45d2e376484031617f78d2b58a6b1b\
             9c7ef464f5a01b47e42ec3736322445e\
             8e2240ca5e69e2c78b3239ecfab21649"
        )
    );

    let tag512 = MacAlgorithm::HmacSha512.sign(&key, msg).expect("sign");
    assert_eq!(
        tag512,
        unhex(
            "164b7a7bfcf819e2e395fbe73b56e0a3\
             87bd64222e831fd610270cd7ea250554\
             9758bf75c05a994a6d034f65f8f0e6fd\
             caeab1a34d4a6b4b636e070a38bce737"
        )
    );

    MacAlgorithm::HmacSha256
        .verify(&key, msg, &tag256)
        .expect("verify");
    let err = MacAlgorithm::HmacSha256
        .verify(&key, msg, &tag384[..32])
        .expect_err("bad tag");
    assert!(matches!(
        err,
        mazbri_crypto::CryptoError::AuthenticationFailed
    ));
}

#[test]
fn sha_and_blake3_digests() {
    let registry = AlgorithmRegistry::default_registry();
    assert_eq!(
        registry
            .digest(AlgorithmId::Sha256)
            .expect("d")
            .hash(b"abc")
            .expect("h"),
        unhex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert_eq!(
        DigestAlgorithm::Sha384.hash(b"abc").expect("h"),
        unhex(
            "cb00753f45a35e8bb5a03d699ac65007\
             272c32ab0eded1631a8b605a43ff5bed\
             8086072ba1e7cc2358baeca134c825a7"
        )
    );
    assert_eq!(
        DigestAlgorithm::Sha512.hash(b"abc").expect("h"),
        unhex(
            "ddaf35a193617abacc417349ae204131\
             12e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd\
             454d4423643ce80e2a9ac94fa54ca49f"
        )
    );
    assert_eq!(
        DigestAlgorithm::Blake3.hash(b"").expect("h"),
        unhex("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262")
    );
    assert_eq!(
        DigestAlgorithm::Blake3.hash(b"abc").expect("h"),
        unhex("6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85")
    );

    let meta = registry.get(AlgorithmId::Blake3).expect("meta");
    assert_eq!(meta.security, SecurityClass::Mainstream);
    assert!(!require_mac_for(AlgorithmId::Blake3));
    assert!(!require_mac_for(AlgorithmId::HmacSha256));
}

#[test]
fn suite_tenant_default_maps_aes256_gcm() {
    assert_eq!(
        mazbri_crypto::suite_data_alg(mazbri_crypto::SUITE_TENANT_DEFAULT_2026_11),
        Some(AlgorithmId::Aes256Gcm)
    );
    assert_eq!(mazbri_crypto::suite_data_alg("suite/unknown"), None);
}
