//! Canvas's options dynamic bag (ADR 0017): the core only moves bytes, it interprets no key.
//!
//! Three invariants: a round trip loses no module-private vocabulary, the key is absent from the wire when unused, and patch can
//! address deep keys inside the bag by the wire shape (the same pointer vocabulary as ADR 0005).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use accrete::{Accrete, AccreteOps};
use serde_json::{json, Value};

fn accrete(js: Value) -> Accrete {
    serde_json::from_value(js).expect("valid accrete json")
}

#[test]
fn options_roundtrip_keeps_module_vocabulary() {
    // 3d-force-graph's vocabulary (node sizes, force parameters, colours) — the core knows none of them,
    // a round trip only moves bytes: this is the executable form of "variants do not enter accrete".
    let src = json!({
        "type": "canvas",
        "url": "/assets/forcegraph/index.js",
        "options": {
            "nodeSizePx": 2.4,
            "linkWidthPx": 1.2,
            "physics": { "charge": -200, "linkDistance": 120 },
            "palette": ["#818cf8", "#34d399"]
        },
        "source": { "slot": "mfg" }
    });
    let node = accrete(src.clone());
    assert_eq!(serde_json::to_value(&node).unwrap(), src);
}

#[test]
fn options_absent_is_not_on_the_wire() {
    let node = accrete(json!({ "type": "canvas", "url": "/x.js" }));
    let wire = serde_json::to_value(&node).unwrap();
    assert!(
        wire.get("options").is_none(),
        "缺省 = 宿主传 `{{}}` 给模块，wire 上不留空键"
    );
}

#[test]
fn options_is_addressable_by_patch() {
    let mut node = accrete(json!({
        "type": "canvas",
        "url": "/x.js",
        "options": { "nodeSizePx": 2, "physics": { "charge": -200 } }
    }));
    node.replace_at("/options/nodeSizePx", json!(5))
        .expect("patch a deep key inside the options bag");
    let wire = serde_json::to_value(&node).unwrap();
    assert_eq!(wire["options"]["nodeSizePx"], 5);
    assert_eq!(wire["options"]["physics"]["charge"], -200);
}