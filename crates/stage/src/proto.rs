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
        if let Some(msg) = dead_end_drop(&v) {
            return Err(crate::error::ParseError { msg });
        }
        if let Some(msg) = header_without_fields(&v) {
            return Err(crate::error::ParseError { msg });
        }
        if let Some(msg) = stepper_without_window(&v) {
            return Err(crate::error::ParseError { msg });
        }
        if let Some(msg) = retired_stage(&v) {
            return Err(crate::error::ParseError { msg });
        }
        if let Some(msg) = retired_header_rows(&v) {
            return Err(crate::error::ParseError { msg });
        }
        if let Some(msg) = retired_template_body(&v) {
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

/// A node without a write channel carrying `sink:` — serde would silently drop that key (most node structs take no unknown
/// fields, but there is no `deny_unknown_fields`, because once the internally tagged enum arms are tightened, schemars expands every arm
/// inline and the named types under `$defs` disappear with it, leaving the SDK generator to number them by order). So the "took a channel
/// it does not support" error is reported loudly here, rather than inside serde.
///
/// `input` and `select` are not in the list: they no longer have the field at all and refuse it in serde (ADR 0032 §2), so this
/// check would never see them get that far.
fn illegal_channel(v: &serde_yaml::Value) -> Option<String> {
    const WRITERS: &[&str] = &["button", "bundle", "frame"];
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

/// A drop that cannot emit is a dead end (ADR 0034): `accept` promises the box takes
/// cargo, and `sink` is the only way the landing says what arrived. Nothing downstream
/// can repair that, so it is refused where every other impossible frame is — at load,
/// next to `illegal_channel` (same reason: serde would take the key and drop it).
fn dead_end_drop(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Mapping(m) => {
            let accepts = m
                .get("attrs")
                .and_then(|a| a.get("accept"))
                .is_some_and(|a| !a.is_null());
            if accepts && !m.contains_key("sink") {
                let kind = m.get("type").and_then(|t| t.as_str()).unwrap_or("frame");
                return Some(format!(
                    "node `{kind}` has `attrs.accept` but no `sink` — a drop that cannot \
                     emit is a dead end; add `sink: {{slot: <event name or local slot>}}`"
                ));
            }
            m.iter().find_map(|(_, val)| dead_end_drop(val))
        }
        serde_yaml::Value::Sequence(s) => s.iter().find_map(dead_end_drop),
        _ => None,
    }
}

/// A header cell is derived from a field key (ADR 0031 §4): with no fields there is no key to
/// derive one from, so `attrs.header` on a `tiles` without `attrs.fields` can never render. Same
/// rule as `dead_end_drop` — refused at load, because serde would take the key and draw nothing.
fn header_without_fields(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Mapping(m) => {
            let kind = m.get("type").and_then(|t| t.as_str());
            let attrs = m.get("attrs");
            let headed = attrs
                .and_then(|a| a.get("header"))
                .is_some_and(|h| !h.is_null());
            // An empty field list is the same as none: a grid with no fields has no cells, so
            // the renderer treats `fields: []` as absent and so does this check.
            let fielded = attrs
                .and_then(|a| a.get("fields"))
                .and_then(|c| c.as_sequence())
                .is_some_and(|c| !c.is_empty());
            if kind == Some("tiles") && headed && !fielded {
                return Some(
                    "node `tiles` has `attrs.header` but no `attrs.fields` — a header cell is \
                     derived from a field key, so it can never render; add `attrs.fields: \
                     [{key: <entry field>}]`"
                        .into(),
                );
            }
            m.iter().find_map(|(_, val)| header_without_fields(val))
        }
        serde_yaml::Value::Sequence(s) => s.iter().find_map(header_without_fields),
        _ => None,
    }
}

/// A `step` moves a window and nothing else, so a stepper on a `tiles` that says no `window` is a
/// clock ticking for nobody: either the frame wants a window (`{size: 1}` for a page or a marquee)
/// or it does not want the step.
fn stepper_without_window(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Mapping(m) => {
            let kind = m.get("type").and_then(|t| t.as_str());
            let attrs = m.get("attrs");
            let stepped = attrs
                .and_then(|a| a.get("step"))
                .is_some_and(|s| !s.is_null());
            let windowed = attrs
                .and_then(|a| a.get("window"))
                .is_some_and(|w| !w.is_null());
            if kind == Some("tiles") && stepped && !windowed {
                return Some(
                    "node `tiles` has `attrs.step` but no `attrs.window` — a stepper only advances \
                     a window; add `attrs.window: {size: 1}` (one item on stage) or drop the step"
                        .into(),
                );
            }
            m.iter().find_map(|(_, val)| stepper_without_window(val))
        }
        serde_yaml::Value::Sequence(s) => s.iter().find_map(stepper_without_window),
        _ => None,
    }
}

