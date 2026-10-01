//! ADR 0008 wire shapes: `local` kind (slot = 槽名), optional `path`
//! on both source/local, `Target` retired (unknown kinds must fail),
//! and the read-side pointer `get_at` on the WIRE shape.

use accrete::{Accrete, BindVariant};
use serde_json::json;

fn bind(js: serde_json::Value) -> BindVariant {
    serde_json::from_value::<accrete::Bind>(js).unwrap().variant
}

#[test]
fn local_kind_carries_slot_and_optional_path() {
    match bind(json!({"kind": "local", "slot": "page"})) {
        BindVariant::Local { slot, path } => {
            assert_eq!(slot, "page");
            assert_eq!(path, None);
        }
        other => panic!("expected Local, got {other:?}"),
    }
    match bind(json!({"kind": "local", "slot": "chan", "path": "/id"})) {
        BindVariant::Local { slot, path } => {
            assert_eq!(slot, "chan");
            assert_eq!(path.as_deref(), Some("/id"));
        }
        other => panic!("expected Local, got {other:?}"),
    }
}

#[test]
fn source_kind_gains_optional_path() {
    match bind(json!({"kind": "source", "source": "chan"})) {
        BindVariant::Source { source, path } => {
            assert_eq!(source, "chan");
            assert_eq!(path, None);
        }
        other => panic!("expected Source, got {other:?}"),
    }
}

#[test]
fn target_variant_is_retired() {
    // ADR 0008: the vague `Target` kind is deleted, not revived.
    assert!(serde_json::from_value::<accrete::Bind>(json!(
        {"kind": "target", "target": "x"}
    ))
    .is_err());
}

#[test]
fn get_at_walks_wire_shape() {
    let a: Accrete = serde_json::from_value(json!({
        "type": "case",
        "children": [
            {"type": "text", "bind": {"value": {"kind": "default", "default": "hello"}}}
        ]
    }))
    .unwrap();
    assert_eq!(
        a.get_at("/children/0/bind/value/default").unwrap(),
        json!("hello")
    );
    // miss on a key skipped by skip_serializing_if is an error, not a panic
    assert!(a.get_at("/id").is_err());
    assert!(a.get_at("/children/9/bind").is_err());
    assert!(a.get_at("nope").is_err());
}

#[test]
fn local_roundtrips_through_serde() {
    let a: Accrete = serde_json::from_value(json!({
        "type": "select",
        "bind": {
            "value": {"kind": "local", "slot": "page"},
            "options": {"kind": "source", "source": "menu"}
        }
    }))
    .unwrap();
    let back = serde_json::to_value(&a).unwrap();
    assert_eq!(back["bind"]["value"]["kind"], json!("local"));
    assert_eq!(back["bind"]["value"]["slot"], json!("page"));
    // skip_serializing_if: absent path stays absent on the wire
    assert!(back["bind"]["value"].get("path").is_none());
}
