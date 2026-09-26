//! KDL (v2) → Brick decoding.
//!
//! Backend: the official `kdl` crate (6.x, `KdlDocument::parse` — real KDL v2:
//! `#true`/`#false`/`#null` typed literals, `key=value` properties on nodes).
//! nushell 0.115's `from kdl` ships the same implementation, so grammar
//! behaviour here matches `nu` one-for-one.
//!
//! Brick serializes with `#[serde(tag = "type")]`: the wire form is an object
//! with a `type` field plus the variant struct's fields. This module builds
//! exactly that shape as serde_json::Value, then lets serde produce the Brick.
//!
//! Mapping convention:
//! 1. node name        → brick type (e.g. `fold`, `text`)
//! 2. entry args       → plain values (bind key, template name)
//! 3. properties       → scalar fields (id, class, format, ...)
//! 4. child blocks     → `children`, `bind`, `item`, `style`/`grid`/`data` maps
//!    (a child-block entry like `key="v"` parses as a *node named key with a
//!    value argument*, not a property — the kdl crate rejects real properties
//!    inside child blocks; same behaviour as nushell)

use crate::error::KdlError;
use brick::Brick;
use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use serde_json::{json, Map, Value};

/// Scalar conversion shared by kdl decoding.
pub trait KdlScalarExt {
    fn to_json(&self) -> Value;
}

impl KdlScalarExt for KdlValue {
    fn to_json(&self) -> Value {
        match self {
            KdlValue::Null => Value::Null,
            KdlValue::Bool(b) => Value::Bool(*b),
            KdlValue::Integer(i) => serde_json::Number::from_i128(*i)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            KdlValue::Float(f) => serde_json::Number::from_f64(*f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            KdlValue::String(s) => Value::String(s.clone()),
        }
    }
}

pub(crate) fn kdl_scalar(v: &KdlValue) -> Value {
    v.to_json()
}

fn entry_scalar(e: &KdlEntry) -> Value {
    kdl_scalar(e.value())
}

/// Positional (unnamed) entries of a node.
fn entry_args(node: &KdlNode) -> Vec<&KdlEntry> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .collect()
}

/// Named (property) entries of a node, as (name, value) pairs.
fn entry_props(node: &KdlNode) -> Vec<(String, Value)> {
    node.entries()
        .iter()
        .filter_map(|e| e.name().map(|n| (n.value().to_string(), entry_scalar(e))))
        .collect()
}

pub fn parse(src: &str) -> Result<Vec<Brick>, KdlError> {
    let doc = KdlDocument::parse(src).map_err(|e| KdlError { msg: e.to_string() })?;
    doc.nodes().iter().map(brick_of).collect()
}

fn err<T>(msg: impl Into<String>) -> Result<T, KdlError> {
    Err(KdlError { msg: msg.into() })
}

/// Build the tagged wire object: {"type": name, ...fields}
fn node_obj(name: &str, fields: Map<String, Value>) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), json!(name));
    m.extend(fields);
    Value::Object(m)
}

fn brick_of(node: &KdlNode) -> Result<Brick, KdlError> {
    let v = serde_json::from_value::<Brick>(brick_value(node)?)?;
    Ok(v)
}

/// Properties that belong to the node itself, not to its attrs struct.
const NODE_FIELDS: &[&str] = &["id", "type", "kind"];

/// attr field names by target struct — everything listed goes into `attrs`;
/// unknown properties are rejected instead of silently dropped.
fn is_attr_field(name: &str) -> bool {
    matches!(
        name,
        "class"
            | "selector"
            | "height"
            | "width"
            | "left"
            | "right"
            | "top"
            | "bottom"
            | "direction"
            | "replace_header"
            | "float_body"
            | "instant"
            | "scroll"
            | "oneshot"
            | "desc"
            | "thumb"
            | "format"
            | "horizontal"
            | "style"
            | "grid"
    )
}

fn insert_attr(attrs: &mut Map<String, Value>, key: &str, val: Value) -> Result<(), KdlError> {
    if key == "class" {
        // class="a b" → Vec<String>
        let list: Vec<Value> = match val {
            Value::String(ref s) => s.split_whitespace().map(|c| json!(c)).collect(),
            other => vec![other],
        };
        attrs.insert("class".into(), Value::Array(list));
        Ok(())
    } else if is_attr_field(key) {
        attrs.insert(key.to_string(), val);
        Ok(())
    } else {
        err(format!("unknown attr `{key}`"))
    }
}

