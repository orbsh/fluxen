//! ADR 0013：PositionAttr 新增 `overlay` / `dismiss` 之后，两类形状（带新字段
//! / 不带）都要过真实 codec 的往返——wire 是契约，Rust 端枚举构造不算数。

use accrete::Accrete;

fn roundtrip(src: &str) -> Accrete {
    let a: Accrete = serde_json::from_str(src).expect("decode");
    let back = serde_json::to_string(&a).expect("encode");
    let b: Accrete = serde_json::from_str(&back).expect("re-decode");
    assert_eq!(a, b, "round-trip changed the node: {back}");
    b
}

#[test]
fn float_overlay_attrs_roundtrip() {
    // 新形状：overlay + dismiss
    roundtrip(r#"{"type":"float","attrs":{"overlay":true,"dismiss":true},"children":[]}"#);
    // 旧形状：只有定位，解出来 overlay/dismiss 为 None，行为与今天逐字一致
    roundtrip(r#"{"type":"float","attrs":{"bottom":"0.2rem","left":"1rem"},"children":[]}"#);
    // 混用：定位与 overlay 同现也不产生信封歧义（新字段不进 PosH/PosV 那对 flatten）
    roundtrip(r#"{"type":"float","attrs":{"overlay":true,"bottom":"0.2rem"},"children":[]}"#);
    // 只给 dismiss（没 overlay）：解得出，渲染侧忽略
    roundtrip(r#"{"type":"float","attrs":{"dismiss":true},"children":[]}"#);
}