//! 租户接入协议固定测试向量校验（`docs/protocol-vectors/tenant-integration-v1.json`）。
//!
//! - 默认：读取向量文件，用当前实现重算全部确定性字段并逐一比对；文档与代码一旦漂移即失败。
//! - `MAZBRI_REGEN_PROTOCOL_VECTORS=1`：重新生成测试向量并重写向量文件。
//!
//! 向量中的全部密钥均为专用生成的测试密钥，仅供测试 / 联调，严禁用于任何真实租户。

use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use mazbri_core::crypto::transport::{
    EnvelopeDirection, EnvelopeHeaderRef, SealEnvelopeParams, TransportCipher, TransportEnvelope,
    canonical_envelope_aad, canonical_signature_payload, derive_transport_enc_key,
    derive_transport_mac_key, iv_len_for, open_envelope, seal_envelope_deterministic, sign_payload,
};
use mazbri_crypto::id::{AlgorithmId, AlgorithmKind};
use mazbri_crypto::profile::{TenantTransportProfile, TransportMode};
use mazbri_crypto::registry::AlgorithmRegistry;
use mazbri_crypto::{SUITE_AES_256_GCM, parse_algorithm_id};
use serde_json::{Value, json};

const SCHEMA: &str = "mazbri-tenant-protocol-vectors/v1";
const TENANT_ID: &str = "7f3c2a1e-0d4b-4c8e-9a51-2b6f0e9d1c3a";
const PRODUCT_ID: &str = "0b9e7d52-3c1a-4f60-8e2d-5a4b3c2d1e0f";
const HTTP_PATH: &str = "/api/v1/gateway";
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

fn vectors_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/protocol-vectors/tenant-integration-v1.json")
}

/// 测试专用租户密钥（32 字节：0x00..0x1f）。
fn tenant_secret() -> Vec<u8> {
    (0u8..32).collect()
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
        TenantTransportProfile::from_named_suite(SUITE_AES_256_GCM, TransportMode::Full)
            .unwrap();
    profile.suite = None;
    profile.data_alg = Some(data_alg);
    profile.mac_alg = mac_alg;
    profile.require_signature = mac_alg.is_some();
    profile.allow_insecure = legacy;
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

    let secret = tenant_secret();
    let direction = EnvelopeDirection::ClientToGateway;
    let enc_key = derive_transport_enc_key(&secret, TENANT_ID, direction, data_alg).unwrap();
    let mac_key = mac_alg.map(|_| derive_transport_mac_key(&secret, TENANT_ID, direction).unwrap());
    let iv = fixed_iv(iv_len_for(data_alg).unwrap());
    let nonce = fixed_nonce(seed);
    let plaintext = plaintext_request();

    let envelope = seal_envelope_deterministic(
        SealEnvelopeParams {
            profile: &profile,
            registry,
            master_key: &secret,
            direction,
            tenant_id: TENANT_ID,
            http_path: HTTP_PATH,
            plaintext: plaintext.as_bytes(),
            timestamp: TIMESTAMP,
        },
        |_| iv.clone(),
        nonce.clone(),
    )
    .unwrap();

    let aad_string = canonical_envelope_aad(EnvelopeHeaderRef {
        direction,
        tenant_id: TENANT_ID,
        http_path: HTTP_PATH,
        data_alg: data_alg.as_str(),
        mac_alg: mac_alg.map(AlgorithmId::as_str),
        timestamp: TIMESTAMP,
        nonce: &nonce,
        iv_b64: &envelope.iv,
    });
    let canonical = mac_alg.map(|_| {
        canonical_signature_payload(&envelope, direction, TENANT_ID, HTTP_PATH)
    });

    let opened = open_envelope(
        &profile,
        registry,
        &secret,
        EnvelopeDirection::ClientToGateway,
        TENANT_ID,
        HTTP_PATH,
        &envelope,
    )
    .unwrap();
    assert_eq!(opened, plaintext.as_bytes(), "{name}: open_envelope");
    assert!(
        open_envelope(
            &profile,
            registry,
            &secret,
            EnvelopeDirection::GatewayToClient,
            TENANT_ID,
            HTTP_PATH,
            &envelope,
        )
        .is_err(),
        "{name}: reflection must fail"
    );

    let aad = if data_alg.kind() == AlgorithmKind::Aead {
        Value::from(aad_string)
    } else {
        Value::Null
    };
    let vector = json!({
        "name": name,
        "direction": direction.as_str(),
        "data_alg": data_raw,
        "mac_alg": mac_raw,
        "legacy_allow_insecure": legacy,
        "data_key_hex": hex::encode(&*enc_key),
        "mac_key_hex": mac_key.as_ref().map(|k| hex::encode(k.as_slice())),
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

fn build_document() -> Value {
    let registry = AlgorithmRegistry::default_registry();
    let mut transport = Vec::new();
    for (seed, case) in (0u8..=u8::MAX).step_by(15).zip(TRANSPORT_CASES) {
        let (vector, _) = build_transport_vector(&registry, seed, case);
        transport.push(vector);
    }
    json!({
        "schema": SCHEMA,
        "test_only": true,
        "warning": "仅供测试 / 联调：本文件所有密钥均为专用生成的测试密钥，公开可见，严禁用于任何真实租户或生产环境。",
        "generated_by": "mazbri-libs/mazbri-core/tests/protocol_vectors.rs（MAZBRI_REGEN_PROTOCOL_VECTORS=1）",
        "common": {
            "tenant_id": TENANT_ID,
            "http_path": HTTP_PATH,
            "timestamp": TIMESTAMP,
            "timestamp_note": "2026-01-01 08:00:00 Asia/Shanghai；真实请求须使用当前时间（偏差 ≤ clock_skew_seconds）",
        },
        "keys": {
            "tenant_secret_key_b64": STANDARD.encode(tenant_secret()),
        },
        "transport": transport,
        "callback": callback_vector(),
    })
}

#[test]
fn tenant_integration_vectors_match_implementation() {
    let path = vectors_path();
    if std::env::var("MAZBRI_REGEN_PROTOCOL_VECTORS").as_deref() == Ok("1") {
        let doc = build_document();
        let mut text = serde_json::to_string_pretty(&doc).unwrap();
        text.push('\n');
        std::fs::write(&path, text).unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(stored["schema"], SCHEMA);
    assert_eq!(stored["test_only"], Value::Bool(true));
    let rebuilt = build_document();
    assert_eq!(
        rebuilt, stored,
        "protocol vectors drifted from implementation; regenerate and update docs"
    );
}
