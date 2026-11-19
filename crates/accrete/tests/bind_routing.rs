//! ADR 0031 wire shape of one value: an address (`slot`), a closed `scope`
//! (`remote` | `local` | `both`), an optional `path` and `default`. `source`
//! and `sink` are the two directions of that same struct, and the retired
//! `kind` vocabulary must not decode.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use accrete::{Accrete, AccreteOps, Scope};
use serde_json::json;

fn bind(js: serde_json::Value) -> accrete::Bind {
    serde_json::from_value::<accrete::Bind>(js).unwrap()
}

#[test]
fn a_slot_carries_its_scope_and_optional_path() {
    let b = bind(json!({"slot": "page", "scope": "local"}));
    assert_eq!(b.slot.as_deref(), Some("page"));
    assert_eq!(b.scope, Scope::Local);
    assert_eq!(b.path, None);

    let b = bind(json!({"slot": "chan", "scope": "local", "path": "/id"}));
    assert_eq!(b.slot.as_deref(), Some("chan"));
    assert_eq!(b.path.as_deref(), Some("/id"));
}

#[test]
fn an_absent_scope_is_remote() {
    // The default is the side that owns the session: a frame that says nothing
    // means "an ordinary slot", not "a page-local one".
    let b = bind(json!({"slot": "notice"}));
    assert_eq!(b.scope, Scope::Remote);
    assert!(b.scope.remote() && !b.scope.local());
}

#[test]
fn scope_is_a_closed_enum() {
    assert!(serde_json::from_value::<accrete::Bind>(json!({
        "slot": "x", "scope": "nearby"
    }))
    .is_err());
}

#[test]
fn the_retired_kinds_do_not_decode() {
    // ADR 0031: the address and the mode are one field each; `kind` is gone
    // and an unknown key is an error, not a silently different value.
    assert!(serde_json::from_value::<accrete::Bind>(json!({
        "kind": "local", "slot": "page"
    }))
    .is_err());
    assert!(serde_json::from_value::<accrete::Bind>(json!({
        "kind": "default", "default": "hi"
    }))
    .is_err());
    assert!(serde_json::from_value::<accrete::Bind>(json!({
        "source": "chan"
    }))
    .is_err());
}

#[test]
fn no_slot_means_the_default_is_the_value() {
    // A constant is not a third channel: with no address, `default` *is* the value.
    let b = bind(json!({"default": "untitled"}));
    assert_eq!(b.slot, None);
    assert_eq!(b.default, Some(json!("untitled")));

    // With an address it is the fallback while the slot is still empty.
    let b = bind(json!({"slot": "draft", "default": "untitled"}));
    assert_eq!(b.slot.as_deref(), Some("draft"));
    assert_eq!(b.default, Some(json!("untitled")));
}

#[test]
fn get_at_walks_wire_shape() {
    let a: Accrete = serde_json::from_value(json!({
        "type": "frame",
        "children": [
            {"type": "text", "source": {"default": "hello"}}
        ]
    }))
    .unwrap();
    assert_eq!(
        a.get_at("/children/0/source/default").unwrap(),
        json!("hello")
    );
    // miss on a key skipped by skip_serializing_if is an error, not a panic
    assert!(a.get_at("/id").is_err());
    assert!(a.get_at("/children/9/source").is_err());
    assert!(a.get_at("nope").is_err());
}

#[test]
fn a_module_url_is_the_only_way_to_name_a_renderer() {
    // ADR 0007 的形态是唯一形态：种类住在数据里，框架不替任何渲染器选实现。
    // 原来的 chart/editor 默认 url 已移除——那正是"种类内置"的痕迹。
    let c: Accrete = serde_json::from_value(json!({
        "type": "canvas",
        "url": "/assets/g2chart/index.js",
        "source": {"default": {"type": "line"}}
    }))
    .unwrap();
    assert!(matches!(c, Accrete::canvas(_)));
    // 没有 url 的 canvas 是坏帧，不是"合法的默认"
    assert!(
        serde_json::from_value::<Accrete>(json!({"type": "canvas"})).is_err(),
        "a canvas without a module url must not decode"
    );
    // 旧词汇不再 decode：框架没有 chart 这个种类
    assert!(serde_json::from_value::<Accrete>(json!({"type": "chart"})).is_err());
    assert!(serde_json::from_value::<Accrete>(json!({"type": "editor"})).is_err());
    assert!(serde_json::from_value::<Accrete>(json!({"type": "diagram"})).is_err());
}

#[test]
fn a_select_says_read_write_and_options() {
    // Three channels on one node: the value in (`source`), the choice out
    // (`sink`), and the option rows (a named second channel).
    let a: Accrete = serde_json::from_value(json!({
        "type": "select",
        "source": {"slot": "page", "scope": "local"},
        "sink": {"slot": "page", "scope": "local"},
        "options": {"slot": "menu"}
    }))
    .unwrap();
    let Accrete::select(s) = &a else {
        panic!("expected select")
    };
    assert_eq!(s.get_source().unwrap().scope, Scope::Local);
    assert_eq!(s.get_sink().unwrap().slot.as_deref(), Some("page"));
    assert_eq!(s.options.as_ref().unwrap().slot.as_deref(), Some("menu"));

    let back = serde_json::to_value(&a).unwrap();
    // skip_serializing_if: absent path stays absent on the wire
    assert!(back["source"].get("path").is_none());
    assert_eq!(back, json!({
        "type": "select",
        "source": {"slot": "page", "scope": "local"},
        "sink": {"slot": "page", "scope": "local"},
        "options": {"slot": "menu"}
    }));
}

#[test]
fn a_value_can_be_local_now_and_mirrored_up() {
    // ADR 0031 §2: `both` is what the retired `report` said — assign in the
    // page and let the far side hear about it, without a third channel.
    let a: Accrete = serde_json::from_value(json!({
        "type": "select",
        "source": {"slot": "page", "default": "talk"},
        "options": {"slot": "nav"},
        "sink": {"slot": "island", "scope": "both"}
    }))
    .unwrap();
    let Accrete::select(s) = &a else {
        panic!("expected select")
    };
    let sink = s.get_sink().unwrap();
    assert_eq!(sink.slot.as_deref(), Some("island"));
    assert!(sink.scope.local() && sink.scope.remote());
    assert_eq!(s.source.as_ref().unwrap().default, Some(json!("talk")));
}