//! YAML carrier tests: /send?fmt=yaml shape contract (Content item or array,
//! action preserved — NOT forced to create like the KDL path).

#[test]
fn yaml_single_content_item_keeps_action() {
    // fluxora file shape: one Content head (action:/event:/method:/data:)
    let src = r#"
action: join
method: concat
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
    // OneOrMany: single item collapses to an object; action is join (not create)
    assert_eq!(frame["content"]["action"], "join");
    assert_eq!(frame["content"]["method"], "concat");
    assert_eq!(frame["content"]["event"], "chat");
    assert_eq!(frame["content"]["data"]["type"], "text");
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
