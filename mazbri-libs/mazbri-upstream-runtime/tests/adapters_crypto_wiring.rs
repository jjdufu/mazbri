//! step-11：`http_envelope` / `sign_form` 经 `crypto_pipeline`；密钥注入。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mazbri_core::channel_egress::{ADAPTER_HTTP_ENVELOPE, ADAPTER_SIGN_FORM};
use mazbri_core::{BodyEncoding, ChannelEgressSpec, EgressRequestSpec, EgressResponseSpec, WRAP_METHOD_BIZ_CONTENT};
use mazbri_crypto::{
    AlgorithmId, AlgorithmRegistry, CanonStyle, CipherScope, CryptSpec, CryptoOrder, IvSource,
    KeyPurpose, KeyRef, Mac, SignSpec, SlotLocation, SlotSpec, UpstreamCryptoProfile, WireEncodings,
};
use mazbri_upstream_runtime::slots::role;
use mazbri_upstream_runtime::{
    default_slots_json_cipher_sign_iv, pipeline_keys_from_credentials, seal_request, ExecuteMode,
    ExecuteOptions, InvokeInput, PipelineContext, PipelineKeys, RuntimeEngine, SsrfChecker,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use base64::Engine as _;

fn registry() -> AlgorithmRegistry {
    AlgorithmRegistry::default_registry()
}

fn mac_key(secret: &str) -> KeyRef {
    KeyRef::raw(KeyPurpose::Mac, secret.as_bytes().to_vec())
}

fn data_key_32() -> KeyRef {
    KeyRef::raw(KeyPurpose::DataEncryption, vec![0x42; 32])
}

async fn spawn_capture_server() -> (String, tokio::sync::oneshot::Receiver<Vec<u8>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<Vec<u8>>();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = Vec::with_capacity(65536);
        let mut tmp = [0u8; 8192];
        loop {
            let n = sock.read(&mut tmp).await.unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let hdr_end = pos + 4;
                let headers = String::from_utf8_lossy(&buf[..hdr_end]);
                let cl = headers
                    .lines()
                    .find_map(|line| {
                        let lower = line.trim().trim_end_matches('\r').to_ascii_lowercase();
                        lower
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().trim_end_matches('\r').parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if buf.len() >= hdr_end + cl {
                    break;
                }
            }
        }
        let body = br#"{"ok":true,"v":1}"#;
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        let _ = sock.write_all(body).await;
        let _ = tx.send(buf);
    });
    (format!("http://{addr}"), rx)
}

fn engine() -> RuntimeEngine {
    let client = Arc::new(
        reqwest::Client::builder()
            .no_proxy()
            .http1_only()
            .build()
            .unwrap(),
    );
    RuntimeEngine::new(client, SsrfChecker::with_allow_private(true))
}

#[test]
fn credentials_inject_mac_and_data_keys() {
    let mut creds = HashMap::new();
    creds.insert("client_secret".into(), "bypass-secret".into());
    creds.insert(
        "crypto_data_key".into(),
        hex::encode([0x11u8; 32]),
    );
    let keys = pipeline_keys_from_credentials(&creds);
    assert_eq!(
        keys.mac.as_ref().unwrap().raw_bytes().unwrap(),
        b"bypass-secret"
    );
    assert_eq!(keys.data.as_ref().unwrap().raw_bytes().unwrap().len(), 32);
}

#[tokio::test]
async fn http_envelope_aead_seal_via_pipeline() {
    let (base, rx) = spawn_capture_server().await;
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::AeadOnly);
    profile.crypt = Some(CryptSpec {
        alg: AlgorithmId::Aes256Gcm,
        encoding: None,
        fields: vec![],
    });
    profile.iv = IvSource::Random;
    profile.slots = default_slots_json_cipher_sign_iv()
        .into_iter()
        .filter(|s| s.role != role::SIGN)
        .collect();
    profile.validate(&registry()).unwrap();

    let mut keys = PipelineKeys::empty();
    keys.data = Some(data_key_32());

    let spec = ChannelEgressSpec {
        adapter: ADAPTER_HTTP_ENVELOPE.into(),
        base_url: base,
        operation: Some("demo.op".into()),
        request: EgressRequestSpec {
            method: "POST".into(),
            path: "/env".into(),
            body_encoding: BodyEncoding::Json,
            wrap: Some(WRAP_METHOD_BIZ_CONTENT.into()),
            ..Default::default()
        },
        response: Some(EgressResponseSpec {
            unwrap: Some("$.v".into()),  // spawn_capture_server body: {"ok":true,"v":1}
            ..Default::default()
        }),
        crypto: Some(profile),
        ..Default::default()
    };

    let input = InvokeInput {
        supplier_id: "s".into(),
        supplier_code: "s".into(),
        channel_id: "c".into(),
        request_id: "req-envelope".into(),
        payload: serde_json::json!({"hello": "world"}),
        spec,
        config: None,
        timeout: Duration::from_secs(3),
        mode: ExecuteMode::Business,
        options: ExecuteOptions::default(),
        credentials: HashMap::new(),
        pipeline_keys: Some(keys),
        artifact_bytes: None,
        cache_key: None,
    };

    let result = engine().invoke(input).await;
    assert!(result.ok, "{:?}", result.error_message);
    let raw = rx.await.unwrap();
    let http = String::from_utf8_lossy(&raw);
    assert!(http.contains("POST /env"));
    // 明文 hello 不应出现在密文 JSON body 中
    let body = http.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(!body.contains("hello"), "plaintext leaked: {body}");
    assert!(body.contains("payload") || body.contains("iv"), "expected sealed slots: {body}");
}

