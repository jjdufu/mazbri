use mazbri_core::crypto::request_binding::{RequestBindingError, RequestSigner};
use mazbri_core::crypto::transport::{
    SealEnvelopeParams, TransportEnvelope, open_envelope, seal_envelope,
};
use mazbri_core::crypto::{
    AlgorithmRegistry, TenantTransportProfile, TransportMode,
};
use reqwest::Client;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Gateway invoke path (also the `path_and_query` covered by request binding).
pub const GATEWAY_INVOKE_PATH: &str = "/api/v1/gateway";

/// Official SDK client configuration (destructive Profile-based transport).
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Gateway base URL (no trailing slash required).
    pub gateway_base_url: String,
    /// Tenant UUID string (also AEAD AAD).
    pub tenant_id: String,
    /// API key for `/auth/token`.
    pub api_key: String,
    /// Tenant data-plane key material (also used as MAC key when `mac_alg` is set).
    pub tenant_key: Vec<u8>,
    /// Active key version string (e.g. `v1`).
    pub key_version: String,
    /// Tenant transport Profile (`mode=none|request|full` + suite/algs).
    pub transport: TenantTransportProfile,
    /// Optional HTTP request binding signer (`tenant_request_keys` kid + alg + key):
    /// HMAC secret or asymmetric private key, see [`RequestSigner::new`].
    pub request_signer: Option<RequestSigner>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub success: bool,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub expires_in: Option<usize>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub tenant_info: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CallResult<T> {
    pub response: T,
    pub http_status: u16,
    pub response_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("crypto error: {0}")]
    Crypto(#[from] mazbri_core::crypto::transport::TransportCryptoError),
    #[error("token request failed: {0}")]
    Token(String),
    #[error("gateway response invalid: {0}")]
    Gateway(String),
    #[error("unsupported transport algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("legacy transport algorithm denied: {0}")]
    LegacyDenied(String),
    #[error("request binding failed: {0}")]
    Binding(#[from] RequestBindingError),
}

#[derive(Clone)]
pub struct MazbriClient {
    client: Client,
    config: ClientConfig,
    access_token: String,
    registry: AlgorithmRegistry,
}

impl MazbriClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn new(config: ClientConfig) -> Result<Self, Error> {
        let client = Client::builder().build()?;
        let registry = AlgorithmRegistry::default_registry();
        config
            .transport
            .validate(&registry)
            .map_err(map_profile_error)?;
        let mut sdk = Self {
            client,
            config,
            access_token: String::new(),
            registry,
        };
        let token = sdk.fetch_token().await?;
        sdk.access_token = token
            .access_token
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| Error::Token("token response did not include access_token".into()))?;
        Ok(sdk)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn fetch_token(&self) -> Result<TokenResponse, Error> {
        let url = self.token_url();
        let response = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "api_key": self.config.api_key,
                "grant_type": "client_credentials"
            }))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        let token: TokenResponse = serde_json::from_str(&body).map_err(|error| {
            Error::Token(format!(
                "failed to parse token response (HTTP {status}): {error}"
            ))
        })?;
        if !token.success {
            return Err(Error::Token(token.error.unwrap_or_else(|| {
                format!("token request rejected with HTTP {status}")
            })));
        }
        Ok(token)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn call<Req, Resp>(&self, request: &Req) -> Result<Resp, Error>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        Ok(self.call_with_meta(request).await?.response)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn call_envelope<Req>(
        &self,
        request: &Req,
    ) -> Result<CallResult<GatewayResponseEnvelope>, Error>
    where
        Req: Serialize,
    {
        self.send_request(request, None).await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn call_envelope_with_headers<Req>(
        &self,
        request: &Req,
        headers: &HeaderMap,
    ) -> Result<CallResult<GatewayResponseEnvelope>, Error>
    where
        Req: Serialize,
    {
        self.send_request(request, Some(headers)).await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn call_with_meta<Req, Resp>(&self, request: &Req) -> Result<CallResult<Resp>, Error>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        let result = self.send_request(request, None).await?;
        let response = Self::decode_envelope(result.response)?;
        Ok(CallResult {
            response,
            http_status: result.http_status,
            response_bytes: result.response_bytes,
        })
    }

    fn seal_request(&self, plaintext: &[u8]) -> Result<TransportEnvelope, Error> {
        let (_, mac_alg) = self
            .config
            .transport
            .effective_algs()
            .map_err(map_profile_error)?;
        let mac_key = mac_alg.map(|_| self.config.tenant_key.as_slice());
        seal_envelope(SealEnvelopeParams {
            profile: &self.config.transport,
            registry: &self.registry,
            data_key: &self.config.tenant_key,
            mac_key,
            key_version: &self.config.key_version,
            tenant_id: &self.config.tenant_id,
            http_path: GATEWAY_INVOKE_PATH,
            plaintext,
            timestamp: chrono::Utc::now().timestamp(),
        })
        .map_err(map_crypto_error)
    }

    fn open_response(&self, envelope: &TransportEnvelope) -> Result<Vec<u8>, Error> {
        let mac_key = envelope
            .mac_alg
            .as_ref()
            .map(|_| self.config.tenant_key.as_slice());
        open_envelope(
            &self.config.transport,
            &self.registry,
            &self.config.tenant_key,
            mac_key,
            &self.config.tenant_id,
            GATEWAY_INVOKE_PATH,
            envelope,
        )
        .map_err(map_crypto_error)
    }

    async fn send_request(
        &self,
        request: &impl Serialize,
        extra_headers: Option<&HeaderMap>,
    ) -> Result<CallResult<GatewayResponseEnvelope>, Error> {
        let plaintext = serde_json::to_vec(request)?;
        let body = match self.config.transport.mode {
            TransportMode::None => plaintext,
            TransportMode::Request | TransportMode::Full => {
                let envelope = self.seal_request(&plaintext)?;
                serde_json::to_vec(&envelope).map_err(|error| {
                    Error::Gateway(format!("failed to serialize encrypted envelope: {error}"))
                })?
            }
        };
        let mut request_builder = self
            .client
            .post(self.invoke_url())
            .bearer_auth(&self.access_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.clone());
        request_builder = self.apply_request_binding(request_builder, &body)?;
        if let Some(headers) = extra_headers {
            request_builder = request_builder.headers(headers.clone());
        }
        let response = request_builder.send().await?;
        let http_status = response.status().as_u16();
        let raw = response.bytes().await?;
        let response_bytes = raw.len();
        let payload = match self.config.transport.mode {
            TransportMode::Full => {
                let envelope: TransportEnvelope = serde_json::from_slice(&raw).map_err(|error| {
                    Error::Gateway(format!(
                        "mode=full expects TransportEnvelope response: {error}"
                    ))
                })?;
                let plaintext = self.open_response(&envelope)?;
                Self::deserialize_gateway_response(&plaintext)?
            }
            TransportMode::None | TransportMode::Request => {
                if let Ok(envelope) = serde_json::from_slice::<TransportEnvelope>(&raw) {
                    return Err(Error::Gateway(format!(
                        "unexpected encrypted response under mode={:?} (data_alg={})",
                        self.config.transport.mode, envelope.data_alg
                    )));
                }
                Self::deserialize_gateway_response(&raw)?
            }
        };
        Ok(CallResult {
            response: payload,
            http_status,
            response_bytes,
        })
    }

    fn decode_envelope<Resp>(envelope: GatewayResponseEnvelope) -> Result<Resp, Error>
    where
        Resp: DeserializeOwned,
    {
        if !envelope.success {
            return Err(Error::Gateway(
                envelope
                    .error
                    .unwrap_or_else(|| "gateway returned an error".to_string()),
            ));
        }
        let data = envelope
            .data
            .ok_or_else(|| Error::Gateway("gateway response did not include data".to_string()))?;
        let response = serde_json::from_value::<Resp>(data).map_err(|error| {
            Error::Gateway(format!(
                "failed to decode gateway data into target type: {error}"
            ))
        })?;
        Ok(response)
    }

    fn deserialize_gateway_response(bytes: &[u8]) -> Result<GatewayResponseEnvelope, Error> {
        Ok(serde_json::from_slice::<GatewayResponseEnvelopeJsonWire>(bytes)?.into())
    }

    fn token_url(&self) -> String {
        format!("{}/auth/token", self.base_url())
    }

    fn invoke_url(&self) -> String {
        format!("{}{GATEWAY_INVOKE_PATH}", self.base_url())
    }

    fn apply_request_binding(
        &self,
        mut builder: reqwest::RequestBuilder,
        body: &[u8],
    ) -> Result<reqwest::RequestBuilder, Error> {
        let Some(signer) = self.config.request_signer.as_ref() else {
            return Ok(builder);
        };
        let headers = signer.sign("POST", GATEWAY_INVOKE_PATH, body)?;
        for (name, value) in headers.pairs() {
            builder = builder.header(name, value);
        }
        Ok(builder)
    }

    fn base_url(&self) -> String {
        self.config
            .gateway_base_url
            .trim_end_matches('/')
            .to_string()
    }
}

