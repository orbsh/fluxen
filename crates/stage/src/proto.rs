//! Wire frames between CLI and mirror. The mirror never inspects payloads,
//! only routes frames between peers.

/// A frame sent over the mirror.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "dir", rename_all = "lowercase")]
pub enum Frame {
    /// CLI -> UI: content protocol message (carries bricks or raw data)
    Ui { payload: Value },
    /// CLI -> CLI: raw text
    Cli { text: String },
}

use content::Content;
use serde_json::Value;

/// Parse KDL and wrap the resulting bricks in a Content::Create message —
/// the same frame shape the UI already consumes on the fluxora gateway.
pub fn parse_kdl_to_frame(src: &str) -> Result<Value, crate::error::KdlError> {
    let bricks = crate::kdl_parse::parse(src)?;
    let payload = if bricks.len() == 1 {
        serde_json::to_value(&bricks[0])?
    } else {
        serde_json::to_value(&bricks)?
    };
    let msg = content::Message {
        sender: "stage".into(),
        created: None,
        content: vec![Content::Create(content::Influx {
            event: "stage".into(),
            data: payload,
            method: Default::default(),
            channel: None,
        })],
    };
    Ok(serde_json::to_value(&msg)?)
}

/// Offline KDL -> brick JSON trees (no content envelope).
pub fn parse_kdl_to_bricks(src: &str) -> Result<Vec<brick::Brick>, crate::error::KdlError> {
    crate::kdl_parse::parse(src)
}
