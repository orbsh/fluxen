//! Canvas 的 options 动态袋（ADR 0017）：核心只搬字节，不解释任何键。
//!
//! 三条不变量：往返不丢模块私有词汇、缺省时 wire 上不出现该键、patch 能按
//! wire 形状寻址到袋里的深层键（与 ADR 0005 的指针词法同一套）。

use accrete::{Accrete, AccreteOps};
use serde_json::{json, Value};

fn accrete(js: Value) -> Accrete {
    serde_json::from_value(js).expect("valid accrete json")
}

#[test]
fn options_roundtrip_keeps_module_vocabulary() {
    // 3d-force-graph 的词汇（节点尺寸、力参数、配色）——核心一个都不认识，
    // 往返只搬字节：这是"变体不进 accrete"的可执行形式。
    let src = json!({
        "type": "canvas",
        "url": "/assets/forcegraph/index.js",
        "options": {
            "nodeSizePx": 2.4,
            "linkWidthPx": 1.2,
            "physics": { "charge": -200, "linkDistance": 120 },
            "palette": ["#818cf8", "#34d399"]
        },
        "bind": { "value": { "kind": "source", "source": "mfg" } }
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