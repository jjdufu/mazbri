//! Profile 校验与命名套件解析。

use mazbri_crypto::{
    known_suites, resolve_suite, suite_data_alg, suite_mac_alg, AlgorithmId, AlgorithmRegistry,
    CanonStyle, CipherScope, CryptSpec, CryptoError, CryptoOrder, IvSource,
    SignSpec, SlotLocation, SlotSpec, TenantTransportProfile, TransportMode,
    UpstreamCryptoProfile, WireEncoding, WireEncodings, SUITE_AES_256_GCM,
    SUITE_GM_SM4_HMAC_SM3, SUITE_TENANT_CHACHA20,
};

fn registry() -> AlgorithmRegistry {
    AlgorithmRegistry::default_registry()
}

#[test]
fn resolve_known_suites_and_reject_unknown() {
    let def = resolve_suite(SUITE_AES_256_GCM).expect("default");
    assert_eq!(def.data_alg, AlgorithmId::Aes256Gcm);
    assert_eq!(def.mac_alg, None);

    let gm = resolve_suite(SUITE_GM_SM4_HMAC_SM3).expect("gm");
    assert_eq!(gm.data_alg, AlgorithmId::Sm4CbcPkcs7);
    assert_eq!(gm.mac_alg, Some(AlgorithmId::HmacSm3));

    let chacha = resolve_suite(SUITE_TENANT_CHACHA20).expect("chacha");
    assert_eq!(chacha.data_alg, AlgorithmId::Chacha20Poly1305);
    assert_eq!(chacha.mac_alg, None);

    assert_eq!(
        suite_data_alg(SUITE_AES_256_GCM).expect("data"),
        AlgorithmId::Aes256Gcm
    );
    assert_eq!(
        suite_mac_alg(SUITE_GM_SM4_HMAC_SM3).expect("mac"),
        Some(AlgorithmId::HmacSm3)
    );
    assert_eq!(
        suite_mac_alg(SUITE_TENANT_CHACHA20).expect("no mac"),
        None
    );

    let err = resolve_suite("suite/unknown").expect_err("unknown");
    assert!(matches!(
        err,
        CryptoError::UnknownSuite { name } if name == "suite/unknown"
    ));
    let err = suite_data_alg("suite/unknown").expect_err("unknown data");
    assert!(matches!(err, CryptoError::UnknownSuite { .. }));

    assert_eq!(
        known_suites(),
        &[
            SUITE_AES_256_GCM,
            SUITE_GM_SM4_HMAC_SM3,
            SUITE_TENANT_CHACHA20
        ]
    );
}

#[test]
fn tenant_plaintext_and_default_suite_ok() {
    let reg = registry();
    TenantTransportProfile::plaintext()
        .validate(&reg)
        .expect("plaintext");

    let profile =
        TenantTransportProfile::from_named_suite(SUITE_AES_256_GCM, TransportMode::Full)
            .expect("suite");
    profile.validate(&reg).expect("aes-gcm full");

    let gm =
        TenantTransportProfile::from_named_suite(SUITE_GM_SM4_HMAC_SM3, TransportMode::Request)
            .expect("gm suite");
    gm.validate(&reg).expect("gm suite");
}

#[test]
fn tenant_rejects_cbc_without_mac_and_legacy_without_allow() {
    let reg = registry();

    let mut cbc = TenantTransportProfile::plaintext();
    cbc.mode = TransportMode::Request;
    cbc.data_alg = Some(AlgorithmId::Aes256Cbc);
    let err = cbc.validate(&reg).expect_err("cbc needs mac");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("requires mac_alg")));

    cbc.mac_alg = Some(AlgorithmId::HmacSha256);
    cbc.validate(&reg).expect("cbc+hmac");

    let mut legacy = cbc.clone();
    legacy.data_alg = Some(AlgorithmId::Aes128Ecb);
    legacy.mac_alg = Some(AlgorithmId::HmacSha256);
    legacy.allow_insecure = false;
    let err = legacy.validate(&reg).expect_err("legacy");
    assert!(matches!(
        err,
        CryptoError::LegacyNotAllowed {
            id: AlgorithmId::Aes128Ecb
        }
    ));
    legacy.allow_insecure = true;
    legacy.validate(&reg).expect("legacy allowed");
}

#[test]
fn tenant_rejects_mode_none_with_signature_or_algs() {
    let reg = registry();

    let mut p = TenantTransportProfile::plaintext();
    p.require_signature = true;
    let err = p.validate(&reg).expect_err("sig");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("require_signature")));

    let mut p = TenantTransportProfile::plaintext();
    p.data_alg = Some(AlgorithmId::Aes256Gcm);
    let err = p.validate(&reg).expect_err("alg");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("mode=none")));

    let err = TenantTransportProfile::from_named_suite(
        SUITE_AES_256_GCM,
        TransportMode::None,
    )
    .expect_err("none+suite ctor");
    assert!(matches!(err, CryptoError::InvalidProfile { .. }));
}

