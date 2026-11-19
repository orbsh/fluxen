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
        if let Some(msg) = illegal_channel(&v) {
            return Err(crate::error::ParseError { msg });
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

/// 只读的节点带了 `sink:` —— serde 会静默丢掉这个键（`Bind` 之外的节点不收未知字段，
/// 但没有 `deny_unknown_fields`，因为内部标记的枚举臂一旦收紧，schemars 就把每个臂
/// 内联展开，`$defs` 的具名类型随之消失，SDK 生成器只能按顺序编号）。所以"接了不支持的
/// 通道"这条错误在这里响亮地报，而不是在 serde 里。
fn illegal_channel(v: &serde_yaml::Value) -> Option<String> {
    const WRITERS: &[&str] = &["input", "select", "button", "bundle", "dock"];
    match v {
        serde_yaml::Value::Mapping(m) => {
            let kind = m.get("type").and_then(|t| t.as_str());
            if let Some(kind) = kind {
                if m.contains_key("sink") && !WRITERS.contains(&kind) {
                    return Some(format!(
                        "node `{kind}` has no `sink` channel — it only reads; \
                         move the value into `source:` or use a node that writes"
                    ));
                }
            }
            m.iter().find_map(|(_, val)| illegal_channel(val))
        }
        serde_yaml::Value::Sequence(s) => s.iter().find_map(illegal_channel),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::illegal_channel;

    fn y(src: &str) -> serde_yaml::Value {
        serde_yaml::from_str(src).expect("yaml")
    }

    #[test]
    fn a_read_only_node_may_not_write() {
        let bad = y("type: text\nsink: {slot: x}\nsource: {default: hi}\n");
        assert!(illegal_channel(&bad).is_some(), "text 收了 sink 应当报错");

        let ok = y("type: input\nsink: {slot: x}\n");
        assert!(illegal_channel(&ok).is_none(), "input 可以写");
    }

    #[test]
    fn the_check_reaches_into_children() {
        let nested = y("type: frame\nchildren:\n- type: text\n  sink: {slot: y}\n");
        assert!(illegal_channel(&nested).is_some(), "子节点也要查");
    }
}
