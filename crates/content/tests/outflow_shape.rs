//! ADR 0010: the wrapped event payload shape shared by the uplink and
//! the `local` slot write (`Ctx::emit` builds exactly this object).

use content::Outflow;
use serde_json::json;

#[test]
fn id_is_omitted_when_absent() {
    let o = Outflow {
        event: "channel::select".into(),
        id: None,
        data: json!("2"),
    };
    assert_eq!(
        serde_json::to_value(&o).unwrap(),
        json!({"event": "channel::select", "data": "2"})
    );
}

#[test]
fn id_passes_through_when_present() {
    let o = Outflow {
        event: "page".into(),
        id: Some("tab1".into()),
        data: json!(7),
    };
    assert_eq!(
        serde_json::to_value(&o).unwrap(),
        json!({"event": "page", "id": "tab1", "data": 7})
    );
    // decode back
    let d: Outflow = serde_json::from_value(serde_json::to_value(&o).unwrap()).unwrap();
    assert_eq!(d, o);
}
