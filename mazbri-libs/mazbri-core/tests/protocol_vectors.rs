//! 租户接入协议固定测试向量校验（`docs/protocol-vectors/tenant-integration-v1.json`）。
//!
//! - 默认：读取向量文件，用当前实现重算全部确定性字段并逐一比对；
//!   ECDSA / SM2 等非确定性签名只做「用公钥验签」。文档与代码一旦漂移即失败。
//! - `MAZBRI_REGEN_PROTOCOL_VECTORS=1`：重新生成测试专用非对称密钥并重写向量文件。
//!
//! 向量中的全部密钥均为专用生成的测试密钥，仅供测试 / 联调，严禁用于任何真实租户。

use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use mazbri_core::crypto::request_binding::{
    RequestSigner, SignatureVersion, binding_key_ref, canonical_http_binding, verify_binding,
};
use mazbri_core::crypto::transport::{
    TransportCipher, TransportEnvelope, canonical_signature_payload, data_key_prefix,
    encrypt_with_alg_iv, iv_len_for, open_envelope, sign_payload,
};
use mazbri_crypto::id::{AlgorithmId, AlgorithmKind};
use mazbri_crypto::profile::{KeySelection, TenantTransportProfile, TransportMode};
use mazbri_crypto::registry::AlgorithmRegistry;
use mazbri_crypto::{SUITE_TENANT_DEFAULT_2026_11, generate_sign_keypair, parse_algorithm_id};
use serde_json::{Value, json};

const SCHEMA: &str = "mazbri-tenant-protocol-vectors/v1";
const TENANT_ID: &str = "7f3c2a1e-0d4b-4c8e-9a51-2b6f0e9d1c3a";
const PRODUCT_ID: &str = "0b9e7d52-3c1a-4f60-8e2d-5a4b3c2d1e0f";
const HTTP_PATH: &str = "/api/v1/gateway";
const KEY_VERSION: &str = "v1";
/// 2026-01-01 08:00:00 Asia/Shanghai。
const TIMESTAMP: i64 = 1_767_225_600;
const CALLBACK_SECRET: &str = "test-only-callback-secret-do-not-use";

/// (向量名, 数据算法, MAC 算法, 是否 legacy)
const TRANSPORT_CASES: [(&str, &str, Option<&str>, bool); 17] = [
    ("aes-128-gcm", "aes-128-gcm", None, false),
    ("aes-192-gcm", "aes-192-gcm", None, false),
    ("aes-256-gcm", "aes-256-gcm", None, false),
    (
        "aes-256-gcm+hmac-sha256",
        "aes-256-gcm",
        Some("hmac-sha256"),
        false,
    ),
    ("chacha20-poly1305", "chacha20-poly1305", None, false),
    (
        "aes-128-cbc+hmac-sha256",
        "aes-128-cbc",
        Some("hmac-sha256"),
        false,
    ),
    (
        "aes-256-cbc+hmac-sha512",
        "aes-256-cbc",
        Some("hmac-sha512"),
        false,
    ),
    (
        "aes-128-ctr+hmac-sha384",
        "aes-128-ctr",
        Some("hmac-sha384"),
        false,
    ),
    (
        "aes-256-ctr+hmac-sha256",
        "aes-256-ctr",
        Some("hmac-sha256"),
        false,
    ),
    (
        "aes-128-cfb+hmac-sha256",
        "aes-128-cfb",
        Some("hmac-sha256"),
        false,
    ),
    (
        "aes-256-cfb+hmac-sha256",
        "aes-256-cfb",
        Some("hmac-sha256"),
        false,
    ),
    (
        "sm4-cbc-pkcs7+hmac-sm3",
        "sm4-cbc-pkcs7",
        Some("hmac-sm3"),
        false,
    ),
    ("sm4-ctr+hmac-sm3", "sm4-ctr", Some("hmac-sm3"), false),
    (
        "legacy aes-256-ecb+hmac-md5",
        "aes-256-ecb",
        Some("hmac-md5"),
        true,
    ),
    (
        "legacy aes-128-ecb+hmac-sha256",
        "aes-128-ecb",
        Some("hmac-sha256"),
        true,
    ),
    (
        "legacy sm4-ecb-pkcs7+hmac-md5",
        "sm4-ecb-pkcs7",
        Some("hmac-md5"),
        true,
    ),
    (
        "legacy des-cbc-pkcs5+hmac-sha256",
        "des-cbc-pkcs5",
        Some("hmac-sha256"),
        true,
    ),
];

