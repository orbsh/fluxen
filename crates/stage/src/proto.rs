//! Wire payloads between CLI and mirror. The mirror never inspects payloads;
//! it routes raw bytes between peers. So CLI sends a BARE Message<Accrete> —
//! exactly what the UI's decoder expects.
//!
//! KDL was retired (2026-10-02, user ruling: maintain YAML/JSON only — the
//! hand-maintained KDL layer kept drifting from the wire shape: unlisted
//! fields dropped or 400'd, layout wraps forced into `Content::Create`
//! with a stray event name, examples silently diverging from the YAML
//! originals they mirrored).

use serde::Deserialize;
use serde_json::Value;

/// Parse a YAML document as Content items (one item or an array — the
/// fluxora file shape: `action:/event:/data:` heads) and wrap them
/// in a draw frame. The action survives: create/set/append/patch/remove/tmpl
/// files are all first-class.
pub fn parse_yaml_to_frame(src: &str) -> Result<Value, crate::error::ParseError> {
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
        serde_yaml::from_str(src).map_err(|e| crate::error::ParseError { msg: e.to_string() })?;
    // serde ignores unknown fields, so fluxora's retired `sub:` would be
    // silently DROPPED (empty subtree, no error) — refuse it explicitly.
    if let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(src) {
        if has_retired_key(&v) {
            return Err(crate::error::ParseError {
                msg: "retired fluxora key `sub:` found — migrate to `children:`".into(),
            });
        }
    }
    let content = match items {
        ContentItems::One(c) => vec![c],
        ContentItems::Many(v) => v,
    };
    if content.is_empty() {
        return Err(crate::error::ParseError {
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
