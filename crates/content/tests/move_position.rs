//! A moved row's landing spot: the same vocabulary `append` uses — `0` = the end, a positive number
//! counts inward from the **tail**, a negative one inward from the **head** — but counted on the list
//! **with the row lifted out** (ADR 0005).
//!
//! The lift is the whole point. Counted before the row came out, "the last row to the front" and
//! "the last row to the last place" differ by one, and both readings are defensible in prose; counted
//! after, the number describes the list the reader is looking at once the row has left. That also
//! makes the domain one position narrower than `append`'s, which sees the list as it is.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn
)]

use content::{Content, MoveOp, move_index};
use serde_json::{Value, json};

fn frame(position: i64) -> Value {
    let actions: Vec<Content<Value>> = vec![Content::Move(MoveOp {
        slot: "rows".into(),
        id: "r1".into(),
        position,
    })];
    serde_json::to_value(&actions).unwrap()
}

#[test]
fn the_frame_says_which_row_and_where() {
    let item = &frame(-1)[0];
    assert_eq!(item["action"], json!("move"));
    assert_eq!(item["slot"], json!("rows"));
    assert_eq!(item["id"], json!("r1"));
    assert_eq!(item["position"], json!(-1));
    // `0` is written too, unlike `append`: there the default *is* the end, while here the end is one
    // of several places a row can be sent, and an absent field would read as "no opinion".
    assert_eq!(frame(0)[0]["position"], json!(0));
}

#[test]
fn a_frame_without_a_position_is_refused() {
    // Unlike `append`, there is no default place: `0` (= the end) is one of several places a row can
    // be sent, not the absence of an opinion, so a frame that names none is not a move.
    let decoded = serde_json::from_value::<Vec<Content<Value>>>(json!([
        {"action": "move", "slot": "rows", "id": "r1"}
    ]));
    assert!(decoded.is_err(), "a move has to say where the row goes");
}

#[test]
fn the_landing_table_counts_the_lifted_list() {
    // Ten rows, moving one: nine places remain. A B C D E F G H I J with J lifted is A..I.
    assert_eq!(move_index(10, -1), Some(0), "最后一行挪到最前");
    assert_eq!(move_index(10, 0), Some(9), "第一行挪到最后");
    assert_eq!(move_index(10, 1), Some(8), "倒数第二个位置之前");
    assert_eq!(move_index(10, 9), Some(0), "头部，与 -1 同义");
    assert_eq!(move_index(10, -9), Some(8), "最后一个位置之前，与 +1 同义");
}

#[test]
fn out_of_range_is_refused_not_clamped() {
    assert_eq!(move_index(10, 10), None, "被拎出来之后只有九个位置");
    assert_eq!(move_index(10, -10), None);
    assert_eq!(move_index(2, 1), Some(0), "两行之间互换是域内的");
    assert_eq!(move_index(2, 2), None);
    // No row, no move: the row must be in the list, so a zero-row list has no landing spot at all.
    assert_eq!(move_index(0, 0), None);
    assert_eq!(move_index(0, -1), None);
    assert_eq!(move_index(10, i64::MIN), None, "取负不能溢出");
}