fn brick_value(node: &KdlNode) -> Result<Value, KdlError> {
    let name = node.name().value();

    let mut fields = Map::new();
    let mut attrs = Map::new();

    for (k, v) in entry_props(node) {
        if NODE_FIELDS.contains(&k.as_str()) {
            fields.insert(k, v);
        } else {
            insert_attr(&mut attrs, &k, v)?;
        }
    }

    // child blocks
    let mut children: Vec<Value> = Vec::new();
    let mut item: Vec<Value> = Vec::new();
    let mut binds: Map<String, Value> = Map::new();
    let mut template_data: Option<Map<String, Value>> = None;
    let doc = node.children().cloned().unwrap_or_default();
    for child in doc.nodes() {
        match child.name().value() {
            "bind" => {
                let key = entry_args(child)
                    .first()
                    .copied()
                    .and_then(|v| match v.value() {
                        KdlValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| KdlError {
                        msg: "bind requires a key: bind \"click\" { event \"toggle\" }".into(),
                    })?;
                binds.insert(key, bind_value(child)?);
            }
            "item" => {
                let idoc = child.children().cloned().unwrap_or_default();
                for sub in idoc.nodes() {
                    item.push(brick_value(sub)?);
                }
            }
            // attrs block: entries are value-arg nodes `class "a b"` /
            // `format "md"` (a real `key=value` property inside a child block
            // is rejected by the kdl crate itself — see module docs).
            "attrs" => {
                let adoc = child.children().cloned().unwrap_or_default();
                for e in adoc.nodes() {
                    let key = e.name().value();
                    let val = entry_args(e)
                        .first()
                        .copied()
                        .map(entry_scalar)
                        .unwrap_or(Value::Null);
                    insert_attr(&mut attrs, key, val).map_err(|_: KdlError| KdlError {
                        msg: format!("unknown attr `{key}` on `{name}`"),
                    })?;
                }
            }
            "style" | "grid" | "data" => {
                // map block: each child node is one entry `key value`
                let mut m = Map::new();
                let mdoc = child.children().cloned().unwrap_or_default();
                for e in mdoc.nodes() {
                    let key = e.name().value().to_string();
                    let val = entry_args(e)
                        .first()
                        .copied()
                        .map(entry_scalar)
                        .unwrap_or(Value::Null);
                    m.insert(key, val);
                }
                match child.name().value() {
                    "style" => attrs.insert("style".into(), Value::Object(m)),
                    "grid" => attrs.insert("grid".into(), Value::Object(m)),
                    // `data` is brick::Template's map (minijinja context), not
                    // a data_* routing other bricks use — that routing never
                    // existed in brick and is retired with this parser.
                    _ => {
                        template_data = Some(m);
                        None
                    }
                };
            }
            _ => children.push(brick_value(child)?),
        }
    }

    // brick::Template { name, data }: name is the first positional arg,
    // data comes from the `data` child block (empty by default — serde
    // rejects a missing `data` field).
    if name == "template" {
        if let Some(first) = entry_args(node).first() {
            fields.insert("name".into(), entry_scalar(first));
        }
        fields.insert(
            "data".into(),
            Value::Object(template_data.unwrap_or_default()),
        );
    }

    if !binds.is_empty() {
        fields.insert("bind".into(), Value::Object(binds));
    }
    if !children.is_empty() {
        fields.insert("children".into(), Value::Array(children));
    }
    if !item.is_empty() {
        fields.insert("item".into(), Value::Array(item));
    }
    if !attrs.is_empty() {
        fields.insert("attrs".into(), Value::Object(attrs));
    }

    Ok(node_obj(name, fields))
}

/// bind <key> { <kind> "arg" ... default "..." }
/// Bind serializes flattened: the variant fields sit directly on the bind object.
fn bind_value(node: &KdlNode) -> Result<Value, KdlError> {
    let mut m = Map::new();

    // kind node: name = kind (source/target/event/field/submit), value arg =
    // its string; `default` child holds the default value; a `field` kind may
    // carry a `payload { ... }` child block (flattened payload map for the
    // BindVariant::Field variant).
    let mut kind_fields: Option<(String, Map<String, Value>)> = None;
    let mut default: Option<Value> = None;
    let doc = node.children().cloned().unwrap_or_default();
    for c in doc.nodes() {
        let cname = c.name().value();
        if cname == "default" {
            default = entry_args(c).first().copied().map(entry_scalar);
            continue;
        }
        let mut kf = Map::new();
        if let Some(v) = entry_args(c).first() {
            kf.insert(cname.to_string(), entry_scalar(v));
        }
        for (k, v) in entry_props(c) {
            kf.insert(k, v);
        }
        if cname == "field" {
            // payload sub-block: `payload { k "v" ... }` → kf["payload"] = {...}
            let cdoc = c.children().cloned().unwrap_or_default();
            for p in cdoc.nodes() {
                if p.name().value() == "payload" {
                    let mut pm = Map::new();
                    let pd = p.children().cloned().unwrap_or_default();
                    for e in pd.nodes() {
                        let val = entry_args(e)
                            .first()
                            .copied()
                            .map(entry_scalar)
                            .unwrap_or(Value::Null);
                        pm.insert(e.name().value().to_string(), val);
                    }
                    for (k, v) in entry_props(p) {
                        pm.insert(k, v);
                    }
                    kf.insert("payload".into(), Value::Object(pm));
                }
            }
        }
        if kind_fields.is_some() {
            return err("bind accepts exactly one kind node");
        }
        kind_fields = Some((cname.to_string(), kf));
    }

    match kind_fields {
        Some((k, kf)) => {
            m.insert("kind".into(), json!(k));
            m.extend(kf);
        }
        None => {
            m.insert("kind".into(), json!("default"));
        }
    }
    if let Some(d) = default {
        m.insert("default".into(), d);
    }
    Ok(Value::Object(m))
}
