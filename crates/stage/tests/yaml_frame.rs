//! YAML carrier tests: /send shape contract (Content item or array, action
//! preserved end-to-end). What stage produces *is* the wire: the action list,
//! no envelope (ADR 0021 decision 6).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use serde_json::Value;

/// What goes on the socket: the action list, no envelope.
fn wire(src: &str) -> Value {
    let frame = stage::proto::parse_yaml_to_frame(src).unwrap();
    serde_json::to_value(&frame).unwrap()
}

#[test]
fn yaml_single_content_item_keeps_action() {
    // ADR 0005 carrier shape: append head; row id lives in the DATA
    let src = r#"
action: append
slot: chat
data:
  type: text
  id: m1
  bind:
    value:
      kind: default
      default: hi
"#;
    // the file's single item is one element of the list — the list is the wire
    let frame = wire(src);
    assert!(frame.is_array(), "一帧就是动作列表");
    assert_eq!(frame[0]["action"], "append");
    assert_eq!(frame[0]["slot"], "chat");
    assert_eq!(frame[0]["data"]["type"], "text");
    assert_eq!(frame[0]["data"]["id"], "m1");
}

#[test]
fn yaml_patch_item_roundtrips() {
    let src = r#"
action: patch
slot: chat
id: a1
path: /bind/value/default
op: append
value: " tok"
"#;
    let frame = wire(src);
    assert_eq!(frame[0]["action"], "patch");
    assert_eq!(frame[0]["path"], "/bind/value/default");
    assert_eq!(frame[0]["op"], "append");
    assert_eq!(frame[0]["value"], " tok");
}

#[test]
fn yaml_remove_item_roundtrips() {
    let src = r#"
action: remove
slot: chat
id: a1
"#;
    let frame = wire(src);
    assert_eq!(frame[0]["action"], "remove");
    assert_eq!(frame[0]["id"], "a1");
}

#[test]
fn yaml_array_of_content_items() {
    let src = r#"
- action: create
  slot: stage
  data:
    type: text
    id: a
- action: set
  slot: case
  data:
    type: text
    id: b
"#;
    let content = wire(src);
    let content = content.as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["action"], "create");
    assert_eq!(content[1]["action"], "set");
}

#[test]
fn yaml_rejects_retired_sub_shape() {
    // fluxora's retired `sub:` must NOT deserialize (children is the model)
    let src = r#"
action: create
slot: stage
data:
  type: case
  sub:
  - type: text
"#;
    assert!(stage::proto::parse_yaml_to_frame(src).is_err());
}

#[test]
fn the_wire_decodes_as_the_renderers_frame_type() {
    // 端到端的那一段尾巴：stage 发出去的字节，必须就是渲染器解开的那一帧
    // （ADR 0021 决策 6）。用真的示例文件，走 JSON 与 CBOR 两条路。
    let src = std::fs::read_to_string("../../examples/yaml/00.main.yaml").unwrap();
    let frame = stage::proto::parse_yaml_to_frame(&src).unwrap();
    let wire = frame.as_slice();
    let json_bytes = serde_json::to_vec(&wire).unwrap();
    let frame: Vec<content::Content<accrete::Accrete>> = content::codec::ActiveCodec::Json
        .decode_auto(&json_bytes)
        .expect("json wire decodes");
    assert!(!frame.is_empty());
    let cbor_bytes = content::codec::ActiveCodec::Cbor.encode(&wire).unwrap();
    let frame: Vec<content::Content<accrete::Accrete>> = content::codec::ActiveCodec::Cbor
        .decode_auto(&cbor_bytes)
        .expect("cbor wire decodes");
    assert!(!frame.is_empty());
}

#[test]
fn yaml_rejects_empty_document() {
    assert!(stage::proto::parse_yaml_to_frame("").is_err());
    assert!(stage::proto::parse_yaml_to_frame("[]").is_err());
}