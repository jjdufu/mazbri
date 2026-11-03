//! step-10：五种 `CryptoOrder` 往返 + 国密套件至少一则。

use mazbri_crypto::{
    AlgorithmId, AlgorithmRegistry, CanonStyle, CipherScope, CryptSpec, CryptoOrder, IvSource,
    KeyPurpose, KeyRef, SignSpec, SlotLocation, SlotSpec, UpstreamCryptoProfile, WireEncodings,
};
use mazbri_upstream_runtime::slots::role;
use mazbri_upstream_runtime::{
    default_slots_json_cipher_sign_iv, open_response, seal_request, PipelineContext, PipelineKeys,
};

fn registry() -> AlgorithmRegistry {
    AlgorithmRegistry::default_registry()
}

fn data_key_32() -> KeyRef {
    KeyRef::raw(KeyPurpose::DataEncryption, vec![0x11; 32])
}

fn data_key_16() -> KeyRef {
    KeyRef::raw(KeyPurpose::DataEncryption, vec![0x22; 16])
}

fn mac_key() -> KeyRef {
    KeyRef::raw(KeyPurpose::Mac, b"mac-secret-key-32bytes-pad!!!!!".to_vec())
}

fn slots_sign_header_biz_json() -> Vec<SlotSpec> {
    vec![
        SlotSpec {
            role: role::SIGN.into(),
            location: SlotLocation::Header,
            name: "X-Sign".into(),
        },
        SlotSpec {
            role: role::BIZ.into(),
            location: SlotLocation::Json,
            name: "biz".into(),
        },
    ]
}

fn slots_cipher_sign_iv() -> Vec<SlotSpec> {
    default_slots_json_cipher_sign_iv()
}

fn slots_cipher_iv() -> Vec<SlotSpec> {
    vec![
        SlotSpec {
            role: role::CIPHER.into(),
            location: SlotLocation::Json,
            name: "payload".into(),
        },
        SlotSpec {
            role: role::IV.into(),
            location: SlotLocation::Json,
            name: "iv".into(),
        },
    ]
}

fn slots_cipher_sign_iv_gm() -> Vec<SlotSpec> {
    slots_cipher_sign_iv()
}

#[test]
fn order_sign_only_roundtrip() {
    let reg = registry();
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::SignOnly);
    profile.canon = CanonStyle::Raw;
    profile.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSha256,
        encoding: None,
    });
    profile.slots = slots_sign_header_biz_json();
    profile.validate(&reg).expect("profile");

    let mut keys = PipelineKeys::empty();
    keys.mac = Some(mac_key());
    let ctx = PipelineContext::new(&reg);
    let body = br#"{"hello":"world"}"#;

    let sealed = seal_request(body, &profile, &keys, &ctx).expect("seal");
    assert!(sealed.slots.headers.contains_key("X-Sign"));
    let opened = open_response(&sealed, &profile, &keys, &ctx).expect("open");
    assert_eq!(opened, body);
}

#[test]
fn order_encrypt_only_aead_roundtrip() {
    let reg = registry();
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::EncryptOnly);
    profile.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Gcm,
        encoding: None,
        fields: vec![],
    });
    profile.iv = IvSource::Random;
    profile.slots = slots_cipher_iv();
    profile.validate(&reg).expect("profile");

    let mut keys = PipelineKeys::empty();
    keys.data = Some(data_key_32());
    let ctx = PipelineContext::new(&reg);
    let body = b"encrypt-only-payload";

    let sealed = seal_request(body, &profile, &keys, &ctx).expect("seal");
    assert!(sealed.slots.json.contains_key("payload"));
    assert!(sealed.slots.json.contains_key("iv"));
    let opened = open_response(&sealed, &profile, &keys, &ctx).expect("open");
    assert_eq!(opened, body);
}

#[test]
fn order_aead_only_chacha_random_prefix() {
    let reg = registry();
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::AeadOnly);
    profile.crypt = Some(CryptSpec {
        alg: AlgorithmId::Chacha20Poly1305,
        encoding: None,
        fields: vec![],
    });
    profile.iv = IvSource::RandomPrefix;
    profile.slots = vec![SlotSpec {
        role: role::CIPHER.into(),
        location: SlotLocation::Json,
        name: "payload".into(),
    }];
    profile.validate(&reg).expect("profile");

    let mut keys = PipelineKeys::empty();
    keys.data = Some(data_key_32());
    let ctx = PipelineContext::new(&reg);
    let body = b"aead-only-chacha";

    let sealed = seal_request(body, &profile, &keys, &ctx).expect("seal");
    assert!(!sealed.slots.json.contains_key("iv"));
    let opened = open_response(&sealed, &profile, &keys, &ctx).expect("open");
    assert_eq!(opened, body);
}

