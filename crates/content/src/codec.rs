use serde::{Serialize, de::DeserializeOwned};
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CodecError {
    #[error("Serialization failed: {0}")]
    Encode(String),
    #[error("Deserialization failed: {0}")]
    Decode(String),
    #[error("Unsupported codec type: {0}")]
    Unsupported(String),
}

/// Parsed from config files (e.g. `config.toml` with `codec = "cbor"`).
///
/// Bincode was removed — see `../fluxora/docs/decisions/001-reject-bincode-for-cbor.md`:
/// - serde 2.x incompatible (v1.x broken, v2.x API unstable)
/// - No type self-description; Gateway cannot partially parse routing metadata
/// - CBOR (`ciborium`) covers all advantages and adds cross-language support
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
pub enum CodecType {
    Json,
    #[default]
    Cbor,
}

impl FromStr for CodecType {
    type Err = CodecError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "json" => Ok(Self::Json),
            "cbor" => Ok(Self::Cbor),
            _ => Err(CodecError::Unsupported(s.into())),
        }
    }
}

/// 用于运行时执行 (替代 Box<dyn Codec>)
/// 去除了冗余的内部结构体，直接内联逻辑
#[derive(Debug, Clone)]
pub enum ActiveCodec {
    Json,
    Cbor,
}

impl ActiveCodec {
    pub fn new(t: CodecType) -> Self {
        match t {
            CodecType::Json => Self::Json,
            CodecType::Cbor => Self::Cbor,
        }
    }

    /// Returns the config-level `CodecType` corresponding to this active codec.
    pub fn as_type(&self) -> CodecType {
        match self {
            Self::Json => CodecType::Json,
            Self::Cbor => CodecType::Cbor,
        }
    }

    pub fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>, CodecError> {
        match self {
            Self::Json => serde_json::to_vec(value).map_err(|e| CodecError::Encode(e.to_string())),
            Self::Cbor => {
                let mut out = Vec::new();
                ciborium::ser::into_writer(value, &mut out)
                    .map_err(|e| CodecError::Encode(e.to_string()))?;
                Ok(out)
            }
        }
    }

    pub fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, CodecError> {
        match self {
            Self::Json => {
                serde_json::from_slice(bytes).map_err(|e| CodecError::Decode(e.to_string()))
            }
            Self::Cbor => {
                let mut cursor = std::io::Cursor::new(bytes);
                ciborium::de::from_reader(&mut cursor)
                    .map_err(|e| CodecError::Decode(e.to_string()))
            }
        }
    }

    /// Receive-side auto-detect: the frame's first byte decides the wire
    /// format, independent of this codec's pinned send format. JSON messages
    /// are objects ('{'); CBOR top-level maps start in the 0xA0-0xBF major
    /// type 5 range. Anything else (or a shape mismatch) fails.
    pub fn decode_auto<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, CodecError> {
        match bytes.first() {
            Some(b'{') => Self::decode_json(bytes),
            Some(b) if (0xA0..=0xBF).contains(b) => Self::decode_cbor(bytes),
            _ => Err(CodecError::Decode("frame not json/cbor".into())),
        }
    }

    fn decode_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
        serde_json::from_slice(bytes).map_err(|e| CodecError::Decode(e.to_string()))
    }

    fn decode_cbor<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
        let mut cursor = std::io::Cursor::new(bytes);
        ciborium::de::from_reader(&mut cursor).map_err(|e| CodecError::Decode(e.to_string()))
    }
}
