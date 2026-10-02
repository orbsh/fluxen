//! ADR 0008 wire shapes: `local` kind (slot = 槽名), optional `path`
//! on both source/local, `Target` retired (unknown kinds must fail),
//! and the read-side pointer `get_at` on the WIRE shape.

use accrete::{Accrete, AccreteOps, BindVariant};
use serde_json::json;

fn bind(js: serde_json::Value) -> BindVariant {
    serde_json::from_value::<accrete::Bind>(js).unwrap().variant
}

#[test]
fn local_kind_carries_slot_and_optional_path() {
    match bind(json!({"kind": "local", "slot": "page"})) {
        BindVariant::Local { slot, path, event } => {
            assert_eq!(slot, "page");
            assert_eq!(path, None);
            assert_eq!(event, None);
        }
        other => panic!("expected Local, got {other:?}"),
    }
    match bind(json!({"kind": "local", "slot": "chan", "path": "/id"})) {
        BindVariant::Local { slot, path, event } => {
            assert_eq!(slot, "chan");
            assert_eq!(path.as_deref(), Some("/id"));
            assert_eq!(event, None);
        }
        other => panic!("expected Local, got {other:?}"),
    }
}

#[test]
fn local_kind_gains_optional_event_name() {
    // ADR 0010: emitter-side channel name; absent = slot name.
    match bind(json!({"kind": "local", "slot": "page", "event": "channel::select"})) {
        BindVariant::Local { slot, event, .. } => {
            assert_eq!(slot, "page");
            assert_eq!(event.as_deref(), Some("channel::select"));
        }
        other => panic!("expected Local, got {other:?}"),
    }
    // roundtrip: explicit event serializes back; absent stays absent
    let v = json!({"kind": "local", "slot": "p", "event": "e"});
    let b = bind(v.clone());
    assert_eq!(serde_json::to_value(&b).unwrap(), v);
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
fn pages_variant_decodes_with_display_default() {
    // ADR 0010: wire tag `pages`; `display` absent = render (serde default).
    let p: accrete::Pages = serde_json::from_value(json!({
        "id": "pager",
        "bind": {
            "value": { "kind": "source", "source": "pages" },
            "select": { "kind": "local", "slot": "page", "path": "/data" }
        }
    }))
    .unwrap();
    assert_eq!(p.display, accrete::PagesDisplay::Render);
    // explicit dom mode
    let p2: accrete::Pages =
        serde_json::from_value(json!({ "display": "dom" })).unwrap();
    assert_eq!(p2.display, accrete::PagesDisplay::Dom);
    // enum tag on the Accrete level round-trips
    let a: accrete::Accrete = serde_json::from_value(json!({ "type": "pages" })).unwrap();
    // stringify! on the match path yields spaced tokens (proc-macro behavior)
    assert!(a.get_type().contains("pages"));
    assert!(matches!(a, accrete::Accrete::pages(_)));
}

#[test]
fn chart_url_has_backward_compatible_default() {
    // ADR 0009: old frames without `url` still decode (serde default).
    let c: accrete::Chart = serde_json::from_value(json!({
        "bind": {"value": {"kind": "default", "default": {"type": "line"}}}
    }))
    .unwrap();
    assert_eq!(c.url, "/assets/g2chart/index.js");
    // explicit url wins
    let c2: accrete::Chart =
        serde_json::from_value(json!({"url": "https://cdn/g2chart/index.js"})).unwrap();
    assert_eq!(c2.url, "https://cdn/g2chart/index.js");
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
