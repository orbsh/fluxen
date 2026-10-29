//! Wire payloads between CLI and mirror. The mirror never inspects payloads;
//! it routes raw bytes between peers. What crosses the socket is the **action
//! list** — the shape `content::Content<Accrete>` defines, and the only thing
//! the renderer decodes (ADR 0021 decision 6).
//!
//! The envelope below is **stage's own**: it moved out of `content` because a
//! host that wants `ev` / `sender` / `created` wraps its own frames, and stage
//! is one such host (prism and reflux are the others, each their own). Nothing
//! here is sent to a renderer — `Message::wire` is what the page gets.
//!
//! KDL was retired (2026-10-02, user ruling: maintain YAML/JSON only — the
//! hand-maintained KDL layer kept drifting from the wire shape: unlisted
//! fields dropped or 400'd, layout wraps forced into `Content::Create`
//! with a stray event name, examples silently diverging from the YAML
//! originals they mirrored).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 渲染类帧的操作名（stage 的操作通道只剩这一种取值）。
pub const EV_DRAW: &str = "draw";

/// stage 的信封（ADR 0021 决策 6）。`ev` 恒为 `EV_DRAW`、`sender` 恒为 `"stage"`，
/// 因为 stage 目前没有自己的动词；字段留在这里是因为信封的形状由宿主定，而 stage
/// 是对端（`/cli`）的对话方。渲染器从不看它——它拿到的是 [`Message::wire`]。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message<T> {
    pub ev: String,
    pub sender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<Value>,
    pub content: Vec<content::Content<T>>,
}

impl<T: Serialize> Message<T> {
    /// 发给渲染器的那一半：动作列表本身，没有信封。
    pub fn wire(&self) -> &[content::Content<T>] {
        &self.content
    }
}

/// Parse a YAML document as Content items (one item or an array — the
/// fluxora file shape: `action:/event:/data:` heads) and wrap them
/// in a draw frame. The action survives: create/set/append/patch/remove/tmpl
/// files are all first-class.
pub fn parse_yaml_to_frame(src: &str) -> Result<Message<accrete::Accrete>, crate::error::ParseError> {
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
    Ok(Message {
        ev: EV_DRAW.into(),
        sender: "stage".into(),
        created: None,
        content,
    })
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