#[derive(Debug, Clone, Copy)]
enum BodySource {
    Plaintext,
    Envelope,
}

/// (向量名, 算法, kid, 请求体来源)
const BINDING_CASES: [(&str, &str, &str, BodySource); 9] = [
    (
        "hmac-v1 / hmac-sha256（明文 none 模式）",
        "hmac-sha256",
        "test-kid-hmac",
        BodySource::Plaintext,
    ),
    (
        "hmac-v1 / hmac-sha256（加密信封）",
        "hmac-sha256",
        "test-kid-hmac",
        BodySource::Envelope,
    ),
    (
        "hmac-v1 / hmac-sm3",
        "hmac-sm3",
        "test-kid-hmac-sm3",
        BodySource::Envelope,
    ),
    (
        "rsa-v1 / rsa-pkcs1v15-sign-sha256",
        "rsa-pkcs1v15-sign-sha256",
        "test-kid-rsa",
        BodySource::Envelope,
    ),
    (
        "rsa-v1 / rsa-pkcs1v15-sign-sha512",
        "rsa-pkcs1v15-sign-sha512",
        "test-kid-rsa",
        BodySource::Envelope,
    ),
    (
        "ecdsa-v1 / ecdsa-p256-sha256",
        "ecdsa-p256-sha256",
        "test-kid-ecdsa",
        BodySource::Envelope,
    ),
    (
        "ecdsa-v1 / ecdsa-p384-sha384",
        "ecdsa-p384-sha384",
        "test-kid-ecdsa-p384",
        BodySource::Envelope,
    ),
    (
        "ed25519-v1 / ed25519",
        "ed25519",
        "test-kid-ed25519",
        BodySource::Envelope,
    ),
    (
        "sm2-v1 / sm2-sign",
        "sm2-sign",
        "test-kid-sm2",
        BodySource::Envelope,
    ),
];

fn vectors_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/protocol-vectors/tenant-integration-v1.json")
}

/// 测试专用租户密钥（32 字节：0x00..0x1f）。
fn tenant_secret() -> Vec<u8> {
    (0u8..32).collect()
}

/// 测试专用 HMAC 请求密钥（32 字节：0x80..0x9f；即 mgmt `hmacKey` 的 Base64 解码结果）。
fn hmac_binding_key() -> Vec<u8> {
    (0x80u8..0xa0).collect()
}

/// 固定 IV：0xa0, 0xa1, ...（按算法长度截取）。
fn fixed_iv(len: usize) -> Vec<u8> {
    (0xa0u8..=0xff).take(len).collect()
}

/// 固定 nonce：16 字节 seed..seed+15 的 base64url（无填充）。
fn fixed_nonce(seed: u8) -> String {
    let raw: Vec<u8> = (0..16u8).map(|i| seed.wrapping_add(i)).collect();
    URL_SAFE_NO_PAD.encode(raw)
}

fn plaintext_request() -> String {
    format!(
        r#"{{"product_id":"{PRODUCT_ID}","path":"/v1/echo","method":"POST","body":{{"hello":"mazbri","n":1}}}}"#
    )
}

fn profile_for(
    data_alg: AlgorithmId,
    mac_alg: Option<AlgorithmId>,
    legacy: bool,
) -> TenantTransportProfile {
    let mut profile =
        TenantTransportProfile::from_named_suite(SUITE_TENANT_DEFAULT_2026_11, TransportMode::Full)
            .unwrap();
    profile.suite = None;
    profile.data_alg = Some(data_alg);
    profile.mac_alg = mac_alg;
    profile.require_signature = mac_alg.is_some();
    profile.allow_insecure = legacy;
    profile.key = Some(KeySelection::Latest {
        key_id: "tenant-default".to_owned(),
    });
    profile
}

