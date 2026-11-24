//! ADR 0034: cargo lives on the frame. `attrs.draggable` is the source, `attrs.accept`
//! plus `sink` the landing, and the cargo itself is the frame's own subtree — no
//! `kind`/`payload` bag to keep in step with the children.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn
)]

use accrete::{Accrete, AccreteOps};
use serde_json::json;

fn frame(node: Accrete) -> accrete::Frame {
    match node {
        Accrete::frame(f) => f,
        other => panic!("expected frame, got {other:?}"),
    }
}

#[test]
fn a_draggable_frame_owns_its_cargo() {
    let wire = json!({
        "type": "frame",
        "id": "m42",
        "attrs": { "draggable": { "tag": "message" } },
        "children": [{ "type": "text", "source": { "default": "卡片" } }]
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    let f = frame(node.clone());
    assert_eq!(f.id.as_deref(), Some("m42"));
    assert_eq!(
        f.attrs
            .as_ref()
            .and_then(|a| a.draggable.as_ref())
            .and_then(|d| d.tag.as_deref()),
        Some("message")
    );
    // The cargo is this subtree itself: there is no second payload.
    assert_eq!(f.children.as_ref().map(Vec::len), Some(1));
    // Round trip: the shape does not drift.
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn a_draggable_frame_may_be_untagged() {
    // Cargo with no tag: an unqualified landing takes it, a landing with a tag does not.
    let wire = json!({
        "type": "frame",
        "attrs": { "draggable": {} },
        "children": [{ "type": "text", "source": { "default": "卡片" } }]
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    let f = frame(node.clone());
    assert_eq!(
        f.attrs
            .as_ref()
            .and_then(|a| a.draggable.as_ref())
            .and_then(|d| d.tag.clone()),
        None
    );
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn the_old_payload_bag_is_refused() {
    // `kind`/`payload` are `grab`'s dead keys: cargo is a subtree, not a second bag, so they are now
    // decode errors (instead of being silently dropped and leaving an empty box behind).
    assert!(
        serde_json::from_value::<Accrete>(json!({
            "type": "frame",
            "attrs": { "draggable": { "kind": "message", "payload": { "item": "m42" } } }
        }))
        .is_err()
    );
}

#[test]
fn the_landing_owns_the_dest_and_its_own_id() {
    let wire = json!({
        "type": "frame",
        "id": "rail",
        "attrs": { "accept": { "tag": "message" } },
        "sink": { "dest": "pin" }
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    let f = frame(node.clone());
    assert_eq!(
        f.attrs
            .as_ref()
            .and_then(|a| a.accept.as_ref())
            .and_then(|a| a.tag.as_deref()),
        Some("message")
    );
    // The dest is determined by the uplink slot name + the frame's id; no extra metadata is carried. A drop is a value flowing out, so this channel is
    // `sink` (ADR 0031 §2).
    assert_eq!(f.get_sink().and_then(|b| b.dest.as_deref()), Some("pin"));
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn accept_without_a_tag_takes_any_cargo() {
    // Absent = take everything (no need to write it when there is only one landing); written means a closed-set comparison.
    let wire = json!({ "type": "frame", "attrs": { "accept": {} }, "sink": { "dest": "pin" } });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    let f = frame(node.clone());
    assert_eq!(
        f.attrs
            .as_ref()
            .and_then(|a| a.accept.as_ref())
            .and_then(|a| a.tag.clone()),
        None
    );
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn a_frame_is_both_ends_and_reads_a_slot() {
    // Three channels coexist on one node: read (source), send (sink), being dragged (draggable).
    let wire = json!({
        "type": "frame",
        "id": "slot1",
        "attrs": { "draggable": { "tag": "snippet" }, "accept": {}, "class": ["box"] },
        "source": { "slot": "inbox" },
        "sink": { "dest": "pin" },
        "children": []
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    let f = frame(node.clone());
    assert_eq!(
        f.get_source().and_then(|b| b.slot.as_deref()),
        Some("inbox")
    );
    assert_eq!(f.get_sink().and_then(|b| b.dest.as_deref()), Some("pin"));
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}
