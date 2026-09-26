//! Wire payloads between CLI and mirror. The mirror never inspects payloads;
//! it routes raw bytes between peers. So CLI sends a BARE Message<Accrete> —
//! exactly what the UI's decoder expects.

use content::Content;
use serde::Deserialize;
use serde_json::Value;

/// Parse KDL and wrap the resulting accretes in a Content::Create message —
/// the same frame shape the UI already consumes on the fluxora gateway.
pub fn parse_kdl_to_frame(src: &str) -> Result<Value, crate::error::KdlError> {
    let accretes = crate::kdl_parse::parse(src)?;
    let payload = if accretes.len() == 1 {
        serde_json::to_value(&accretes[0])?
    } else {
        serde_json::to_value(&accretes)?
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

/// Offline KDL -> accrete JSON trees (no content envelope).
pub fn parse_kdl_to_accretes(src: &str) -> Result<Vec<accrete::Accrete>, crate::error::KdlError> {
    crate::kdl_parse::parse(src)
}

/// Parse a YAML document as Content items (one item or an array — the
/// fluxora file shape: `action:/event:/method:/data:` heads) and wrap them
/// in a draw frame. Unlike KDL (layout-only, forced into Content::Create),
/// the action survives: set/join/tmpl files are first-class.
pub fn parse_yaml_to_frame(src: &str) -> Result<Value, crate::error::KdlError> {
    // serde_yaml deserializes straight into the typed Content<Accrete> — the
    // current accrete shape (children + tagged bind), NOT fluxora's retired
    // `sub:` form. One item or an array, mirroring the wire's OneOrMany.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ContentItems {
        One(content::Content<accrete::Accrete>),
        Many(Vec<content::Content<accrete::Accrete>>),
    }
    let items: ContentItems =
        serde_yaml::from_str(src).map_err(|e| crate::error::KdlError { msg: e.to_string() })?;
    // serde ignores unknown fields, so fluxora's retired `sub:` would be
    // silently DROPPED (empty subtree, no error) — refuse it explicitly.
    if let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(src) {
        if has_retired_key(&v) {
            return Err(crate::error::KdlError {
                msg: "retired fluxora key `sub:` found — migrate to `children:`".into(),
            });
        }
    }
    let content = match items {
        ContentItems::One(c) => vec![c],
        ContentItems::Many(v) => v,
    };
    if content.is_empty() {
        return Err(crate::error::KdlError {
            msg: "yaml document has no content items".into(),
        });
    }
    let msg = content::Message {
        ev: content::EV_DRAW.into(),
        sender: "stage".into(),
        created: None,
        content,
    };
    Ok(serde_json::to_value(&msg)?)
}

/// Recursive walk over the raw YAML tree for the retired `sub:` key.
fn has_retired_key(v: &serde_yaml::Value) -> bool {
    match v {
        serde_yaml::Value::Mapping(m) => m
            .iter()
            .any(|(k, val)| k.as_str() == Some("sub") || has_retired_key(val)),
        serde_yaml::Value::Sequence(s) => s.iter().any(has_retired_key),
        _ => false,
    }
}
