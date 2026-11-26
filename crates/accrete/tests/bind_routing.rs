//! ADR 0031 wire shape of one value, split by direction (ADR 0039): a read is a `Source`
//! (`slot`, `scope`, `path`, `default`), a write is a `Sink` (`dest`, `scope`, `path`). The retired `kind`
//! vocabulary must not decode, and neither direction may spell the other's address.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn
)]

use accrete::{Accrete, Scope};
use serde_json::json;

fn source(js: serde_json::Value) -> accrete::Source {
    serde_json::from_value::<accrete::Source>(js).unwrap()
}

fn sink(js: serde_json::Value) -> accrete::Sink {
    serde_json::from_value::<accrete::Sink>(js).unwrap()
}

#[test]
fn a_slot_carries_its_scope_and_optional_path() {
    let b = source(json!({"slot": "page", "scope": "local"}));
    assert_eq!(b.slot.as_deref(), Some("page"));
    assert_eq!(b.scope, Scope::Local);
    assert_eq!(b.path, None);

    let b = source(json!({"slot": "chan", "scope": "local", "path": "/id"}));
    assert_eq!(b.slot.as_deref(), Some("chan"));
    assert_eq!(b.path.as_deref(), Some("/id"));
}

#[test]
fn an_absent_scope_is_remote() {
    // The default is the side that owns the session: a frame that says nothing
    // means "an ordinary slot", not "a page-local one".
    let b = source(json!({"slot": "notice"}));
    assert_eq!(b.scope, Scope::Remote);
    assert!(b.scope.remote() && !b.scope.local());
}

#[test]
fn scope_is_a_closed_enum() {
    assert!(
        serde_json::from_value::<accrete::Source>(json!({
            "slot": "x", "scope": "nearby"
        }))
        .is_err()
    );
}

#[test]
fn the_retired_kinds_do_not_decode() {
    // ADR 0031: the address and the mode are one field each; `kind` is gone
    // and an unknown key is an error, not a silently different value.
    assert!(
        serde_json::from_value::<accrete::Source>(json!({
            "kind": "local", "slot": "page"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<accrete::Source>(json!({
            "kind": "default", "default": "hi"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<accrete::Source>(json!({
            "source": "chan"
        }))
        .is_err()
    );
}

#[test]
fn no_slot_means_the_default_is_the_value() {
    // A constant is not a third channel: with no address, `default` *is* the value.
    let b = source(json!({"default": "untitled"}));
    assert_eq!(b.slot, None);
    assert_eq!(b.default, Some(json!("untitled")));

    // With an address it is the fallback while the slot is still empty.
    let b = source(json!({"slot": "draft", "default": "untitled"}));
    assert_eq!(b.slot.as_deref(), Some("draft"));
    assert_eq!(b.default, Some(json!("untitled")));
}

#[test]
fn a_sink_names_a_dest_and_has_no_default() {
    // ADR 0023 chose `dest` for an uplink target and the outflow carries the same word, so a write says the
    // same thing the frame says. A write has no use for `default` — the shared struct made it expressible and
    // nothing read it, so it is gone with the struct.
    let s = sink(json!({"dest": "message", "scope": "both"}));
    assert_eq!(s.dest.as_deref(), Some("message"));
    assert!(s.scope.local() && s.scope.remote());

    assert!(
        serde_json::from_value::<accrete::Sink>(json!({"dest": "x", "default": 1})).is_err(),
        "a sink must not carry a default"
    );
    assert!(
        serde_json::from_value::<accrete::Sink>(json!({"slot": "x"})).is_err(),
        "a sink addresses a dest, not a slot"
    );
    // A sink without a dest is legal and means "no address"; a button no longer needs one at all
    // (ADR 0039 §2 amended) — the enclosing loom's `submit` names the timing.
    let marker = sink(json!({}));
    assert_eq!(marker.dest, None);
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
    // The ADR 0007 shape is the only shape: the kind lives in the data, the framework picks an implementation for no renderer.
    // The old default urls for chart/editor are removed — those were exactly the traces of "kinds are built in".
    let c: Accrete = serde_json::from_value(json!({
        "type": "canvas",
        "url": "/assets/g2chart/index.js",
        "source": {"default": {"type": "line"}}
    }))
    .unwrap();
    assert!(matches!(c, Accrete::canvas(_)));
    // A canvas without a url is a bad frame, not "a legal default"
    assert!(
        serde_json::from_value::<Accrete>(json!({"type": "canvas"})).is_err(),
        "a canvas without a module url must not decode"
    );
    // The old vocabulary no longer decodes: the framework has no chart kind
    assert!(serde_json::from_value::<Accrete>(json!({"type": "chart"})).is_err());
    assert!(serde_json::from_value::<Accrete>(json!({"type": "editor"})).is_err());
    assert!(serde_json::from_value::<Accrete>(json!({"type": "diagram"})).is_err());
}

#[test]
fn a_picker_says_read_and_options() {
    // Two channels on one node: the value in (`source`) and the option rows (a named second channel).
    // The choice does not leave through the picker: that is the enclosing loom's business, and the
    // schema no longer has a shape to spell it (ADR 0032 §2).
    let a: Accrete = serde_json::from_value(json!({
        "type": "picker",
        "source": {"slot": "page", "scope": "local"},
        "options": {"slot": "menu"}
    }))
    .unwrap();
    let Accrete::picker(s) = &a else {
        panic!("expected picker")
    };
    assert_eq!(s.source.as_ref().unwrap().scope, Scope::Local);
    assert_eq!(s.options.as_ref().unwrap().slot.as_deref(), Some("menu"));

    let back = serde_json::to_value(&a).unwrap();
    // skip_serializing_if: absent path stays absent on the wire
    assert!(back["source"].get("path").is_none());
    assert_eq!(
        back,
        json!({
            "type": "picker",
            "source": {"slot": "page", "scope": "local"},
            "options": {"slot": "menu"}
        })
    );
    // The dead shape does not decode: a picker with its own sink is gone, not ignored.
    assert!(
        serde_json::from_value::<Accrete>(json!({
            "type": "picker",
            "source": {"slot": "page", "scope": "local"},
            "sink": {"dest": "page", "scope": "local"}
        }))
        .is_err(),
        "a field's sink must not decode"
    );
}

#[test]
fn a_value_can_be_local_now_and_mirrored_up() {
    // ADR 0031 §2: `both` is what the retired `report` said — assign in the
    // page and let the far side hear about it, without a third channel.
    let a: Accrete = serde_json::from_value(json!({
        "type": "loom",
        "sink": {"dest": "island", "scope": "both"},
        "children": [{
            "type": "picker",
            "tag": "page",
            "source": {"slot": "page", "default": "talk"},
            "options": {"slot": "nav"}
        }]
    }))
    .unwrap();
    let Accrete::loom(b) = &a else {
        panic!("expected loom")
    };
    let sink = b.sink.as_ref().unwrap();
    assert_eq!(sink.dest.as_deref(), Some("island"));
    assert!(sink.scope.local() && sink.scope.remote());
    let Accrete::picker(s) = &b.children.as_ref().unwrap()[0] else {
        panic!("expected picker")
    };
    assert_eq!(s.source.as_ref().unwrap().default, Some(json!("talk")));
}
