//! Wire payloads between CLI and mirror. The mirror never inspects payloads;
//! it routes raw bytes between peers. What crosses the socket is the **action
//! list** — the shape `content::Content<Accrete>` defines, and the only thing
//! the renderer decodes (ADR 0021 decision 6).
//!
//! stage has **no envelope**: it has one verb (draw), so an `ev` would only
//! restate that, a `sender` would be a field no reader has, and `created` was
//! wrote by nobody since it appeared. A host with real verbs of its own (prism:
//! `connected`, `init.error`, `echo.result`) wraps its own frames; this one
//! sends the list.
//!
//! KDL was retired (2026-10-02, user ruling: maintain YAML/JSON only — the
//! hand-maintained KDL layer kept drifting from the wire shape: unlisted
//! fields dropped or 400'd, layout wraps forced into `Content::Create`
//! with a stray event name, examples silently diverging from the YAML
//! originals they mirrored).

use serde::Deserialize;

/// Parse a YAML document into the frame itself: the action list (one item or an
/// array — the fluxora file shape: `action:/event:/data:` heads). The action
/// survives: create/set/append/patch/remove/tmpl files are all first-class.
pub fn parse_yaml_to_frame(
    src: &str,
) -> Result<Vec<content::Content<accrete::Accrete>>, crate::error::ParseError> {
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
    Ok(content)
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