#[tokio::test]
async fn sign_form_hmac_via_pipeline() {
    let (base, rx) = spawn_capture_server().await;
    let secret = "form-mac-secret";
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::SignOnly);
    profile.canon = CanonStyle::SortedKv;
    profile.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSha256,
        encoding: None,
    });
    profile.slots = vec![
        SlotSpec {
            role: role::SIGN.into(),
            location: SlotLocation::Form,
            name: "sign".into(),
        },
        SlotSpec {
            role: role::BIZ.into(),
            location: SlotLocation::Form,
            name: "biz_content".into(),
        },
    ];
    profile.validate(&registry()).unwrap();

    let mut keys = PipelineKeys::empty();
    keys.mac = Some(mac_key(secret));

    let mut creds = HashMap::new();
    creds.insert("app_id".into(), "app-1".into());
    creds.insert("client_secret".into(), secret.into());

    let spec = ChannelEgressSpec {
        adapter: ADAPTER_SIGN_FORM.into(),
        base_url: base,
        operation: Some("trade.create".into()),
        request: EgressRequestSpec {
            method: "POST".into(),
            path: "/sf".into(),
            body_encoding: BodyEncoding::Json,
            ..Default::default()
        },
        crypto: Some(profile.clone()),
        ..Default::default()
    };

    let input = InvokeInput {
        supplier_id: "s".into(),
        supplier_code: "s".into(),
        channel_id: "c".into(),
        request_id: "req-sign-form".into(),
        payload: serde_json::json!({"amount": 100}),
        spec,
        config: None,
        timeout: Duration::from_secs(3),
        mode: ExecuteMode::Business,
        options: ExecuteOptions::default(),
        credentials: creds,
        pipeline_keys: Some(keys.clone()),
        artifact_bytes: None,
        cache_key: None,
    };

    let result = engine().invoke(input).await;
    assert!(result.ok, "{:?}", result.error_message);
    let raw = rx.await.unwrap();
    let http = String::from_utf8_lossy(&raw);
    assert!(http.contains("POST /sf"));
    let body = http.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(body.contains("sign="), "missing sign: {body}");
    assert!(body.contains("biz_content="), "missing biz: {body}");

    // 用同一 pipeline 复验：解析 form → SortedKv 字段（除 sign）应能通过 registry MAC
    let mut fields = std::collections::BTreeMap::new();
    let mut got_sign = String::new();
    for pair in body.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        let k = urlencoding::decode(k).unwrap().into_owned();
        let v = urlencoding::decode(v).unwrap().into_owned();
        if k == "sign" {
            got_sign = v;
        } else {
            fields.insert(k, v);
        }
    }
    assert_ne!(got_sign, "");
    let message = fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let reg = registry();
    let mac = reg.mac(AlgorithmId::HmacSha256).unwrap();
    let tag = mac.sign(keys.mac.as_ref().unwrap(), message.as_bytes()).unwrap();
    let expected = base64::engine::general_purpose::STANDARD.encode(&tag);
    assert_eq!(got_sign, expected, "HMAC mismatch over {message}");
}

#[test]
fn seal_request_sign_only_matches_adapter_contract() {
    let mut profile = UpstreamCryptoProfile::skeleton(CryptoOrder::SignOnly);
    profile.canon = CanonStyle::SortedKv;
    profile.sign = Some(SignSpec {
        alg: AlgorithmId::HmacSha256,
        encoding: None,
    });
    profile.encodings = WireEncodings::base64_all();
    profile.scope = CipherScope::WholeBody;
    profile.slots = vec![
        SlotSpec {
            role: role::SIGN.into(),
            location: SlotLocation::Form,
            name: "sign".into(),
        },
        SlotSpec {
            role: role::BIZ.into(),
            location: SlotLocation::Form,
            name: "biz_content".into(),
        },
    ];
    let mut keys = PipelineKeys::empty();
    keys.mac = Some(mac_key("x"));
    let fields = vec![
        ("app_id".into(), "a".into()),
        ("biz_content".into(), "{\"n\":1}".into()),
    ];
    let reg = registry();
    let ctx = PipelineContext {
        registry: &reg,
        canon_fields: &fields,
        canon_sep: "&",
        aad: &[],
        field_iv: None,
    };
    let sealed = seal_request(br#"{"n":1}"#, &profile, &keys, &ctx).unwrap();
    assert!(sealed.slots.form.contains_key("sign"));
    assert_eq!(
        sealed.slots.form.get("biz_content").map(String::as_str),
        Some(r#"{"n":1}"#)
    );
}
