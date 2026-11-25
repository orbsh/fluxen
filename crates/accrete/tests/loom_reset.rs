//! ADR 0032 §6-7: after a send a loom's fields either stay, clear or go back to their starting value,
//! and a loom may be granted a number of sends. These are wire-contract checks — what a frame may say
//! is the thing being pinned, decoded through the codec like every other node.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn
)]

use accrete::{Accrete, LoomAttr, ResetMode};
use serde_json::json;

fn attrs(v: serde_json::Value) -> LoomAttr {
    let a: Accrete = serde_json::from_value(json!({
        "type": "loom",
        "sink": {"dest": "confirm"},
        "attrs": v,
        "children": [{"type": "input", "tag": "q"}]
    }))
    .expect("decode");
    let Accrete::loom(b) = a else {
        panic!("a loom decoded as something else")
    };
    b.attrs.expect("attrs")
}

#[test]
fn a_loom_that_says_nothing_keeps_its_fields_and_counts_nothing() {
    let a = attrs(json!({}));
    assert_eq!(a.reset, ResetMode::Keep);
    assert_eq!(a.sends, None, "absent is unlimited");
    assert_eq!(a.submit.modes(), [accrete::SubmitMode::Enter]);
}

#[test]
fn reset_is_one_of_three_words() {
    assert_eq!(attrs(json!({"reset": "clear"})).reset, ResetMode::Clear);
    assert_eq!(attrs(json!({"reset": "default"})).reset, ResetMode::Default);
    assert_eq!(attrs(json!({"reset": "keep"})).reset, ResetMode::Keep);
    assert!(
        serde_json::from_value::<Accrete>(json!({
            "type": "loom",
            "attrs": {"reset": "empty"},
            "children": [{"type": "input", "tag": "q"}]
        }))
        .is_err(),
        "a fourth word must not decode"
    );
}

#[test]
fn a_dropped_reset_keeps_its_word_off_the_wire() {
    let back = serde_json::to_value(attrs(json!({}))).expect("encode");
    assert_eq!(back.get("reset"), None, "`keep` is the absent value");
    let back = serde_json::to_value(attrs(json!({"reset": "clear"}))).expect("encode");
    assert_eq!(back.get("reset").and_then(|v| v.as_str()), Some("clear"));
}

#[test]
fn sends_is_a_signed_count_so_minus_one_can_say_unlimited_out_loud() {
    assert_eq!(attrs(json!({"sends": 3})).sends, Some(3));
    assert_eq!(attrs(json!({"sends": -1})).sends, Some(-1));
    assert_eq!(attrs(json!({"sends": 0})).sends, Some(0));
    assert_eq!(
        serde_json::to_value(attrs(json!({"sends": 2}))).unwrap()["sends"],
        2
    );
}
