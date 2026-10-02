//! ADR 0011: the remote vals-plane write (`action: inject`). Its payload is
//! the ADR 0010 wrapper, so the slot ends up holding exactly what an in-page
//! `Local` emit would write — subscribers cannot tell the two apart.

use content::{Content, InjectOp};
use serde_json::{Value, json};

#[test]
fn decodes_with_slot_and_payload_only() {
    let c: Content<Value> =
        serde_json::from_value(json!({"action": "inject", "slot": "page", "data": "about"}))
            .unwrap();
    let Content::Inject(op) = c else {
        panic!("not an inject: {c:?}");
    };
    assert_eq!(op.slot, "page");
    assert_eq!(op.data, json!("about"));
    assert_eq!(op.event, None);
    assert_eq!(op.id, None);
}

#[test]
fn carries_no_t_so_any_plane_type_can_hold_it() {
    // The vals plane's currency is a bare Value (ADR 0008), so the variant
    // needs no T — a frame typed for another plane still decodes it.
    let c: Content<u8> =
        serde_json::from_value(json!({"action": "inject", "slot": "page", "data": "about"}))
            .unwrap();
    assert!(matches!(c, Content::Inject(_)));
}

#[test]
fn event_defaults_to_the_slot_name() {
    let op = InjectOp {
        slot: "page".into(),
        event: None,
        id: None,
        data: json!("about"),
    };
    assert_eq!(
        serde_json::to_value(op.outflow()).unwrap(),
        json!({"event": "page", "data": "about"})
    );
}

#[test]
fn explicit_event_and_id_pass_through() {
    let op = InjectOp {
        slot: "page".into(),
        event: Some("channel::select".into()),
        id: Some("tab1".into()),
        data: json!("2"),
    };
    assert_eq!(
        serde_json::to_value(op.outflow()).unwrap(),
        json!({"event": "channel::select", "id": "tab1", "data": "2"})
    );
}

#[test]
fn absent_event_and_id_are_omitted_on_the_wire() {
    let op = InjectOp {
        slot: "page".into(),
        event: None,
        id: None,
        data: json!("about"),
    };
    assert_eq!(
        serde_json::to_value(&op).unwrap(),
        json!({"slot": "page", "data": "about"})
    );
    let back: InjectOp = serde_json::from_value(serde_json::to_value(&op).unwrap()).unwrap();
    assert_eq!(back, op);
}

#[test]
fn structural_payloads_keep_their_shape() {
    // Not just scalars: a local event's payload can be any JSON, and the
    // wrapper must not reshape it (ADR 0010: shape is defined by the event).
    let op = InjectOp {
        slot: "filters".into(),
        event: None,
        id: None,
        data: json!({"dept": ["研发部", "市场部"], "range": [0, 5]}),
    };
    assert_eq!(
        serde_json::to_value(op.outflow()).unwrap(),
        json!({"event": "filters", "data": {"dept": ["研发部", "市场部"], "range": [0, 5]}})
    );
}