fn build_transport_vector(
    registry: &AlgorithmRegistry,
    seed: u8,
    (name, data_raw, mac_raw, legacy): (&str, &str, Option<&str>, bool),
) -> (Value, TransportEnvelope) {
    let data_alg = parse_algorithm_id(data_raw).unwrap();
    let mac_alg = mac_raw.map(|raw| parse_algorithm_id(raw).unwrap());
    let profile = profile_for(data_alg, mac_alg, legacy);
    profile.validate(registry).unwrap();
    let cipher = TransportCipher::for_profile(registry, &profile);

    let secret = tenant_secret();
    let data_key = data_key_prefix(data_alg, &secret).unwrap();
    let iv = fixed_iv(iv_len_for(data_alg).unwrap());
    let plaintext = plaintext_request();
    let parts = encrypt_with_alg_iv(
        cipher,
        data_alg,
        data_key,
        &iv,
        plaintext.as_bytes(),
        TENANT_ID.as_bytes(),
    )
    .unwrap();
    let mut envelope = TransportEnvelope {
        encrypted: true,
        key_version: KEY_VERSION.to_owned(),
        data_alg: data_alg.as_str().to_owned(),
        mac_alg: mac_alg.map(|mac| mac.as_str().to_owned()),
        timestamp: TIMESTAMP,
        nonce: fixed_nonce(seed),
        iv: parts.iv,
        ciphertext: parts.ciphertext,
        tag: parts.tag,
        signature: String::new(),
    };
    let canonical = mac_alg.map(|mac| {
        let payload = canonical_signature_payload(&envelope, TENANT_ID, HTTP_PATH);
        envelope.signature = sign_payload(cipher, mac, &secret, &payload).unwrap();
        payload
    });
    // 网关侧真实解封路径必须能打开本向量。
    let opened = open_envelope(
        &profile,
        registry,
        &secret,
        Some(&secret),
        TENANT_ID,
        HTTP_PATH,
        &envelope,
    )
    .unwrap();
    assert_eq!(opened, plaintext.as_bytes(), "{name}: open_envelope");

    let aad = if data_alg.kind() == AlgorithmKind::Aead {
        Value::from(TENANT_ID)
    } else {
        Value::Null
    };
    let vector = json!({
        "name": name,
        "data_alg": data_raw,
        "mac_alg": mac_raw,
        "legacy_allow_insecure": legacy,
        "data_key_hex": hex::encode(data_key),
        "mac_key_hex": mac_alg.map(|_| hex::encode(&secret)),
        "iv_hex": hex::encode(&iv),
        "aad_utf8": aad,
        "plaintext_utf8": plaintext,
        "expected": {
            "iv_b64": envelope.iv,
            "ciphertext_b64": envelope.ciphertext,
            "tag_b64": envelope.tag,
            "mac_canonical_string": canonical,
            "signature_hex": envelope.signature,
            "envelope_json": serde_json::to_string(&envelope).unwrap(),
        }
    });
    (vector, envelope)
}

/// 测试专用非对称密钥：(密钥名=算法 id, 私钥 PKCS#8 PEM, 公钥 SPKI PEM)。
struct AsymKeys {
    pairs: Vec<(String, String, String)>,
}

impl AsymKeys {
    fn generate() -> Self {
        let pairs = [
            "rsa-pkcs1v15-sign-sha256",
            "ecdsa-p256-sha256",
            "ecdsa-p384-sha384",
            "ed25519",
            "sm2-sign",
        ]
        .into_iter()
        .map(|alg| {
            let pair = generate_sign_keypair(parse_algorithm_id(alg).unwrap()).unwrap();
            (
                alg.to_owned(),
                pair.private_pem.to_string(),
                pair.public_pem,
            )
        })
        .collect();
        Self { pairs }
    }

