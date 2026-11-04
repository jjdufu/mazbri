use super::response::gateway_error_response;
use crate::crypto::TenantCryptoState;
use crate::gateway::metrics::{GATEWAY_JWT_KID_TOTAL, GATEWAY_REQUEST_BINDING_TOTAL};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use crate::crypto::RequestBindingKey;
use mazbri_core::crypto::AlgorithmRegistry;
pub use mazbri_core::crypto::request_binding::{
    HEADER_KEY_ID, HEADER_NONCE, HEADER_SIGNATURE, HEADER_SIGNATURE_INPUT, HEADER_TIMESTAMP,
};
use mazbri_core::crypto::request_binding::{
    SignatureVersion, canonical_http_binding, parse_signature_input, parse_unix_timestamp,
    truncate_nonce_for_log, validate_binding_nonce, validate_unix_timestamp, verify_binding,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct GatewayHttpPath(pub String);

#[derive(Debug, Clone)]
pub struct RequestBindingOutcome {
    pub auth_mode: &'static str,
    pub kid: Option<String>,
    pub nonce_trunc: Option<String>,
    pub sig_ok: Option<bool>,
}

#[derive(Debug)]
pub struct RequestBindingError {
    pub code: &'static str,
    pub message: &'static str,
}

impl RequestBindingError {
    fn missing() -> Self {
        Self {
            code: "40110",
            message: "Request binding headers are required",
        }
    }
    fn timestamp() -> Self {
        Self {
            code: "40111",
            message: "Request timestamp is outside the allowed skew",
        }
    }
    fn nonce() -> Self {
        Self {
            code: "40112",
            message: "Request nonce was already used",
        }
    }
    fn signature() -> Self {
        Self {
            code: "40113",
            message: "Request signature mismatch",
        }
    }
    const fn failure_reason(&self) -> mazbri_auth::AuthFailureReason {
        use mazbri_auth::AuthFailureReason as R;
        match self.code.as_bytes() {
            b"40110" => R::Missing,
            b"40111" | b"40112" => R::Replay,
            _ => R::Signature,
        }
    }
    fn kid() -> Self {
        Self {
            code: "40114",
            message: "Request binding key id is unknown or inactive",
        }
    }
}

impl IntoResponse for RequestBindingError {
    fn into_response(self) -> Response {
        GATEWAY_REQUEST_BINDING_TOTAL
            .with_label_values(&[self.code])
            .inc();
        mazbri_auth::record_auth_failure(self.failure_reason());
        let mut response =
            gateway_error_response(StatusCode::UNAUTHORIZED, self.code, self.message, None);
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            header::HeaderValue::from_static("Bearer realm=\"mazbri\", error=\"invalid_token\""),
        );
        response
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &'static str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn any_binding_header(headers: &HeaderMap) -> bool {
    [
        HEADER_TIMESTAMP,
        HEADER_NONCE,
        HEADER_SIGNATURE,
        HEADER_SIGNATURE_INPUT,
        HEADER_KEY_ID,
    ]
    .iter()
    .any(|name| headers.contains_key(*name))
}

/// 纯函数校验：签名版本族 ⊇ 密钥算法，`Signature-Input` 与 kid 的算法严格一致，签名正确。
///
/// 与 SDK / loadgen 使用的 `RequestSigner` 互通（见单元测试）。
///
/// # Errors
///
/// 任一条件不满足返回 40113。
pub fn verify_binding_signature(
    registry: &AlgorithmRegistry,
    version: SignatureVersion,
    key: &RequestBindingKey,
    signature_input: &str,
    canonical: &str,
    signature: &str,
) -> Result<(), RequestBindingError> {
    let declared = parse_signature_input(signature_input).map_err(|_| RequestBindingError::signature())?;
    if declared != key.alg || !version.allows(key.alg) {
        return Err(RequestBindingError::signature());
    }
    verify_binding(registry, key.alg, &key.key, canonical, signature)
        .map_err(|_| RequestBindingError::signature())
}

/// `enforce_request_binding` 的入参，字段全部是借用，不产生额外拷贝。
pub struct RequestBindingInput<'a> {
    pub crypto: &'a TenantCryptoState,
    pub tenant_id: Uuid,
    pub require: bool,
    pub nonce_prefix: &'a str,
    pub method: &'a str,
    pub path_and_query: &'a str,
    pub headers: &'a HeaderMap,
    pub body: &'a [u8],
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
/// # Panics
///
/// 仅在内部不变量被破坏时 panic。
pub async fn enforce_request_binding(
    input: RequestBindingInput<'_>,
) -> Result<RequestBindingOutcome, RequestBindingError> {
    let RequestBindingInput {
        crypto,
        tenant_id,
        require,
        nonce_prefix,
        method,
        path_and_query,
        headers,
        body,
    } = input;
    let profile = crypto
        .profile(tenant_id)
        .await
        .map_err(|_| RequestBindingError::kid())?;
    let required = require || profile.require_request_binding;
    let version = SignatureVersion::parse(&profile.signature_version)
        .map_err(|_| RequestBindingError::signature())?;
    if !required && !any_binding_header(headers) {
        GATEWAY_REQUEST_BINDING_TOTAL
            .with_label_values(&["skipped"])
            .inc();
        return Ok(RequestBindingOutcome {
            auth_mode: "bearer",
            kid: None,
            nonce_trunc: None,
            sig_ok: None,
        });
    }
    let timestamp_raw = header_str(headers, HEADER_TIMESTAMP);
    let nonce = header_str(headers, HEADER_NONCE);
    let signature = header_str(headers, HEADER_SIGNATURE);
    let signature_input = header_str(headers, HEADER_SIGNATURE_INPUT);
    let kid = header_str(headers, HEADER_KEY_ID);
    if timestamp_raw.is_none()
        || nonce.is_none()
        || signature.is_none()
        || signature_input.is_none()
        || kid.is_none()
    {
        return Err(RequestBindingError::missing());
    }
    let timestamp_raw = timestamp_raw.expect("checked");
    let nonce = nonce.expect("checked");
    let signature = signature.expect("checked");
    let signature_input = signature_input.expect("checked");
    let kid = kid.expect("checked");
    if parse_signature_input(signature_input).is_err() {
        return Err(RequestBindingError::signature());
    }
    if validate_binding_nonce(nonce).is_err() {
        return Err(RequestBindingError::signature());
    }
    let timestamp =
        parse_unix_timestamp(timestamp_raw).map_err(|_| RequestBindingError::timestamp())?;
    let now = chrono::Utc::now().timestamp();
    let skew = i64::from(profile.clock_skew_seconds);
    if validate_unix_timestamp(timestamp, now, skew).is_err() {
        return Err(RequestBindingError::timestamp());
    }
    let key = crypto
        .request_binding_key(tenant_id, kid)
        .await
        .map_err(|_| RequestBindingError::kid())?
        .ok_or_else(RequestBindingError::kid)?;
    let canonical = canonical_http_binding(method, path_and_query, timestamp, nonce, body);
    verify_binding_signature(
        crypto.registry(),
        version,
        &key,
        signature_input,
        &canonical,
        signature,
    )?;
    let inserted = crypto
        .consume_binding_nonce(nonce_prefix, tenant_id, nonce, profile.nonce_ttl_seconds)
        .await
        .map_err(|_| RequestBindingError::nonce())?;
    if !inserted {
        return Err(RequestBindingError::nonce());
    }
    GATEWAY_REQUEST_BINDING_TOTAL
        .with_label_values(&["ok"])
        .inc();
    Ok(RequestBindingOutcome {
        auth_mode: if version.is_symmetric() {
            "bearer+hmac"
        } else {
            "bearer+sig"
        },
        kid: Some(kid.to_string()),
        nonce_trunc: Some(truncate_nonce_for_log(nonce)),
        sig_ok: Some(true),
    })
}

pub fn record_jwt_kid(kid: &str, result: &'static str) {
    GATEWAY_JWT_KID_TOTAL
        .with_label_values(&[kid, result])
        .inc();
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::crypto::request_binding::{RequestSigner, binding_key_ref};
    use mazbri_core::crypto::{AlgorithmId, generate_sign_keypair};

    const PATH: &str = "/api/v1/gateway";
    const BODY: &[u8] = br#"{"productCode":"p","data":{}}"#;

    fn server_key(alg: AlgorithmId, material: &[u8]) -> RequestBindingKey {
        RequestBindingKey {
            alg,
            key: binding_key_ref(alg, material).expect("server key"),
        }
    }

    fn canonical_for(headers: &mazbri_core::crypto::request_binding::BindingHeaders) -> String {
        canonical_http_binding("POST", PATH, headers.timestamp, &headers.nonce, BODY)
    }

    /// 客户端（SDK / loadgen 共用的 `RequestSigner`）签名 → gateway 校验，覆盖全部 5 个版本族。
    #[test]
    fn request_signer_interops_with_gateway_for_every_version() {
        let registry = AlgorithmRegistry::default_registry();
        for version in SignatureVersion::ALL {
            let alg = version.default_alg();
            let (client_material, server_material) = if version.is_symmetric() {
                (vec![7u8; 32], vec![7u8; 32])
            } else {
                let pair = generate_sign_keypair(alg).expect("keygen");
                (pair.private_pem.as_bytes().to_vec(), pair.public_pem.into_bytes())
            };
            let signer = RequestSigner::new("1", alg, &client_material).expect("signer");
            let headers = signer.sign("POST", PATH, BODY).expect("sign");
            let key = server_key(alg, &server_material);
            verify_binding_signature(
                &registry,
                version,
                &key,
                &headers.signature_input,
                &canonical_for(&headers),
                &headers.signature,
            )
            .unwrap_or_else(|_| panic!("{version} must verify"));
            let tampered = canonical_http_binding("POST", PATH, headers.timestamp, &headers.nonce, b"{}");
            assert!(
                verify_binding_signature(
                    &registry,
                    version,
                    &key,
                    &headers.signature_input,
                    &tampered,
                    &headers.signature
                )
                .is_err(),
                "{version} tampered body must fail"
            );
        }
    }

    #[test]
    fn rejects_wrong_key_wrong_input_and_wrong_family() {
        let registry = AlgorithmRegistry::default_registry();
        let alg = AlgorithmId::EcdsaP256Sha256;
        let client = generate_sign_keypair(alg).expect("keygen");
        let other = generate_sign_keypair(alg).expect("keygen");
        let signer = RequestSigner::new("1", alg, client.private_pem.as_bytes()).expect("signer");
        let headers = signer.sign("POST", PATH, BODY).expect("sign");
        let canonical = canonical_for(&headers);
        let good = server_key(alg, client.public_pem.as_bytes());
        let wrong_key = server_key(alg, other.public_pem.as_bytes());
        let check = |version, key: &RequestBindingKey, input: &str| {
            verify_binding_signature(&registry, version, key, input, &canonical, &headers.signature)
        };
        assert!(check(SignatureVersion::EcdsaV1, &good, &headers.signature_input).is_ok());
        let err = check(SignatureVersion::EcdsaV1, &wrong_key, &headers.signature_input).unwrap_err();
        assert_eq!(err.code, "40113");
        // Signature-Input 声明的算法与 kid 的算法不一致
        assert!(check(SignatureVersion::EcdsaV1, &good, "ecdsa-p384-sha384=v1").is_err());
        assert!(check(SignatureVersion::EcdsaV1, &good, "hmac-sha256=v1").is_err());
        assert!(check(SignatureVersion::EcdsaV1, &good, "ecdsa-p256-sha256").is_err());
        // 租户配置的签名版本族不允许该算法
        for version in [
            SignatureVersion::HmacV1,
            SignatureVersion::RsaV1,
            SignatureVersion::Ed25519V1,
            SignatureVersion::Sm2V1,
        ] {
            assert!(check(version, &good, &headers.signature_input).is_err(), "{version}");
        }
    }

    #[test]
    fn hmac_key_cannot_verify_asymmetric_claim() {
        let registry = AlgorithmRegistry::default_registry();
        let signer = RequestSigner::new("1", AlgorithmId::HmacSha256, &[9u8; 32]).expect("signer");
        let headers = signer.sign("POST", PATH, BODY).expect("sign");
        let key = server_key(AlgorithmId::HmacSha256, &[8u8; 32]);
        assert!(
            verify_binding_signature(
                &registry,
                SignatureVersion::HmacV1,
                &key,
                &headers.signature_input,
                &canonical_for(&headers),
                &headers.signature,
            )
            .is_err(),
            "wrong HMAC key must fail"
        );
    }
}