/// `attrs.stage` was the count before ADR 0031's `window`: the attribute bag drops keys it does not
/// know, so a frame that still says `stage: one` would silently lay every item out. Refused at
/// load, with the spelling that replaced it.
fn retired_stage(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Mapping(m) => {
            let kind = m.get("type").and_then(|t| t.as_str());
            let staged = m
                .get("attrs")
                .and_then(|a| a.get("stage"))
                .is_some_and(|s| !s.is_null());
            if kind == Some("tiles") && staged {
                return Some(
                    "node `tiles` has `attrs.stage`, which is gone — the count of items on stage \
                     is `attrs.window: {size: N}` (`size: 1` is what `stage: one` meant)"
                        .into(),
                );
            }
            m.iter().find_map(|(_, val)| retired_stage(val))
        }
        serde_yaml::Value::Sequence(s) => s.iter().find_map(retired_stage),
        _ => None,
    }
}

/// `attrs.header.rows` was where the header's layers lived before `dir` could make the header a
/// column (ADR 0035). The attribute bag drops keys it does not know, so a frame that still says
/// `rows: 2` would silently render one layer — refused at load, with the spelling that replaced it.
fn retired_header_rows(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Mapping(m) => {
            let kind = m.get("type").and_then(|t| t.as_str());
            let rows = m
                .get("attrs")
                .and_then(|a| a.get("header"))
                .and_then(|h| h.get("rows"))
                .is_some_and(|r| !r.is_null());
            if kind == Some("tiles") && rows {
                return Some(
                    "node `tiles` has `attrs.header.rows`, which is gone — a header is not \
                     necessarily a row now (`dir: row` makes it a column), so the count of header \
                     layers is `attrs.header.layers: 1 | 2`"
                        .into(),
                );
            }
            m.iter().find_map(|(_, val)| retired_header_rows(val))
        }
        serde_yaml::Value::Sequence(s) => s.iter().find_map(retired_header_rows),
        _ => None,
    }
}

