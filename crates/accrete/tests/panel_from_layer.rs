//! ADR 0014's `layer` was folded into `panel` in ADR 0030 (place: viewport, state: bound).
//! `portal` is deleted: appearing across places works by "the server writes a slot, whoever binds it displays it", no subtree hauling needed.
//! After that, every kind of shape must go through the real codec round trip — the wire is the contract, a Rust-side enum construction does not count.
//!
//! Note: accrere's decode does not set `deny_unknown_fields`, so the extra
//! `overlay`/`dismiss` keys in old frames are silently ignored — not an error, just a degraded plain float. So
//! "swept clean" is guarded by **the encoded result having neither key**, not by a decode failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use accrete::Accrete;

fn roundtrip(src: &str) -> Accrete {
    let a: Accrete = serde_json::from_str(src).expect("decode");
    let back = serde_json::to_string(&a).expect("encode");
    let b: Accrete = serde_json::from_str(&back).expect("re-decode");
    assert_eq!(a, b, "round-trip changed the node: {back}");
    b
}

#[test]
fn viewport_bound_roundtrip() {
    // Typical shape: a local slot drives show/hide + dismiss + modal content
    roundtrip(
        r##"{"type":"panel","id":"login_layer","attrs":{"place":"viewport","state":"bound","dismiss":true},
             "source":{"slot":"login_modal","scope":"local"},
             "children":[{"type":"text","source":{"default":"hi"}}]}"##,
    );
    // No attrs / no bind / no children: all three are optional, omitted means empty
    roundtrip(r#"{"type":"panel","id":"bare","attrs":{"place":"viewport","state":"bound"}}"#);
    // Binding a data slot is legal too (both render-side arms read it)
    roundtrip(r#"{"type":"panel","id":"l2","attrs":{"place":"viewport","state":"bound"},"source":{"slot":"modal"}}"#);
}

#[test]
fn float_is_not_a_word_any_more() {
    // `float` is deleted entirely (ADR 0031): a corner-anchored thing is panel's viewport cell + a position.
    let err = serde_json::from_str::<Accrete>(r#"{"type":"float","attrs":{"top":"1rem"}}"#)
        .expect_err("float 应当解码失败");
    assert!(err.to_string().contains("unknown variant"), "{err}");

    // The same shape as a panel: the position keys are flattened into attrs, no extra layer needed
    let p = roundtrip(
        r#"{"type":"panel","attrs":{"place":"viewport","state":"bound","right":"1rem","top":"1rem"},
            "source":{"slot":"notice"},"children":[]}"#,
    );
    let back = serde_json::to_value(&p).expect("to_value").to_string();
    assert!(back.contains("\"right\"") && back.contains("\"top\""), "{back}");
}