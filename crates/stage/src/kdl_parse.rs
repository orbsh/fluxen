//! KDL → Brick (v1) decoding.
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

use crate::error::KdlError;
use brick::Brick;
use kdl::parse_document;
use kdl::KdlNode;
use kdl::KdlValue;
use serde_json::{json, Map, Value};

/// Scalar conversion shared by kdl decoding.
pub trait KdlScalarExt {
    fn to_json(&self) -> Value;
}

impl KdlScalarExt for KdlValue {
    fn to_json(&self) -> Value {
        match self {
            KdlValue::Null => Value::Null,
            KdlValue::Boolean(b) => Value::Bool(*b),
            KdlValue::Int(i) => json!(i),
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

pub fn parse(src: &str) -> Result<Vec<Brick>, KdlError> {
    let nodes = parse_document(src).map_err(|e| KdlError { msg: e.to_string() })?;
    nodes.iter().map(brick_of).collect()
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

fn brick_value(node: &KdlNode) -> Result<Value, KdlError> {
    let name = node.name.as_str();

    let mut fields = Map::new();
    let mut attrs = Map::new();

    for (k, v) in &node.properties {
        if NODE_FIELDS.contains(&k.as_str()) {
            fields.insert(k.clone(), kdl_scalar(v));
        } else if is_attr_field(k) {
            if k == "class" {
                // class="a b" → Vec<String>
                let list: Vec<Value> = match v {
                    KdlValue::String(s) => s.split_whitespace().map(|c| json!(c)).collect(),
                    other => vec![kdl_scalar(other)],
                };
                attrs.insert("class".into(), Value::Array(list));
            } else {
                attrs.insert(k.clone(), kdl_scalar(v));
            }
        } else {
            return err(format!("unknown property `{k}` on `{name}`"));
        }
    }

    // child blocks
    let mut children: Vec<Value> = Vec::new();
    let mut item: Vec<Value> = Vec::new();
    let mut binds: Map<String, Value> = Map::new();
    for child in &node.children {
        match child.name.as_str() {
            "bind" => {
                let key = child
                    .values
                    .first()
                    .and_then(|v| match v {
                        KdlValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| KdlError {
                        msg: "bind requires a key: bind \"click\" { event \"toggle\" }".into(),
                    })?;
                binds.insert(key, bind_value(child)?);
            }
            "item" => {
                for sub in &child.children {
                    item.push(brick_value(sub)?);
                }
            }
            // attrs block: entries are value-arg nodes `class "a b"` /
            // `format "md"`. (Properties inside child blocks are rejected by
            // the kdl 2.0 parser, and `key=value` at node position is invalid
            // KDL — a node must start with a name.)
            "attrs" => {
                for e in &child.children {
                    let key = e.name.as_str();
                    let val = e.values.first().map(kdl_scalar).unwrap_or(Value::Null);
                    if key == "class" {
                        let list: Vec<Value> = match val {
                            Value::String(s) => {
                                s.split_whitespace().map(|c| json!(c)).collect()
                            }
                            other => vec![other],
                        };
                        attrs.insert("class".into(), Value::Array(list));
                    } else if is_attr_field(key) {
                        attrs.insert(key.to_string(), val);
                    } else {
                        return err(format!("unknown attr `{key}` on `{name}`"));
                    }
                }
            }
            "style" | "grid" | "data" => {
                // map block: each child node is one entry `key value`
                // (`key=value` at node position is invalid KDL — a node must
                // start with a name — so entries are value args).
                let mut m = Map::new();
                for e in &child.children {
                    let key = e.name.to_string();
                    let val = e
                        .values
                        .first()
                        .map(kdl_scalar)
                        .unwrap_or(Value::Null);
                    m.insert(key, val);
                }
                match child.name.as_str() {
                    "style" => attrs.insert("style".into(), Value::Object(m)),
                    "grid" => attrs.insert("grid".into(), Value::Object(m)),
                    _ => {
                        for (k, v) in m {
                            fields.insert(format!("data_{k}"), v);
                        }
                        None
                    }
                };
            }
            _ => children.push(brick_value(child)?),
        }
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

    // bind <key> { <kind> "arg" ... default "..." }
    // kind node: name = kind (source/target/event/field/submit), value arg =
    // its string; `default` child holds the default value.
    let mut kind_fields: Option<(String, Map<String, Value>)> = None;
    let mut default: Option<Value> = None;
    for c in &node.children {
        let cname = c.name.as_str();
        if cname == "default" {
            default = c.values.first().map(kdl_scalar);
            continue;
        }
        let mut kf = Map::new();
        if let Some(v) = c.values.first() {
            kf.insert(cname.to_string(), kdl_scalar(v));
        }
        for (k, v) in &c.properties {
            kf.insert(k.clone(), kdl_scalar(v));
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
