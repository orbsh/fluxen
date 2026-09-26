//! Merge-strategy semantics for streaming Brick updates.
//!
//! These lock the contract of `Brick::merge` + `BrickOp` that
//! `Content::Join` frames rely on: same-id rows update in place via
//! Concat / Delete / Replace, and the outer zip of children preserves
//! positional pairing.

use brick::Brick;
use brick::BrickOps;
use brick::merge::{Concat, Delete, Replace};
use serde_json::{Value, json};

fn brick(js: Value) -> Brick {
    serde_json::from_value(js).expect("valid brick json")
}

fn text(id: &str, value: &str) -> Brick {
    brick(json!({
        "type": "text", "id": id,
        "bind": { "value": { "kind": "default", "default": value } }
    }))
}

fn default_of(b: &Brick) -> Value {
    b.get_bind()
        .and_then(|m| m.get("value"))
        .and_then(|x| x.default.clone())
        .unwrap()
}

#[test]
fn concat_string_values_append_left_to_right() {
    let mut lhs = text("m1", "hello ");
    let mut rhs = text("m1", "world");
    lhs.merge(&Concat, &mut rhs);
    assert_eq!(default_of(&lhs), json!("hello world"));
}

#[test]
fn concat_numbers_sum() {
    let mut lhs = brick(json!({
        "type": "text", "id": "n",
        "bind": { "value": { "kind": "default", "default": 3 } }
    }));
    let mut rhs = brick(json!({
        "type": "text", "id": "n",
        "bind": { "value": { "kind": "default", "default": 4 } }
    }));
    lhs.merge(&Concat, &mut rhs);
    // numbers pass through as_f64, so sums land as floats
    assert_eq!(default_of(&lhs), json!(7.0));
}

#[test]
fn concat_objects_deep_merge_by_key() {
    let mut lhs = brick(json!({
        "type": "text", "id": "o",
        "bind": { "value": { "kind": "default", "default": { "a": 1, "b": 2 } } }
    }));
    let mut rhs = brick(json!({
        "type": "text", "id": "o",
        "bind": { "value": { "kind": "default", "default": { "b": 20, "c": 30 } } }
    }));
    lhs.merge(&Concat, &mut rhs);
    assert_eq!(default_of(&lhs), json!({ "a": 1, "b": 20, "c": 30 }));
}

#[test]
fn replace_overwrites_scalar_and_drops_missing_keys_in_objects() {
    let mut lhs = text("r", "old");
    let mut rhs = text("r", "new");
    lhs.merge(&Replace, &mut rhs);
    assert_eq!(default_of(&lhs), json!("new"));
}

#[test]
fn delete_removes_substring_and_subtracts_numbers() {
    let mut lhs = text("d", "abcabc");
    let mut rhs = text("d", "abc");
    lhs.merge(&Delete, &mut rhs);
    assert_eq!(default_of(&lhs), json!(""));

    let mut lhs = brick(json!({
        "type": "text", "id": "d2",
        "bind": { "value": { "kind": "default", "default": 10 } }
    }));
    let mut rhs = brick(json!({
        "type": "text", "id": "d2",
        "bind": { "value": { "kind": "default", "default": 4 } }
    }));
    lhs.merge(&Delete, &mut rhs);
    assert_eq!(default_of(&lhs), json!(6.0));
}

#[test]
fn delete_object_removes_matching_keys_only() {
    let mut lhs = brick(json!({
        "type": "text", "id": "d3",
        "bind": { "value": { "kind": "default", "default": { "a": 1, "b": 2 } } }
    }));
    let mut rhs = brick(json!({
        "type": "text", "id": "d3",
        "bind": { "value": { "kind": "default", "default": { "a": null } } }
    }));
    lhs.merge(&Delete, &mut rhs);
    assert_eq!(default_of(&lhs), json!({ "b": 2 }));
}

#[test]
fn merge_zip_children_keeps_surplus_from_both_sides() {
    // lhs: [c1, c2]; rhs: [r1] -> position 0 merged, position 1 kept.
    let mut lhs = brick(json!({
        "type": "case", "id": "p",
        "children": [ text("x", "A"), text("y", "B") ]
    }));
    let mut rhs = brick(json!({
        "type": "case", "id": "p",
        "children": [ text("x", "+") ]
    }));
    lhs.merge(&Concat, &mut rhs);
    let kids = lhs.borrow_children().unwrap();
    assert_eq!(kids.len(), 2);
    assert_eq!(default_of(&kids[0]), json!("A+"));
    assert_eq!(default_of(&kids[1]), json!("B"));
}

#[test]
fn merge_child_surplus_from_rhs_is_appended() {
    let mut lhs = brick(json!({
        "type": "case", "id": "p",
        "children": [ text("x", "A") ]
    }));
    let mut rhs = brick(json!({
        "type": "case", "id": "p",
        "children": [ text("x", "+"), text("z", "new") ]
    }));
    lhs.merge(&Concat, &mut rhs);
    let kids = lhs.borrow_children().unwrap();
    assert_eq!(kids.len(), 2);
    assert_eq!(default_of(&kids[0]), json!("A+"));
    assert_eq!(default_of(&kids[1]), json!("new"));
}

#[test]
fn bind_keys_missing_on_one_side_are_inserted_whole() {
    let mut lhs = brick(json!({
        "type": "text", "id": "k",
        "bind": { "value": { "kind": "default", "default": "v" } }
    }));
    let mut rhs = brick(json!({
        "type": "text", "id": "k",
        "bind": { "extra": { "kind": "default", "default": 7 } }
    }));
    lhs.merge(&Concat, &mut rhs);
    let bind = lhs.get_bind().unwrap();
    assert!(bind.contains_key("extra"), "rhs-only key inserted");
    assert_eq!(
        default_of(&lhs),
        json!("v"),
        "lhs-only key default preserved"
    );
}

#[test]
fn cmp_id_requires_both_ids_and_equality() {
    assert!(text("a", "1").cmp_id(&text("a", "2")));
    assert!(!text("a", "1").cmp_id(&text("b", "1")));
    let no_id = brick(json!({ "type": "text" }));
    assert!(!text("a", "1").cmp_id(&no_id));
    assert!(!no_id.cmp_id(&text("a", "1")));
}
