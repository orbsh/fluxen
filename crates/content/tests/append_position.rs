//! A row's landing spot: 0 = the end (default); a positive number counts inward from the **tail** (+1 = before the last one, +len = the head);
//! a negative number counts inward from the **head** (-1 = the head, -len = before the last one).
//!
//! The direction is deliberate: the index counts inward from **this operation's own anchor end** — append is anchored at the tail, so positive numbers walk toward the head;
//! "an insert anchored at the head" has the opposite sign (consistent with it, direction opposite). Out of domain is **refused**, not clamped: clamping would put the row
//! where the caller did not ask for, which is exactly a silent downgrade.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use content::{AppendOp, Content, append_index};
use serde_json::{Value, json};

/// A frame is the action list itself (ADR 0021 decision 6) — this layer of tests only cares about append's wire shape.
fn frame(position: i64) -> Value {
    let actions: Vec<Content<Value>> = vec![Content::Append(AppendOp {
        slot: "rows".into(),
        position,
        data: json!({"id": "r1"}),
    })];
    serde_json::to_value(&actions).unwrap()
}

fn decoded_position(value: Value) -> i64 {
    let back: Vec<Content<Value>> = serde_json::from_value(value).unwrap();
    match &back[0] {
        Content::Append(op) => op.position,
        other => panic!("expected an append, got {other:?}"),
    }
}

#[test]
fn the_tail_stays_off_the_wire() {
    // Absent means 0 (the end), so when the position carries no information the field should not be in the frame.
    let item = &frame(0)[0];
    assert_eq!(item["action"], json!("append"));
    assert!(item.get("position").is_none(), "省缺即末尾，不写");
    assert_eq!(decoded_position(frame(0)), 0);
}

#[test]
fn a_position_that_carries_information_is_written_and_decodes() {
    assert_eq!(frame(3)[0]["position"], 3);
    assert_eq!(decoded_position(frame(3)), 3);
    assert_eq!(decoded_position(json!([
        {"action": "append", "slot": "rows", "position": -1, "data": {"id": "r2"}}
    ])), -1);
}

#[test]
fn the_landing_table() {
    // With len = 4 (A B C D), which array index each position lands at
    assert_eq!(append_index(4, 0), Some(4), "末尾");
    assert_eq!(append_index(4, 1), Some(3), "最后一个之前");
    assert_eq!(append_index(4, 2), Some(2));
    assert_eq!(append_index(4, 4), Some(0), "头部");
    assert_eq!(append_index(4, -1), Some(0), "头部（与 +4 同义）");
    assert_eq!(append_index(4, -2), Some(1));
    assert_eq!(append_index(4, -4), Some(3), "最后一个之前（与 +1 同义）");
}

#[test]
fn out_of_range_is_refused_not_clamped() {
    assert_eq!(append_index(4, 5), None, "+len+1 及以上");
    assert_eq!(append_index(4, -5), None, "-len-1 及以下");
    assert_eq!(append_index(0, 0), Some(0), "空列表只有一种落点");
    assert_eq!(append_index(0, 1), None);
    assert_eq!(append_index(0, -1), None);
    assert_eq!(append_index(4, i64::MIN), None, "取负不能溢出");
}