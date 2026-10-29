//! ADR 0010/0023: the wrapped uplink payload shape shared by a `dest` send and
//! the `local` slot write (`Ctx::emit` builds exactly this object).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use content::Outflow;
use serde_json::json;

#[test]
fn id_is_omitted_when_absent() {
    let o = Outflow {
        dest: "channel::select".into(),
        id: None,
        data: json!("2"),
    };
    assert_eq!(
        serde_json::to_value(&o).unwrap(),
        json!({"dest": "channel::select", "data": "2"})
    );
}

#[test]
fn id_passes_through_when_present() {
    let o = Outflow {
        dest: "page".into(),
        id: Some("tab1".into()),
        data: json!(7),
    };
    assert_eq!(
        serde_json::to_value(&o).unwrap(),
        json!({"dest": "page", "id": "tab1", "data": 7})
    );
    // decode back
    let d: Outflow = serde_json::from_value(serde_json::to_value(&o).unwrap()).unwrap();
    assert_eq!(d, o);
}