    fn from_file(doc: &Value) -> Self {
        let pairs = doc["keys"]["asymmetric"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, pair)| {
                (
                    name.clone(),
                    pair["private_key_pem"].as_str().unwrap().to_owned(),
                    pair["public_key_pem"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        Self { pairs }
    }

    /// RSA 的 sha256 / sha512 共用同一把 RSA 密钥。
    fn key_name(alg: &str) -> &str {
        if alg.starts_with("rsa-") {
            "rsa-pkcs1v15-sign-sha256"
        } else {
            alg
        }
    }

    fn for_alg(&self, alg: &str) -> (&str, &str) {
        let key_name = Self::key_name(alg);
        let (_, private, public) = self
            .pairs
            .iter()
            .find(|(name, _, _)| name == key_name)
            .unwrap();
        (private, public)
    }

    fn to_json(&self) -> Value {
        let map = self
            .pairs
            .iter()
            .map(|(name, private, public)| {
                (
                    name.clone(),
                    json!({ "private_key_pem": private, "public_key_pem": public }),
                )
            })
            .collect();
        Value::Object(map)
    }
}

fn build_binding_vector(
    registry: &AlgorithmRegistry,
    keys: &AsymKeys,
    seed: u8,
    (name, alg_raw, kid, source): (&str, &str, &str, BodySource),
    envelope_body: &str,
) -> Value {
    let alg = parse_algorithm_id(alg_raw).unwrap();
    let version = SignatureVersion::for_alg(alg).unwrap();
    let symmetric = alg.kind() == AlgorithmKind::Mac;
    let body = match source {
        BodySource::Plaintext => plaintext_request(),
        BodySource::Envelope => envelope_body.to_owned(),
    };
    let nonce = fixed_nonce(seed);
    let canonical = canonical_http_binding("POST", HTTP_PATH, TIMESTAMP, &nonce, body.as_bytes());

    let (signer_material, verify_material, key_ref_name) = if symmetric {
        (
            hmac_binding_key(),
            hmac_binding_key(),
            "keys.hmac_binding_key_b64".to_owned(),
        )
    } else {
        let (private, public) = keys.for_alg(alg_raw);
        (
            private.as_bytes().to_vec(),
            public.as_bytes().to_vec(),
            format!("keys.asymmetric.{}", AsymKeys::key_name(alg_raw)),
        )
    };
    let signer = RequestSigner::new(kid, alg, &signer_material).unwrap();
    let headers = signer
        .sign_with("POST", HTTP_PATH, TIMESTAMP, nonce.clone(), body.as_bytes())
        .unwrap();
    let verify_key = binding_key_ref(alg, &verify_material).unwrap();
    verify_binding(registry, alg, &verify_key, &canonical, &headers.signature).unwrap();
    let deterministic = !matches!(
        alg,
        AlgorithmId::EcdsaP256Sha256 | AlgorithmId::EcdsaP384Sha384 | AlgorithmId::Sm2Sign
    );
    let header_map = headers
        .pairs()
        .into_iter()
        .map(|(key, value)| (key.to_owned(), Value::from(value)))
        .collect();
    json!({
        "name": name,
        "signature_version": version.as_str(),
        "alg": alg_raw,
        "kid": kid,
        "key": key_ref_name,
        "method": "POST",
        "path_and_query": HTTP_PATH,
        "timestamp": TIMESTAMP,
        "nonce": nonce,
        "body_utf8": body,
        "deterministic": deterministic,
        "expected": {
            "canonical_string": canonical,
            "signature_b64": headers.signature,
            "headers": Value::Object(header_map),
        }
    })
}

fn callback_vector() -> Value {
    let payload = json!({
        "event": "task.completed",
        "task_id": "00000000-0000-4000-8000-00000000c0de",
        "status": "success",
        "data": {"result": "ok"}
    });
    let compact = serde_json::to_string(&payload).unwrap();
    let message = format!("{TIMESTAMP}.{compact}");
    let registry = AlgorithmRegistry::default_registry();
    let signature = sign_payload(
        TransportCipher::strict(&registry),
        AlgorithmId::HmacSha256,
        CALLBACK_SECRET.as_bytes(),
        &message,
    )
    .unwrap();
    json!({
        "secret_utf8": CALLBACK_SECRET,
        "timestamp": TIMESTAMP,
        "payload": payload,
        "expected": {
            "payload_compact": compact,
            "signed_message": message,
            "headers": {
                "X-Mazbri-Signature": signature,
                "X-Mazbri-Timestamp": TIMESTAMP.to_string(),
            }
        }
    })
}

fn build_document(keys: &AsymKeys) -> Value {
    let registry = AlgorithmRegistry::default_registry();
    let mut transport = Vec::new();
    let mut envelope_body = String::new();
    for (seed, case) in (0u8..=u8::MAX).step_by(15).zip(TRANSPORT_CASES) {
        let (vector, envelope) = build_transport_vector(&registry, seed, case);
        if case.0 == "aes-256-gcm" {
            envelope_body = serde_json::to_string(&envelope).unwrap();
        }
        transport.push(vector);
    }
    assert!(envelope_body.starts_with(r#"{"encrypted":"#), "信封 body 应为信封 JSON");
    let binding: Vec<Value> = (0xc0u8..)
        .zip(BINDING_CASES)
        .map(|(seed, case)| build_binding_vector(&registry, keys, seed, case, &envelope_body))
        .collect();
    json!({
        "schema": SCHEMA,
        "test_only": true,
        "warning": "仅供测试 / 联调：本文件所有密钥均为专用生成的测试密钥，公开可见，严禁用于任何真实租户或生产环境。",
        "generated_by": "mazbri-libs/mazbri-core/tests/protocol_vectors.rs（MAZBRI_REGEN_PROTOCOL_VECTORS=1）",
        "common": {
            "tenant_id": TENANT_ID,
            "http_path": HTTP_PATH,
            "key_version": KEY_VERSION,
            "timestamp": TIMESTAMP,
            "timestamp_note": "2026-01-01 08:00:00 Asia/Shanghai；真实请求须使用当前时间（偏差 ≤ clock_skew_seconds）",
        },
        "keys": {
            "tenant_secret_key_b64": STANDARD.encode(tenant_secret()),
            "hmac_binding_key_b64": STANDARD.encode(hmac_binding_key()),
            "asymmetric": keys.to_json(),
        },
        "transport": transport,
        "binding": binding,
        "callback": callback_vector(),
    })
}

/// 比对时去掉非确定性签名（ECDSA / SM2），这些另行用公钥验签。
fn strip_nondeterministic(doc: &mut Value) {
    for vector in doc["binding"].as_array_mut().unwrap() {
        if vector["deterministic"] == Value::Bool(false) {
            vector["expected"]["signature_b64"] = Value::Null;
            vector["expected"]["headers"]["x-mazbri-signature"] = Value::Null;
        }
    }
}

fn verify_nondeterministic(doc: &Value) {
    let registry = AlgorithmRegistry::default_registry();
    let keys = AsymKeys::from_file(doc);
    for vector in doc["binding"].as_array().unwrap() {
        if vector["deterministic"] == Value::Bool(true) {
            continue;
        }
        let alg_raw = vector["alg"].as_str().unwrap();
        let alg = parse_algorithm_id(alg_raw).unwrap();
        let (_, public) = keys.for_alg(alg_raw);
        let key = binding_key_ref(alg, public.as_bytes()).unwrap();
        let expected = &vector["expected"];
        let signature = expected["signature_b64"].as_str().unwrap();
        assert_eq!(
            expected["headers"]["x-mazbri-signature"].as_str(),
            Some(signature)
        );
        let canonical = expected["canonical_string"].as_str().unwrap();
        assert!(
            verify_binding(&registry, alg, &key, canonical, signature).is_ok(),
            "{}: stored signature must verify",
            vector["name"]
        );
        let tampered = format!("{canonical}x");
        assert!(verify_binding(&registry, alg, &key, &tampered, signature).is_err());
    }
}

#[test]
fn tenant_integration_vectors_match_implementation() {
    let path = vectors_path();
    if std::env::var("MAZBRI_REGEN_PROTOCOL_VECTORS").as_deref() == Ok("1") {
        let doc = build_document(&AsymKeys::generate());
        let mut text = serde_json::to_string_pretty(&doc).unwrap();
        text.push('\n');
        std::fs::write(&path, text).unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(stored["schema"], SCHEMA);
    assert_eq!(stored["test_only"], Value::Bool(true));
    let keys = AsymKeys::from_file(&stored);
    let mut rebuilt = build_document(&keys);
    let mut expected = stored.clone();
    strip_nondeterministic(&mut rebuilt);
    strip_nondeterministic(&mut expected);
    assert_eq!(
        rebuilt, expected,
        "protocol vectors drifted from implementation; regenerate and update docs"
    );
    verify_nondeterministic(&stored);
}
