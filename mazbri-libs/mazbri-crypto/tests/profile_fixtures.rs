//! step-14：通用 `UpstreamCryptoProfile` fixture 自动加载与校验。
//!
//! 目录：`tests/fixtures/*.json`（kebab-case 文件名；无厂商品牌类型）。

use std::fs;
use std::path::{Path, PathBuf};

use mazbri_crypto::{
    AlgorithmId, AlgorithmRegistry, CryptoError, CryptoOrder, UpstreamCryptoProfile,
};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load_upstream_fixtures() -> Vec<(String, UpstreamCryptoProfile)> {
    let dir = fixtures_dir();
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read fixtures dir {}: {e}", dir.display()))
        .filter_map(std::result::Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    entries
        .into_iter()
        .map(|entry| {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_owned();
            let raw = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
            let profile: UpstreamCryptoProfile = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("parse fixture {name}: {e}\n{raw}"));
            (name, profile)
        })
        .collect()
}

#[test]
fn fixtures_dir_loads_at_least_three_generic_profiles() {
    let loaded = load_upstream_fixtures();
    assert!(
        loaded.len() >= 3,
        "expected ≥3 fixtures under tests/fixtures, got {}",
        loaded.len()
    );

    let names: Vec<&str> = loaded.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.contains(&"aes-gcm-aead-only.json"),
        "missing aes-gcm-aead-only.json in {names:?}"
    );
    assert!(
        names.contains(&"gm-sm4-hmac-sm3-encrypt-then-sign.json"),
        "missing gm fixture in {names:?}"
    );
    assert!(
        names.contains(&"legacy-aes-ecb-hmac-sha256.json"),
        "missing legacy fixture in {names:?}"
    );

    // 禁止厂商品牌文件名混入核心 fixture 目录。
    for name in &names {
        assert!(
            !name.to_ascii_lowercase().contains("upupt"),
            "fixture name must stay generic, got {name}"
        );
    }
}

#[test]
fn every_fixture_validates_against_default_registry() {
    let reg = AlgorithmRegistry::default_registry();
    let loaded = load_upstream_fixtures();
    assert!(
        loaded.len() >= 3,
        "fixtures dir must expose ≥3 profiles, got {}",
        loaded.len()
    );

    for (name, profile) in &loaded {
        profile
            .validate(&reg)
            .unwrap_or_else(|e| panic!("fixture {name} failed validate: {e:?}"));
    }
}

#[test]
fn aes_gcm_aead_only_fixture_shape() {
    let loaded = load_upstream_fixtures();
    let profile = loaded
        .iter()
        .find(|(n, _)| n == "aes-gcm-aead-only.json")
        .map(|(_, p)| p)
        .expect("aes-gcm fixture");
    assert_eq!(profile.order, CryptoOrder::AeadOnly);
    assert_eq!(
        profile.crypt.as_ref().map(|c| c.alg),
        Some(AlgorithmId::Aes256Gcm)
    );
    assert!(profile.sign.is_none());
    assert!(!profile.allow_insecure);
}

#[test]
fn gm_encrypt_then_sign_fixture_shape() {
    let loaded = load_upstream_fixtures();
    let profile = loaded
        .iter()
        .find(|(n, _)| n == "gm-sm4-hmac-sm3-encrypt-then-sign.json")
        .map(|(_, p)| p)
        .expect("gm fixture");
    assert_eq!(profile.order, CryptoOrder::EncryptThenSign);
    assert_eq!(
        profile.crypt.as_ref().map(|c| c.alg),
        Some(AlgorithmId::Sm4CbcPkcs7)
    );
    assert_eq!(
        profile.sign.as_ref().map(|s| s.alg),
        Some(AlgorithmId::HmacSm3)
    );
    assert!(!profile.allow_insecure);
}

#[test]
fn legacy_fixture_requires_allow_insecure_gate() {
    let reg = AlgorithmRegistry::default_registry();
    let loaded = load_upstream_fixtures();
    let mut profile = loaded
        .into_iter()
        .find(|(n, _)| n == "legacy-aes-ecb-hmac-sha256.json")
        .map(|(_, p)| p)
        .expect("legacy fixture");

    assert!(profile.allow_insecure, "legacy sample must set allow_insecure");
    assert_eq!(profile.order, CryptoOrder::SignThenEncrypt);
    assert_eq!(
        profile.crypt.as_ref().map(|c| c.alg),
        Some(AlgorithmId::Aes128Ecb)
    );
    assert_eq!(
        profile.sign.as_ref().map(|s| s.alg),
        Some(AlgorithmId::HmacSha256)
    );
    profile.validate(&reg).expect("legacy with allow_insecure");

    profile.allow_insecure = false;
    let err = profile
        .validate(&reg)
        .expect_err("legacy must be denied without allow_insecure");
    assert!(matches!(
        err,
        CryptoError::LegacyNotAllowed {
            id: AlgorithmId::Aes128Ecb
        }
    ));
}
