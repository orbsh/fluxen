//! ADR 0034: cargo lives on the frame. `attrs.draggable` is the source, `attrs.accept`
//! plus `sink` the landing, and the cargo itself is the frame's own subtree — no
//! `kind`/`payload` bag to keep in step with the children.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

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
        f.attrs.as_ref().and_then(|a| a.draggable.as_ref()).and_then(|d| d.tag.as_deref()),
        Some("message")
    );
    // 货物就是这棵子树本身：没有第二份 payload。
    assert_eq!(f.children.as_ref().map(Vec::len), Some(1));
    // 往返：形状不漂移。
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn a_draggable_frame_may_be_untagged() {
    // 没有 tag 的货：不限定的落点照收，写了 tag 的落点不收。
    let wire = json!({
        "type": "frame",
        "attrs": { "draggable": {} },
        "children": [{ "type": "text", "source": { "default": "卡片" } }]
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    let f = frame(node.clone());
    assert_eq!(f.attrs.as_ref().and_then(|a| a.draggable.as_ref()).and_then(|d| d.tag.clone()), None);
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn the_old_payload_bag_is_refused() {
    // `kind`/`payload` 是 `grab` 的死键：货物是子树，不是第二个袋子，所以它们现在
    // 是解码错误（而不是被静默丢掉后留下一个空盒子）。
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
        "sink": { "slot": "pin" }
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    let f = frame(node.clone());
    assert_eq!(
        f.attrs.as_ref().and_then(|a| a.accept.as_ref()).and_then(|a| a.tag.as_deref()),
        Some("message")
    );
    // 落点由上行槽名 + 帧的 id 确定；不另带元数据。drop 是值流出，所以这条通道是
    // `sink`（ADR 0031 §2）。
    assert_eq!(f.get_sink().and_then(|b| b.slot.as_deref()), Some("pin"));
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn accept_without_a_tag_takes_any_cargo() {
    // 缺席 = 全收（只有一个落点时不必写）；写了就是闭集合比对。
    let wire = json!({ "type": "frame", "attrs": { "accept": {} }, "sink": { "slot": "pin" } });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    let f = frame(node.clone());
    assert_eq!(
        f.attrs.as_ref().and_then(|a| a.accept.as_ref()).and_then(|a| a.tag.clone()),
        None
    );
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}

#[test]
fn a_frame_is_both_ends_and_reads_a_slot() {
    // 同一个节点三条通道并存：读（source）、发（sink）、被拖（draggable）。
    let wire = json!({
        "type": "frame",
        "id": "slot1",
        "attrs": { "draggable": { "tag": "snippet" }, "accept": {}, "class": ["box"] },
        "source": { "slot": "inbox" },
        "sink": { "slot": "pin" },
        "children": []
    });
    let node: Accrete = serde_json::from_value(wire.clone()).unwrap();
    let f = frame(node.clone());
    assert_eq!(f.get_source().and_then(|b| b.slot.as_deref()), Some("inbox"));
    assert_eq!(f.get_sink().and_then(|b| b.slot.as_deref()), Some("pin"));
    assert_eq!(serde_json::to_value(&node).unwrap(), wire);
}