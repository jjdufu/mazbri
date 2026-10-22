use base64::{Engine as _, engine::general_purpose::STANDARD};
use mazbri_core::crypto::request_binding::{
    SIGNATURE_INPUT_HMAC_V1, canonical_http_binding, generate_binding_nonce,
    sign_hmac_sha256_base64,
};
use mazbri_core::crypto::transport::{
    DATA_ALGORITHM, SIGNATURE_ALGORITHM, TransportEnvelope, canonical_signature_payload,
    decrypt_aes_256_gcm, encrypt_aes_256_gcm, generate_nonce_base64url, sign_payload,
    validate_transport_algorithms, verify_payload_signature,
};
use mazbri_core::{
    FuryErrorEnvelope, FurySuccessEnvelope, PayloadCodec, deserialize_fury_error,
    deserialize_fury_success, deserialize_payload, serialize_payload,
};
use reqwest::Client;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub gateway_base_url: String,
    pub tenant_id: String,
    pub api_key: String,
    pub tenant_key: Vec<u8>,
    pub key_version: String,
    pub request_codec: PayloadCodec,
    pub response_codec: PayloadCodec,
    /// HMAC-SHA256 request-binding key (`tenant_request_keys`). When set, SDK sends binding headers.
    pub request_hmac_key: Option<Vec<u8>>,
    pub request_key_id: Option<String>,
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
    Serialization(#[from] mazbri_core::CodecError),
    #[error("crypto error: {0}")]
    Crypto(#[from] mazbri_core::crypto::transport::TransportCryptoError),
    #[error("token request failed: {0}")]
    Token(String),
    #[error("gateway response invalid: {0}")]
    Gateway(String),
}

#[derive(Clone)]
pub struct MazbriClient {
    client: Client,
    config: ClientConfig,
    access_token: String,
}

impl MazbriClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn new(config: ClientConfig) -> Result<Self, Error> {
        let client = Client::builder().build()?;
        let mut sdk = Self {
            client,
            config,
            access_token: String::new(),
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

    fn encrypt_request(&self, plaintext: &[u8]) -> Result<TransportEnvelope, Error> {
        let encrypted = encrypt_aes_256_gcm(
            &self.config.tenant_key,
            plaintext,
            self.config.tenant_id.as_bytes(),
        )?;
        let timestamp = chrono::Utc::now().timestamp_millis();
        let nonce = generate_nonce_base64url();
        let mut envelope = TransportEnvelope {
            encrypted: true,
            key_version: self.config.key_version.clone(),
            algorithm: DATA_ALGORITHM.to_string(),
            signature_algorithm: SIGNATURE_ALGORITHM.to_string(),
            timestamp,
            nonce,
            iv: encrypted.iv,
            ciphertext: encrypted.ciphertext,
            tag: encrypted.tag,
            signature: String::new(),
        };
        let payload =
            canonical_signature_payload(&envelope, &self.config.tenant_id, "/api/v1/gateway");
        envelope.signature = sign_payload(&self.config.tenant_key, &payload)?;
        Ok(envelope)
    }

    async fn send_request(
        &self,
        request: &impl Serialize,
        extra_headers: Option<&HeaderMap>,
    ) -> Result<CallResult<GatewayResponseEnvelope>, Error> {
        let plaintext = serialize_payload(self.config.request_codec, request)?;
        let envelope = self.encrypt_request(&plaintext)?;
        let body = serde_json::to_vec(&envelope).map_err(|error| {
            Error::Gateway(format!("failed to serialize encrypted envelope: {error}"))
        })?;
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
        let payload = if let Ok(envelope) = serde_json::from_slice::<TransportEnvelope>(&raw) {
            validate_transport_algorithms(&envelope)?;
            let canonical =
                canonical_signature_payload(&envelope, &self.config.tenant_id, "/api/v1/gateway");
            verify_payload_signature(&self.config.tenant_key, &canonical, &envelope.signature)?;
            let plaintext = decrypt_aes_256_gcm(
                &self.config.tenant_key,
                &envelope.iv,
                &envelope.ciphertext,
                &envelope.tag,
                self.config.tenant_id.as_bytes(),
            )?;
            self.deserialize_gateway_response(&plaintext)?
        } else {
            self.deserialize_gateway_response(&raw)?
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

    fn deserialize_gateway_response(&self, bytes: &[u8]) -> Result<GatewayResponseEnvelope, Error> {
        match self.config.response_codec {
            PayloadCodec::Json => {
                let envelope = deserialize_payload::<GatewayResponseEnvelopeJsonWire>(
                    PayloadCodec::Json,
                    bytes,
                )?;
                Ok(envelope.into())
            }
            PayloadCodec::Fury => match deserialize_fury_success(bytes) {
                Ok(envelope) => Ok(envelope.into()),
                Err(success_error) => match deserialize_fury_error(bytes) {
                    Ok(envelope) => Ok(envelope.into()),
                    Err(error_error) => {
                        let success_error = success_error.to_string();
                        let error_error = error_error.to_string();
                        let json_envelope =
                                    deserialize_payload::<GatewayResponseEnvelopeJsonWire>(
                                        PayloadCodec::Json,
                                        bytes,
                                    )
                                    .map_err(|json_error| {
                                        Error::Serialization(
                                            mazbri_core::CodecError::Fury(format!(
                                                "failed to decode gateway response as Fury (success: {success_error}; error: {error_error}); JSON fallback also failed ({json_error})"
                                            )),
                                        )
                                    })?;
                        warn!(
                            success_error = % success_error,
                            error_error = % error_error,
                            "gateway response was not Fury, falling back to JSON"
                        );
                        Ok(json_envelope.into())
                    }
                },
            },
        }
    }

    fn token_url(&self) -> String {
        format!("{}/auth/token", self.base_url())
    }

    fn invoke_url(&self) -> String {
        format!("{}/api/v1/gateway", self.base_url())
    }

    fn apply_request_binding(
        &self,
        mut builder: reqwest::RequestBuilder,
        body: &[u8],
    ) -> Result<reqwest::RequestBuilder, Error> {
        let Some(key) = self.config.request_hmac_key.as_deref() else {
            return Ok(builder);
        };
        let kid = self
            .config
            .request_key_id
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("1");
        let timestamp = chrono::Utc::now().timestamp();
        let nonce = generate_binding_nonce();
        let canonical = canonical_http_binding("POST", "/api/v1/gateway", timestamp, &nonce, body);
        let signature = sign_hmac_sha256_base64(key, &canonical)
            .map_err(|error| Error::Gateway(format!("request binding sign failed: {error}")))?;
        builder = builder
            .header("X-Mazbri-Timestamp", timestamp.to_string())
            .header("X-Mazbri-Nonce", nonce)
            .header("X-Mazbri-Signature", signature)
            .header("X-Mazbri-Signature-Input", SIGNATURE_INPUT_HMAC_V1)
            .header("X-Mazbri-Key-Id", kid);
        Ok(builder)
    }

    fn base_url(&self) -> String {
        self.config
            .gateway_base_url
            .trim_end_matches('/')
            .to_string()
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

impl From<FurySuccessEnvelope> for GatewayResponseEnvelope {
    fn from(value: FurySuccessEnvelope) -> Self {
        let data = match value.body_encoding.as_deref() {
            Some("text") => String::from_utf8(value.data)
                .map(serde_json::Value::String)
                .ok(),
            Some("binary") => Some(serde_json::Value::String(STANDARD.encode(&value.data))),
            _ => serde_json::from_slice::<serde_json::Value>(&value.data).ok(),
        };
        Self {
            success: value.success,
            data,
            error: None,
            status: value.status,
            request_id: Some(value.request_id),
            content_type: value.content_type,
            body_encoding: value.body_encoding,
        }
    }
}

impl From<FuryErrorEnvelope> for GatewayResponseEnvelope {
    fn from(value: FuryErrorEnvelope) -> Self {
        Self {
            success: value.success,
            data: None,
            error: Some(value.error),
            status: value.status,
            request_id: Some(value.request_id),
            content_type: None,
            body_encoding: None,
        }
    }
}
