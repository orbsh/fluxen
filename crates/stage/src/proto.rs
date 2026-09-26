//! Wire payloads between CLI and mirror. The mirror never inspects payloads;
//! it routes raw bytes between peers. So CLI sends a BARE Message<Brick> —
//! exactly what the UI's decoder expects.

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
        ev: content::EV_DRAW.into(),
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