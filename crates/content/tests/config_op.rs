//! ADR 0038 §2: the `config` op — options by name, merge not replace, and the
//! renderer keeps the two halves. Decoding is all this crate owns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use content::Content;
use serde_json::{Value, json};

#[test]
fn a_config_frame_carries_its_members_by_name() {
    let c: Content<Value> =
        serde_json::from_value(json!({"action": "config", "data": {"skin": "sketch"}})).unwrap();
    let Content::Config(op) = c else {
        panic!("not a config: {c:?}");
    };
    assert_eq!(op.data.get("skin"), Some(&json!("sketch")));
}

#[test]
fn several_members_land_in_one_frame() {
    let c: Content<Value> = serde_json::from_value(
        json!({"action": "config", "data": {"skin": "sketch", "density": "compact"}}),
    )
    .unwrap();
    let Content::Config(op) = c else {
        panic!("not a config: {c:?}");
    };
    assert_eq!(op.data.len(), 2);
}

#[test]
fn a_null_member_is_a_member() {
    // Clearing is the renderer's reading of "null"; on the wire it is just a member value.
    let c: Content<Value> =
        serde_json::from_value(json!({"action": "config", "data": {"skin": null}})).unwrap();
    let Content::Config(op) = c else {
        panic!("not a config: {c:?}");
    };
    assert_eq!(op.data.get("skin"), Some(&Value::Null));
}

#[test]
fn an_empty_object_is_a_valid_frame_that_changes_nothing() {
    let c: Content<Value> = serde_json::from_value(json!({"action": "config", "data": {}})).unwrap();
    let Content::Config(op) = c else {
        panic!("not a config: {c:?}");
    };
    assert!(op.data.is_empty());
}

#[test]
fn a_frame_without_data_is_refused() {
    // No data is not the same as "no change": that is a producer bug, so fail loudly as ADR 0003 says.
    let r: Result<Content<Value>, _> = serde_json::from_value(json!({"action": "config"}));
    assert!(r.is_err());
}

#[test]
fn a_non_object_payload_is_refused() {
    let r: Result<Content<Value>, _> =
        serde_json::from_value(json!({"action": "config", "data": ["skin"]}));
    assert!(r.is_err());
}

#[test]
fn the_action_name_is_on_the_wire() {
    let c: Content<Value> =
        serde_json::from_value(json!({"action": "config", "data": {"skin": "sketch"}})).unwrap();
    assert_eq!(
        serde_json::to_value(&c).unwrap()["action"],
        json!("config")
    );
}
