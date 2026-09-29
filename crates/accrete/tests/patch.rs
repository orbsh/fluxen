//! Patch semantics: JSON Pointer (RFC 6901) addressing in the Value domain.
//!
//! Locks the ADR 0005 contract: patches address wire field names (the shape
//! external producers see), `replace_at` swaps the pointed value, `append_at`
//! extends strings / pushes into arrays, and any write that violates the
//! Accrete wire type is rejected atomically (node unchanged).

use accrete::{Accrete, AccreteOps};
use serde_json::{Value, json};

fn accrete(js: Value) -> Accrete {
    serde_json::from_value(js).expect("valid accrete json")
}

fn row() -> Accrete {
    accrete(json!({
        "type": "text", "id": "a1",
        "bind": { "value": { "kind": "default", "default": "hello" } }
    }))
}

fn default_of(b: &Accrete) -> Value {
    b.get_bind()
        .and_then(|m| m.get("value"))
        .and_then(|x| x.default.clone())
        .unwrap()
}

#[test]
fn replace_deep_value() {
    let mut b = row();
    b.replace_at("/bind/value/default", json!("bye")).expect("patch");
    assert_eq!(default_of(&b), json!("bye"));
}

#[test]
fn replace_attrs_field() {
    let mut b = accrete(json!({
        "type": "case",
        "attrs": { "class": ["a"], "horizontal": false },
        "children": [ { "type": "text", "attrs": { "format": "md" } } ]
    }));
    // attrs class swap — impossible under the old positional merge
    b.replace_at("/attrs/class", json!(["x", "y"]))
        .expect("patch attrs");
    let v = serde_json::to_value(&b).unwrap();
    assert_eq!(v["attrs"]["class"], json!(["x", "y"]), "class replaced");
    // deep child path: address into a child subtree
    b.replace_at("/children/0/attrs/format", json!("html"))
        .expect("patch deep");
    let v = serde_json::to_value(&b).unwrap();
    assert_eq!(v["children"][0]["attrs"]["format"], json!("html"));
}

#[test]
fn append_string_value() {
    let mut b = row();
    b.append_at("/bind/value/default", json!(", world"))
        .expect("append token");
    assert_eq!(default_of(&b), json!("hello, world"));
}

#[test]
fn append_array_value() {
    let mut b = accrete(json!({
        "type": "select",
        "bind": { "options": { "kind": "default", "default": ["a"] } }
    }));
    b.append_at("/bind/options/default", json!("b"))
        .expect("append into array");
    assert_eq!(
        b.get_bind().unwrap()["options"].default.clone().unwrap(),
        json!(["a", "b"])
    );
}

#[test]
fn path_miss_is_error() {
    let mut b = row();
    assert!(b.replace_at("/bind/nope/default", json!(1)).is_err());
    assert!(b.replace_at("/children/0", json!(1)).is_err());
    // node untouched after misses
    assert_eq!(default_of(&b), json!("hello"));
}

#[test]
fn type_violation_rolls_back() {
    // attrs.class is Vec<String> — a non-array must fail deserialize and
    // leave the node exactly as it was (atomicity via the type system).
    let mut b = accrete(json!({
        "type": "case",
        "attrs": { "class": ["keep"] },
    }));
    assert!(
        b.replace_at("/attrs/class", json!("not-an-array"))
            .is_err()
    );
    let v = serde_json::to_value(&b).unwrap();
    assert_eq!(v["attrs"]["class"], json!(["keep"]));
}

#[test]
fn patching_the_row_id() {
    let mut b = row();
    b.replace_at("/id", json!("a2")).expect("id patch");
    assert_eq!(b.get_id().as_deref(), Some("a2"));
}