#[test]
fn order_encrypt_then_sign_roundtrip() {
    let reg = registry();
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::EncryptThenSign);
    profile.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Gcm,
        encoding: None,
        fields: vec![],
    });
    profile.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSha256,
        encoding: None,
    });
    profile.iv = IvSource::Random;
    profile.slots = slots_cipher_sign_iv();
    profile.validate(&reg).expect("profile");

    let mut keys = PipelineKeys::empty();
    keys.data = Some(data_key_32());
    keys.mac = Some(mac_key());
    let ctx = PipelineContext::new(&reg);
    let body = b"ets-body";

    let sealed = seal_request(body, &profile, &keys, &ctx).expect("seal");
    assert!(sealed.slots.json.contains_key("sign"));
    let opened = open_response(&sealed, &profile, &keys, &ctx).expect("open");
    assert_eq!(opened, body);
}

#[test]
fn order_sign_then_encrypt_roundtrip() {
    let reg = registry();
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::SignThenEncrypt);
    profile.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Gcm,
        encoding: None,
        fields: vec![],
    });
    profile.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSha256,
        encoding: None,
    });
    profile.iv = IvSource::Random;
    profile.slots = slots_cipher_sign_iv();
    profile.validate(&reg).expect("profile");

    let mut keys = PipelineKeys::empty();
    keys.data = Some(data_key_32());
    keys.mac = Some(mac_key());
    let ctx = PipelineContext::new(&reg);
    let body = b"ste-body";

    let sealed = seal_request(body, &profile, &keys, &ctx).expect("seal");
    let opened = open_response(&sealed, &profile, &keys, &ctx).expect("open");
    assert_eq!(opened, body);
}

#[test]
fn gm_sm4_cbc_hmac_sm3_encrypt_then_sign() {
    let reg = registry();
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::EncryptThenSign);
    profile.crypt = Some(CryptSpec {
        alg: AlgorithmId::Sm4CbcPkcs7,
        encoding: None,
        fields: vec![],
    });
    profile.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSm3,
        encoding: None,
    });
    profile.iv = IvSource::Random;
    profile.encodings = WireEncodings::base64_all();
    profile.slots = slots_cipher_sign_iv_gm();
    profile.validate(&reg).expect("gm profile");

    let mut keys = PipelineKeys::empty();
    keys.data = Some(data_key_16());
    keys.mac = Some(mac_key());
    let ctx = PipelineContext::new(&reg);
    let body = b"gm-national-suite-body";

    let sealed = seal_request(body, &profile, &keys, &ctx).expect("seal gm");
    let opened = open_response(&sealed, &profile, &keys, &ctx).expect("open gm");
    assert_eq!(opened, body);
}

#[test]
fn named_fields_aead_roundtrip() {
    let reg = registry();
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::AeadOnly);
    profile.scope = CipherScope::NamedFields;
    profile.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Gcm,
        encoding: None,
        fields: vec!["secret".into(), "token".into()],
    });
    profile.iv = IvSource::RandomPrefix;
    profile.slots = vec![];
    profile.validate(&reg).expect("profile");

    let mut keys = PipelineKeys::empty();
    keys.data = Some(data_key_32());
    let ctx = PipelineContext::new(&reg);
    let body = br#"{"secret":"s3cr3t","token":"tok","plain":"ok"}"#;

    let sealed = seal_request(body, &profile, &keys, &ctx).expect("seal");
    let opened = open_response(&sealed, &profile, &keys, &ctx).expect("open");
    let got: serde_json::Value = serde_json::from_slice(&opened).unwrap();
    assert_eq!(got["secret"], "s3cr3t");
    assert_eq!(got["token"], "tok");
    assert_eq!(got["plain"], "ok");
}

#[test]
fn rejects_legacy_without_allow_insecure() {
    let reg = registry();
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::SignThenEncrypt);
    profile.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes128Ecb,
        encoding: None,
        fields: vec![],
    });
    profile.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSha256,
        encoding: None,
    });
    profile.allow_insecure = false;
    profile.slots = slots_cipher_sign_iv();
    let err = profile.validate(&reg).expect_err("legacy");
    let _ = err;

    profile.allow_insecure = true;
    profile.validate(&reg).expect("allowed");

    let mut keys = PipelineKeys::empty();
    keys.data = Some(KeyRef::raw(KeyPurpose::DataEncryption, vec![0x33; 16]));
    keys.mac = Some(mac_key());
    // ECB：iv 长度 0，Random 亦可。
    profile.iv = IvSource::Random;
    let ctx = PipelineContext::new(&reg);
    let body = b"legacy-ecb";
    let sealed = seal_request(body, &profile, &keys, &ctx).expect("seal legacy");
    let opened = open_response(&sealed, &profile, &keys, &ctx).expect("open legacy");
    assert_eq!(opened, body);
}

#[test]
fn slot_conflict_on_duplicate_write() {
    use mazbri_upstream_runtime::slots::SlotBags;
    let mut bags = SlotBags::new();
    bags.write(SlotLocation::Header, "X", "1".into())
        .expect("first");
    let err = bags
        .write(SlotLocation::Header, "X", "2".into())
        .expect_err("conflict");
    assert!(matches!(
        err,
        mazbri_upstream_runtime::SlotError::Conflict { .. }
    ));
}
