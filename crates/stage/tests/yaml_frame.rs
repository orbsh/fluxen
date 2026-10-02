//! YAML carrier tests: /send shape contract (Content item or array,
//! action preserved end-to-end).

#[test]
fn yaml_single_content_item_keeps_action() {
    // ADR 0005 carrier shape: append head; row id lives in the DATA
    let src = r#"
action: append
event: chat
data:
  type: text
  id: m1
  bind:
    value:
      kind: default
      default: hi
"#;
    let frame = stage::proto::parse_yaml_to_frame(src).unwrap();
    assert_eq!(frame["ev"], "draw");
    assert_eq!(frame["sender"], "stage");
    // OneOrMany: single item collapses to an object; action is append (not create)
    assert_eq!(frame["content"]["action"], "append");
    assert_eq!(frame["content"]["event"], "chat");
    assert_eq!(frame["content"]["data"]["type"], "text");
    assert_eq!(frame["content"]["data"]["id"], "m1");
}

#[test]
fn yaml_patch_item_roundtrips() {
    let src = r#"
action: patch
event: chat
id: a1
path: /bind/value/default
op: append
value: " tok"
"#;
    let frame = stage::proto::parse_yaml_to_frame(src).unwrap();
    assert_eq!(frame["content"]["action"], "patch");
    assert_eq!(frame["content"]["path"], "/bind/value/default");
    assert_eq!(frame["content"]["op"], "append");
    assert_eq!(frame["content"]["value"], " tok");
}

#[test]
fn yaml_remove_item_roundtrips() {
    let src = r#"
action: remove
event: chat
id: a1
"#;
    let frame = stage::proto::parse_yaml_to_frame(src).unwrap();
    assert_eq!(frame["content"]["action"], "remove");
    assert_eq!(frame["content"]["id"], "a1");
}

#[test]
fn yaml_array_of_content_items() {
    let src = r#"
- action: create
  event: stage
  data:
    type: text
    id: a
- action: set
  event: case
  data:
    type: text
    id: b
"#;
    let frame = stage::proto::parse_yaml_to_frame(src).unwrap();
    let content = frame["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["action"], "create");
    assert_eq!(content[1]["action"], "set");
}

#[test]
fn yaml_rejects_retired_sub_shape() {
    // fluxora's retired `sub:` must NOT deserialize (children is the model)
    let src = r#"
action: create
event: stage
data:
  type: case
  sub:
  - type: text
"#;
    assert!(stage::proto::parse_yaml_to_frame(src).is_err());
}

#[test]
fn yaml_rejects_empty_document() {
    assert!(stage::proto::parse_yaml_to_frame("").is_err());
    assert!(stage::proto::parse_yaml_to_frame("[]").is_err());
}