#[test]
fn tenant_rejects_suite_alg_conflict_and_missing_data_alg() {
    let reg = registry();
    let mut p =
        TenantTransportProfile::from_named_suite(SUITE_AES_256_GCM, TransportMode::Request)
            .expect("suite");
    p.data_alg = Some(AlgorithmId::Chacha20Poly1305);
    let err = p.validate(&reg).expect_err("conflict");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("conflicts")));

    let mut empty = TenantTransportProfile::plaintext();
    empty.mode = TransportMode::Request;
    let err = empty.validate(&reg).expect_err("missing data_alg");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("requires data_alg")));

    let p =
        TenantTransportProfile::from_named_suite(SUITE_TENANT_CHACHA20, TransportMode::Request)
            .expect("chacha");
    p.validate(&reg).expect("chacha ok");
}

#[test]
fn upstream_aead_and_sign_encrypt_orders() {
    let reg = registry();

    let mut aead = UpstreamCryptoProfile::skeleton(CryptoOrder::AeadOnly);
    aead.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Gcm,
        encoding: None,
        fields: Vec::new(),
    });
    aead.validate(&reg).expect("aead");

    let mut ets = UpstreamCryptoProfile::skeleton(CryptoOrder::EncryptThenSign);
    ets.crypt = Some(CryptSpec {
        alg: AlgorithmId::Sm4CbcPkcs7,
        encoding: Some(WireEncoding::Hex {
            case: mazbri_crypto::HexCase::Lower,
        }),
        fields: Vec::new(),
    });
    ets.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSm3,
        encoding: None,
    });
    ets.slots = vec![SlotSpec {
        role: "sign".to_owned(),
        location: SlotLocation::Header,
        name: "X-Sign".to_owned(),
    }];
    ets.canon = CanonStyle::SortedKv;
    ets.encodings = WireEncodings::base64_all();
    ets.validate(&reg).expect("ets sm4+hmac-sm3");
}

#[test]
fn upstream_rejects_encrypt_only_cbc_and_legacy_without_allow() {
    let reg = registry();

    let mut p = UpstreamCryptoProfile::skeleton(CryptoOrder::EncryptOnly);
    p.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Cbc,
        encoding: None,
        fields: Vec::new(),
    });
    let err = p.validate(&reg).expect_err("cbc bare");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("without mac/sign")));

    p.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Gcm,
        encoding: None,
        fields: Vec::new(),
    });
    p.validate(&reg).expect("encrypt_only aead");

    let mut not_aead = UpstreamCryptoProfile::skeleton(CryptoOrder::AeadOnly);
    not_aead.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Cbc,
        encoding: None,
        fields: Vec::new(),
    });
    let err = not_aead.validate(&reg).expect_err("not aead");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("aead")));

    let mut legacy_aead_order = UpstreamCryptoProfile::skeleton(CryptoOrder::AeadOnly);
    legacy_aead_order.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes128Ecb,
        encoding: None,
        fields: Vec::new(),
    });
    let err = legacy_aead_order.validate(&reg).expect_err("legacy ecb");
    assert!(matches!(
        err,
        CryptoError::LegacyNotAllowed {
            id: AlgorithmId::Aes128Ecb
        }
    ));

    let mut legacy = UpstreamCryptoProfile::skeleton(CryptoOrder::EncryptThenSign);
    legacy.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes128Ecb,
        encoding: None,
        fields: Vec::new(),
    });
    legacy.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSha256,
        encoding: None,
    });
    let err = legacy.validate(&reg).expect_err("legacy");
    assert!(matches!(
        err,
        CryptoError::LegacyNotAllowed {
            id: AlgorithmId::Aes128Ecb
        }
    ));
    legacy.allow_insecure = true;
    legacy.validate(&reg).expect("legacy allowed");
}

#[test]
fn upstream_named_fields_and_order_mismatches() {
    let reg = registry();

    let mut p = UpstreamCryptoProfile::skeleton(CryptoOrder::AeadOnly);
    p.scope = CipherScope::NamedFields;
    p.iv = IvSource::RandomPrefix;
    p.crypt = Some(CryptSpec {
        alg: AlgorithmId::Chacha20Poly1305,
        encoding: None,
        fields: Vec::new(),
    });
    let err = p.validate(&reg).expect_err("empty fields");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("crypt.fields")));

    p.crypt.as_mut().expect("crypt").fields = vec!["cardNo".to_owned()];
    p.validate(&reg).expect("named fields");

    let mut sign_only = UpstreamCryptoProfile::skeleton(CryptoOrder::SignOnly);
    sign_only.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSha256,
        encoding: None,
    });
    sign_only.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Gcm,
        encoding: None,
        fields: Vec::new(),
    });
    let err = sign_only.validate(&reg).expect_err("extra crypt");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("must not set crypt")));
}
