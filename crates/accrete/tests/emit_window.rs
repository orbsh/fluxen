//! A field's timing (ADR 0032 §6): the wire says `debounce` or `throttle`, one at a time, and saying
//! nothing means a 250 ms debounce. These are the shapes a producer writes, so they are worth pinning
//! here rather than discovering in a browser.

use accrete::{Accrete, Emit, Input};

fn input(v: serde_json::Value) -> Input {
    serde_json::from_value(v).expect("a valid input")
}

#[test]
fn saying_nothing_is_a_quarter_second_debounce() {
    let i = input(serde_json::json!({ "tag": "q" }));
    assert!(i.emit.is_none());
    assert_eq!(i.emission(), Emit::default());
    assert_eq!(i.emission().millis(), 250);
    assert!(!i.emission().is_throttle());
}

#[test]
fn the_wire_says_one_window_one_way() {
    let d = input(serde_json::json!({ "tag": "q", "emit": { "debounce": 300 } }));
    assert_eq!(d.emission().millis(), 300);
    assert!(!d.emission().is_throttle());

    let t = input(serde_json::json!({ "tag": "q", "emit": { "throttle": 40 } }));
    assert_eq!(t.emission().millis(), 40);
    assert!(t.emission().is_throttle());
}

#[test]
fn zero_is_the_way_back_to_every_keystroke() {
    let i = input(serde_json::json!({ "tag": "q", "emit": { "debounce": 0 } }));
    assert_eq!(i.emission().millis(), 0);
    assert!(!i.emission().is_throttle());
}

#[test]
fn both_windows_in_one_object_are_refused() {
    let err = serde_json::from_value::<Input>(serde_json::json!({
        "tag": "q",
        "emit": { "debounce": 1, "throttle": 2 }
    }))
    .expect_err("one field, one window");
    // serde's untagged message names the enum, not the offending key — the point is that it refuses,
    // so a frame cannot buy both "wait for quiet" and "keep a cadence".
    assert!(err.to_string().contains("Emit"), "{err}");
}

#[test]
fn an_empty_window_is_refused_too() {
    assert!(
        serde_json::from_value::<Input>(serde_json::json!({ "tag": "q", "emit": {} })).is_err()
    );
    // A misspelled arm must not fall through to the default: `{debounce_ms: 5}` is not a window.
    assert!(
        serde_json::from_value::<Input>(
            serde_json::json!({ "tag": "q", "emit": { "debounce_ms": 5 } })
        )
        .is_err()
    );
}

#[test]
fn the_window_round_trips_on_the_node() {
    let node: Accrete = serde_json::from_value(serde_json::json!({
        "type": "input",
        "tag": "q",
        "emit": { "throttle": 50 }
    }))
    .expect("valid node");
    let back = serde_json::to_value(&node).expect("serializes");
    assert_eq!(back["emit"]["throttle"], 50);
    // Absent is absent: the default is the reader's business, not the writer's.
    let plain: Accrete =
        serde_json::from_value(serde_json::json!({ "type": "input", "tag": "q" })).unwrap();
    let back = serde_json::to_value(&plain).expect("serializes");
    assert!(back.get("emit").is_none());
}