/// A `tmpl` whose body is still a string is the retired `{{key}}` form (ADR 0037): a
/// definition is the node tree itself now. serde would take the string and the renderer
/// would reject it at registration — refused here instead, with the spelling that replaced it.
fn retired_template_body(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Mapping(m) => {
            if m.get("action").and_then(|a| a.as_str()) == Some("tmpl")
                && m.get("data").is_some_and(|d| d.is_string())
            {
                return Some(
                    "a `tmpl` body is a node tree, not a `{{key}}` JSON string — write the \
                     tree under `data:` and put `\"$name\"` (the whole value) where a value \
                     goes; list the names it takes in `params:`"
                        .into(),
                );
            }
            m.iter().find_map(|(_, val)| retired_template_body(val))
        }
        serde_yaml::Value::Sequence(s) => s.iter().find_map(retired_template_body),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dead_end_drop, header_without_fields, illegal_channel, retired_header_rows, retired_stage,
        retired_template_body, stepper_without_window,
    };

    fn y(src: &str) -> serde_yaml::Value {
        serde_yaml::from_str(src).expect("yaml")
    }

    #[test]
    fn a_read_only_node_may_not_write() {
        let bad = y("type: text\nsink: {dest: x}\nsource: {default: hi}\n");
        assert!(illegal_channel(&bad).is_some(), "text 收了 sink 应当报错");

        // A field has no write channel at all any more (ADR 0032 §2), so the frame itself refuses the key.
        let field = y("type: input\nsink: {dest: x}\n");
        assert!(
            illegal_channel(&field).is_some(),
            "input 没有 sink，这个检查也要拒绝"
        );

        let ok = y("type: bundle\nsink: {dest: x}\nchildren: []\n");
        assert!(illegal_channel(&ok).is_none(), "bundle 可以写");
    }

    #[test]
    fn the_check_reaches_into_children() {
        let nested = y("type: frame\nchildren:\n- type: text\n  sink: {slot: y}\n");
        assert!(illegal_channel(&nested).is_some(), "子节点也要查");
    }

    #[test]
    fn a_drop_needs_a_sink() {
        let bad = y("type: frame\nattrs: {accept: {tag: note}}\n");
        let msg = dead_end_drop(&bad).expect("accept 没有 sink 应当报错");
        assert!(msg.contains("sink"), "错误要说该补什么：{msg}");

        let ok = y("type: frame\nattrs: {accept: {tag: note}}\nsink: {dest: pin}\n");
        assert!(dead_end_drop(&ok).is_none(), "落点带 sink 就成立");
    }

    #[test]
    fn the_drop_check_reaches_into_children() {
        let nested = y("type: frame\nchildren:\n- type: frame\n  attrs: {accept: {}}\n");
        assert!(dead_end_drop(&nested).is_some(), "子节点也要查");
    }

    #[test]
    fn a_header_needs_fields() {
        let bad = y("type: tiles\nattrs: {header: {selector: --head}}\n");
        let msg = header_without_fields(&bad).expect("有 header 没有 fields 应当报错");
        assert!(msg.contains("fields"), "错误要说该补什么：{msg}");

        // An empty field list is no fields: nothing to derive a header cell from.
        let empty = y("type: tiles\nattrs: {header: {selector: h}, fields: []}\n");
        assert!(
            header_without_fields(&empty).is_some(),
            "空列表也算没有字段"
        );

        let ok = y("type: tiles\nattrs: {header: {selector: h}, fields: [{key: qty}]}\n");
        assert!(header_without_fields(&ok).is_none(), "有字段才有表头");
    }

    #[test]
    fn a_stepper_needs_a_window() {
        let bad = y("type: tiles\nattrs: {step: {slot: page}}\n");
        let msg = stepper_without_window(&bad).expect("step 没有 window 应当报错");
        assert!(msg.contains("window"), "错误要说该补什么：{msg}");

        let ok = y("type: tiles\nattrs: {window: {size: 1}, step: {slot: page}}\n");
        assert!(
            stepper_without_window(&ok).is_none(),
            "窗口上的 stepper 成立"
        );

        // The checks are per node: a sibling's window does not license this node's stepper.
        let mixed = y("children:\n- type: tiles\n  attrs: {step: {timer: 100}}\n");
        assert!(stepper_without_window(&mixed).is_some(), "子节点也要查");
    }

    #[test]
    fn the_old_stage_key_is_refused_by_name() {
        let bad = y("type: tiles\nattrs: {stage: one}\n");
        let msg = retired_stage(&bad).expect("stage 应当报错");
        assert!(msg.contains("window"), "错误要说新拼法：{msg}");

        // `stage` as a VALUE is not this key: only a tiles node's `attrs.stage` is the old count.
        let slot = y("type: tiles\nattrs: {}\nsource: {slot: stage}\n");
        assert!(retired_stage(&slot).is_none(), "`slot: stage` 是值，不是键");
    }

    #[test]
    fn the_old_header_rows_key_is_refused_by_name() {
        let bad = y("type: tiles\nattrs: {header: {selector: h, rows: 2}, fields: [{key: q}]}\n");
        let msg = retired_header_rows(&bad).expect("header.rows 应当报错");
        assert!(msg.contains("layers"), "错误要说新拼法：{msg}");

        let ok = y("type: tiles\nattrs: {header: {selector: h, layers: 2}, fields: [{key: q}]}\n");
        assert!(retired_header_rows(&ok).is_none(), "layers 是现在的拼法");
    }

    #[test]
    fn a_template_body_must_be_a_tree() {
        let bad = y("action: tmpl\nname: segment\ndata: \"{\\\"type\\\": \\\"text\\\"}\"\n");
        let msg = retired_template_body(&bad).expect("字符串本体应当报错");
        assert!(msg.contains("node tree"), "错误要说定义是什么：{msg}");
        assert!(msg.contains("$name"), "错误要说新拼法：{msg}");

        let ok = y(
            "action: tmpl\nname: segment\nparams: [q]\ndata:\n  type: text\n  source: {default: \"$q\"}\n",
        );
        assert!(retired_template_body(&ok).is_none(), "树本体成立");
    }
}