fn map_profile_error(error: mazbri_core::crypto::CryptoError) -> Error {
    use mazbri_core::crypto::CryptoError;
    match error {
        CryptoError::LegacyNotAllowed { id } => Error::LegacyDenied(id.as_str().to_owned()),
        CryptoError::UnknownAlgorithmId { raw } => Error::UnsupportedAlgorithm(raw),
        CryptoError::NotRegistered { id } => Error::UnsupportedAlgorithm(id.as_str().to_owned()),
        CryptoError::Unsupported { reason } => Error::UnsupportedAlgorithm(reason),
        other => Error::Gateway(format!("invalid transport profile: {other}")),
    }
}

fn map_crypto_error(error: mazbri_core::crypto::transport::TransportCryptoError) -> Error {
    use mazbri_core::crypto::transport::TransportCryptoError;
    use mazbri_core::crypto::CryptoError;
    match error {
        TransportCryptoError::UnsupportedAlgorithm => {
            Error::UnsupportedAlgorithm("closed-set algorithm required".into())
        }
        TransportCryptoError::Crypto(CryptoError::LegacyNotAllowed { id }) => {
            Error::LegacyDenied(id.as_str().to_owned())
        }
        TransportCryptoError::Crypto(CryptoError::UnknownAlgorithmId { raw }) => {
            Error::UnsupportedAlgorithm(raw)
        }
        TransportCryptoError::Crypto(CryptoError::NotRegistered { id }) => {
            Error::UnsupportedAlgorithm(id.as_str().to_owned())
        }
        TransportCryptoError::InvalidProfile(reason) => Error::Gateway(format!(
            "invalid transport profile or envelope: {reason}"
        )),
        other => Error::Crypto(other),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayResponseEnvelope {
    pub success: bool,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    pub status: u16,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub body_encoding: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayResponseEnvelopeJsonWire {
    success: bool,
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<String>,
    status: u16,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    body_encoding: Option<String>,
}

impl From<GatewayResponseEnvelopeJsonWire> for GatewayResponseEnvelope {
    fn from(value: GatewayResponseEnvelopeJsonWire) -> Self {
        Self {
            success: value.success,
            data: value.data,
            error: value.error,
            status: value.status,
            request_id: value.request_id,
            content_type: value.content_type,
            body_encoding: value.body_encoding,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::crypto::transport::{open_envelope, seal_envelope, SealEnvelopeParams};
    use mazbri_core::crypto::{
        AlgorithmId, AlgorithmRegistry, KeySelection, TenantTransportProfile, TransportMode,
        SUITE_TENANT_DEFAULT_2026_11,
    };

    fn aes_full_profile() -> TenantTransportProfile {
        let mut profile =
            TenantTransportProfile::from_named_suite(SUITE_TENANT_DEFAULT_2026_11, TransportMode::Full)
                .expect("suite");
        profile.key = Some(KeySelection::Latest {
            key_id: "tenant-default".into(),
        });
        profile
    }

    #[test]
    fn mode_none_profile_validates_without_algs() {
        let registry = AlgorithmRegistry::default_registry();
        let profile = TenantTransportProfile::plaintext();
        profile.validate(&registry).expect("plaintext ok");
        assert!(matches!(profile.mode, TransportMode::None));
    }

    #[test]
    fn mode_full_aes_256_gcm_roundtrip_via_seal_open() {
        let registry = AlgorithmRegistry::default_registry();
        let profile = aes_full_profile();
        profile.validate(&registry).expect("profile");
        let key = [9u8; 32];
        let plaintext = br#"{"product_id":"demo"}"#;
        let sealed = seal_envelope(SealEnvelopeParams {
            profile: &profile,
            registry: &registry,
            data_key: &key,
            mac_key: None,
            key_version: "v1",
            tenant_id: "tenant-1",
            http_path: GATEWAY_INVOKE_PATH,
            plaintext,
            timestamp: 1_710_000_000,
        })
        .expect("seal");
        assert_eq!(sealed.data_alg, "aes-256-gcm");
        assert!(sealed.mac_alg.is_none());
        let opened = open_envelope(
            &profile,
            &registry,
            &key,
            None,
            "tenant-1",
            GATEWAY_INVOKE_PATH,
            &sealed,
        )
        .expect("open");
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn legacy_alg_without_allow_insecure_is_denied() {
        let registry = AlgorithmRegistry::default_registry();
        let mut profile = TenantTransportProfile::from_named_suite(
            SUITE_TENANT_DEFAULT_2026_11,
            TransportMode::Request,
        )
        .expect("suite");
        profile.key = Some(KeySelection::Latest {
            key_id: "tenant-default".into(),
        });
        profile.suite = None;
        profile.data_alg = Some(AlgorithmId::Aes256Ecb);
        profile.mac_alg = Some(AlgorithmId::HmacSha256);
        profile.allow_insecure = false;
        let err = profile.validate(&registry).expect_err("legacy denied");
        assert!(matches!(
            err,
            mazbri_core::crypto::CryptoError::LegacyNotAllowed { .. }
        ));
    }

    #[test]
    fn map_crypto_error_surfaces_legacy_and_unsupported() {
        use mazbri_core::crypto::transport::TransportCryptoError;
        use mazbri_core::crypto::{AlgorithmId, CryptoError};
        match map_crypto_error(TransportCryptoError::Crypto(CryptoError::LegacyNotAllowed {
            id: AlgorithmId::Aes256Ecb,
        })) {
            Error::LegacyDenied(id) => assert_eq!(id, "aes-256-ecb"),
            other => panic!("unexpected {other:?}"),
        }
        match map_crypto_error(TransportCryptoError::UnsupportedAlgorithm) {
            Error::UnsupportedAlgorithm(_) => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    fn offline_client(signer: Option<RequestSigner>) -> MazbriClient {
        MazbriClient {
            client: Client::builder().build().expect("client"),
            config: ClientConfig {
                gateway_base_url: "http://gateway.invalid".into(),
                tenant_id: "00000000-0000-0000-0000-000000000001".into(),
                api_key: String::new(),
                tenant_key: vec![0u8; 32],
                key_version: "v1".into(),
                transport: TenantTransportProfile::plaintext(),
                request_signer: signer,
            },
            access_token: String::new(),
            registry: AlgorithmRegistry::default_registry(),
        }
    }

    #[test]
    fn request_binding_headers_verify_with_registered_public_key() {
        use mazbri_core::crypto::request_binding::{
            HEADER_KEY_ID, HEADER_NONCE, HEADER_SIGNATURE, HEADER_SIGNATURE_INPUT,
            HEADER_TIMESTAMP, binding_key_ref, canonical_http_binding, parse_signature_input,
            verify_binding,
        };
        let body = br#"{"product_id":"p1","path":"/x"}"#;
        for alg in [AlgorithmId::Ed25519, AlgorithmId::Sm2Sign, AlgorithmId::EcdsaP256Sha256] {
            let pair = mazbri_core::crypto::generate_sign_keypair(alg).expect("keygen");
            let signer = RequestSigner::new("3", alg, pair.private_pem.as_bytes()).expect("signer");
            let sdk = offline_client(Some(signer));
            let request = sdk
                .apply_request_binding(sdk.client.post(sdk.invoke_url()), body)
                .expect("binding")
                .build()
                .expect("request");
            let header = |name: &str| {
                request
                    .headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_else(|| panic!("missing {name}"))
                    .to_owned()
            };
            assert_eq!(header(HEADER_KEY_ID), "3");
            assert_eq!(parse_signature_input(&header(HEADER_SIGNATURE_INPUT)).expect("input"), alg);
            let timestamp: i64 = header(HEADER_TIMESTAMP).parse().expect("ts");
            let canonical =
                canonical_http_binding("POST", GATEWAY_INVOKE_PATH, timestamp, &header(HEADER_NONCE), body);
            let public = binding_key_ref(alg, pair.public_pem.as_bytes()).expect("public");
            verify_binding(&sdk.registry, alg, &public, &canonical, &header(HEADER_SIGNATURE))
                .unwrap_or_else(|e| panic!("{alg}: {e}"));
        }
    }

    #[test]
    fn request_binding_absent_without_signer() {
        let sdk = offline_client(None);
        let request = sdk
            .apply_request_binding(sdk.client.post(sdk.invoke_url()), b"{}")
            .expect("binding")
            .build()
            .expect("request");
        assert!(request.headers().get("x-mazbri-signature").is_none());
    }
}
