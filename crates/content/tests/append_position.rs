//! 行的落点：0 = 末尾（默认）；正数从**尾**往内数（+1 = 最后一个之前，+len = 头部）；
//! 负数从**头**往内数（-1 = 头部，-len = 最后一个之前）。
//!
//! 方向是刻意的：索引从**本操作自己的锚定端**往内数——append 锚在尾，正数因此往头走；
//! 「从头部锚定的插入」正负相反（与它一致，方向相反）。域外**拒收**，不夹取：夹取会把行
//! 放到调用方没要求的位置，正是静默降级。
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use content::{AppendOp, Content, Message, append_index};
use serde_json::{Value, json};

fn frame(position: i64) -> Value {
    let msg = Message {
        ev: "draw".into(),
        sender: "t".into(),
        created: None,
        content: vec![Content::Append(AppendOp {
            event: "rows".into(),
            position,
            data: json!({"id": "r1"}),
        })],
    };
    serde_json::to_value(&msg).unwrap()
}

fn decoded_position(value: Value) -> i64 {
    let back: Message<Value> = serde_json::from_value(value).unwrap();
    match &back.content[0] {
        Content::Append(op) => op.position,
        other => panic!("expected an append, got {other:?}"),
    }
}

#[test]
fn the_tail_stays_off_the_wire() {
    // 省缺即 0（末尾），所以位置没有信息时帧里就不该有这个字段。
    // 注意 `content` 是 OneOrMany：单条会塌成一个对象，索引的是对象本身。
    let item = &frame(0)["content"];
    assert!(item.is_object(), "单条 content 在线上塌成对象");
    assert!(item.get("position").is_none(), "省缺即末尾，不写");
    assert_eq!(decoded_position(frame(0)), 0);
}

#[test]
fn a_position_that_carries_information_is_written_and_decodes() {
    assert_eq!(frame(3)["content"]["position"], 3);
    assert_eq!(decoded_position(frame(3)), 3);
    assert_eq!(decoded_position(json!({
        "ev": "draw", "sender": "t",
        "content": [{"action": "append", "event": "rows", "position": -1, "data": {"id": "r2"}}],
    })), -1);
}

#[test]
fn the_landing_table() {
    // len = 4（A B C D）时，每个 position 落在哪个数组下标
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