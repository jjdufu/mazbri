use fory::{Fory, ForyStruct};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayloadCodec {
    #[default]
    Json,
    Fury,
}

impl PayloadCodec {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Fury => "fury",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "fury" => Some(Self::Fury),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("json codec error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("fury codec error: {0}")]
    Fury(String),
    #[error("unsupported codec")]
    UnsupportedCodec,
}

pub fn serialize_payload<T>(codec: PayloadCodec, value: &T) -> Result<Vec<u8>, CodecError>
where
    T: Serialize,
{
    match codec {
        PayloadCodec::Json | PayloadCodec::Fury => Ok(serde_json::to_vec(value)?),
    }
}

pub fn deserialize_payload<T>(codec: PayloadCodec, bytes: &[u8]) -> Result<T, CodecError>
where
    T: DeserializeOwned,
{
    match codec {
        PayloadCodec::Json | PayloadCodec::Fury => Ok(serde_json::from_slice(bytes)?),
    }
}

// ==========================================
// Specific Envelopes for Pure Binary Fury
// ==========================================

#[derive(ForyStruct, Debug, Clone)]
pub struct FurySuccessEnvelope {
    pub success: bool,
    pub data: Vec<u8>,
    pub status: u16,
    pub request_id: String,
    pub content_type: Option<String>,
    pub body_encoding: Option<String>,
}

#[derive(ForyStruct, Debug, Clone)]
pub struct FuryErrorEnvelope {
    pub success: bool,
    pub error: String,
    pub status: u16,
    pub request_id: String,
}

pub fn serialize_fury_success(envelope: &FurySuccessEnvelope) -> Result<Vec<u8>, CodecError> {
    let mut fory = Fory::builder().xlang(true).build();
    fory.register_by_name::<FurySuccessEnvelope>("mazbri", "FurySuccessEnvelope")
        .map_err(|e| CodecError::Fury(e.to_string()))?;
    fory.serialize(envelope)
        .map_err(|e| CodecError::Fury(e.to_string()))
}

pub fn deserialize_fury_success(bytes: &[u8]) -> Result<FurySuccessEnvelope, CodecError> {
    let mut fory = Fory::builder().xlang(true).build();
    fory.register_by_name::<FurySuccessEnvelope>("mazbri", "FurySuccessEnvelope")
        .map_err(|e| CodecError::Fury(e.to_string()))?;
    fory.deserialize(bytes)
        .map_err(|e| CodecError::Fury(e.to_string()))
}

pub fn serialize_fury_error(envelope: &FuryErrorEnvelope) -> Result<Vec<u8>, CodecError> {
    let mut fory = Fory::builder().xlang(true).build();
    fory.register_by_name::<FuryErrorEnvelope>("mazbri", "FuryErrorEnvelope")
        .map_err(|e| CodecError::Fury(e.to_string()))?;
    fory.serialize(envelope)
        .map_err(|e| CodecError::Fury(e.to_string()))
}

pub fn deserialize_fury_error(bytes: &[u8]) -> Result<FuryErrorEnvelope, CodecError> {
    let mut fory = Fory::builder().xlang(true).build();
    fory.register_by_name::<FuryErrorEnvelope>("mazbri", "FuryErrorEnvelope")
        .map_err(|e| CodecError::Fury(e.to_string()))?;
    fory.deserialize(bytes)
        .map_err(|e| CodecError::Fury(e.to_string()))
}
