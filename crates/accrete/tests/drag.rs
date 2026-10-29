//! ADR 0019 wire shapes: `grab` (the source) and `dock` (the landing side).
//! Both are ordinary internal-tagged nodes. The source is a plain wrapper that
//! carries a `kind` tag and one `payload`; the landing side owns the event name
//! and which kinds it accepts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use accrete::{Accrete, DockExpand};
use serde_json::json;

#[test]
fn grab_carries_a_kind_tag_and_one_payload() {
    let wire = json!({
        "type": "grab",
        "kind": "message",
        "payload": { "item": "m42" }
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    match &node {
        Accrete::grab(g) => {
            assert_eq!(g.kind.as_deref(), Some("message"));
            assert_eq!(g.payload, Some(json!({ "item": "m42" })));
            // 源上没有 id、没有绑定、没有 attrs：货物与标签都是字面值，
            // 身份在 payload 里，落点在对面。
            assert_eq!(g.children, None);
        }
        other => panic!("expected grab, got {other:?}"),
    }
    // 往返：形状不漂移（Attr/Bind 加字段后的既定门槛，新增类型同样适用）。
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn grab_may_be_a_bare_wrapper() {
    // 只有 children 的极小形状也必须解析得出来（没有 kind、没有 payload）。
    let wire = json!({
        "type": "grab",
        "children": [{ "type": "text", "bind": { "value": { "kind": "default", "default": "卡片" } } }]
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    match &node {
        Accrete::grab(g) => {
            assert_eq!(g.children.as_ref().map(Vec::len), Some(1));
            assert_eq!(g.kind, None);
            assert_eq!(g.payload, None);
        }
        other => panic!("expected grab, got {other:?}"),
    }
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn dock_owns_the_event_name_and_the_landing_id() {
    let wire = json!({
        "type": "dock",
        "id": "rail",
        "bind": { "value": { "kind": "event", "event": "pin" } }
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    match &node {
        Accrete::dock(d) => {
            assert_eq!(d.id.as_deref(), Some("rail"));
            // 落点由事件名 + 帧的 id（dock 自己）确定；不另带元数据。
            let value = d.bind.as_ref().unwrap().get("value").unwrap();
            assert!(matches!(
                &value.variant,
                accrete::BindVariant::Event { event } if event == "pin"
            ));
        }
        other => panic!("expected dock, got {other:?}"),
    }
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn dock_expand_defaults_to_reveal() {
    // 缺省 = reveal（常态细条，拖拽在飞时展开）；显式 reserved 才是提前占位。
    let bare: Accrete = serde_json::from_value(json!({
        "type": "dock",
        "attrs": {}
    }))
    .unwrap();
    match &bare {
        Accrete::dock(d) => {
            assert_eq!(d.attrs.as_ref().unwrap().expand, DockExpand::Reveal);
        }
        other => panic!("expected dock, got {other:?}"),
    }

    let reserved: Accrete = serde_json::from_value(json!({
        "type": "dock",
        "attrs": { "expand": "reserved", "class": ["rail"] }
    }))
    .unwrap();
    match &reserved {
        Accrete::dock(d) => {
            let attrs = d.attrs.as_ref().unwrap();
            assert_eq!(attrs.expand, DockExpand::Reserved);
            assert_eq!(attrs.class.as_ref().map(Vec::len), Some(1));
        }
        other => panic!("expected dock, got {other:?}"),
    }
    // 枚举是闭的：域外取值必须解不出来，而不是静默落回默认。
    assert!(
        serde_json::from_value::<Accrete>(json!({
            "type": "dock",
            "attrs": { "expand": "sometimes" }
        }))
        .is_err()
    );
}

#[test]
fn dock_accepts_a_closed_set_of_kinds_or_all_of_them() {
    // 缺席 = 全收（只有一个落点时不必写）；写了就是闭集合比对。
    let open: Accrete = serde_json::from_value(json!({ "type": "dock" })).unwrap();
    match &open {
        Accrete::dock(d) => assert_eq!(d.attrs.as_ref().and_then(|a| a.accept.as_ref()), None),
        other => panic!("expected dock, got {other:?}"),
    }

    let wire = json!({
        "type": "dock",
        "attrs": { "accept": ["message", "patch"] }
    });
    let narrow: Accrete = serde_json::from_value(wire.clone()).unwrap();
    match &narrow {
        Accrete::dock(d) => assert_eq!(
            d.attrs.as_ref().unwrap().accept.as_ref().map(Vec::len),
            Some(2)
        ),
        other => panic!("expected dock, got {other:?}"),
    }
    assert_eq!(serde_json::to_value(&narrow).unwrap(), wire);
}

#[test]
fn dock_children_are_what_was_dropped() {
    let wire = json!({
        "type": "dock",
        "children": [{ "type": "text", "bind": { "value": { "kind": "default", "default": "已钉住" } } }]
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    match &node {
        Accrete::dock(d) => assert_eq!(d.children.as_ref().map(Vec::len), Some(1)),
        other => panic!("expected dock, got {other:?}"),
    }
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}