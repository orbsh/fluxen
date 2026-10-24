//! ADR 0014：新增 `layer` / `portal` 两个变体（并撤掉 Float 的 overlay/dismiss）
//! 之后，各类形状都要过真实 codec 的往返——wire 是契约，Rust 端枚举构造不算数。
//!
//! 注意：accrere 的解码不设 `deny_unknown_fields`，所以旧帧里多带的
//! `overlay`/`dismiss` 键会被静默忽略——不是报错，是退化成纯定位 float。所以
//! 「撤干净」这条用**编码结果里没有这两个键**来守，而不是用解码失败来守。
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
fn layer_roundtrip() {
    // 典型形状：local 槽控显隐 + dismiss + 模态内容
    roundtrip(
        r##"{"type":"layer","id":"login_layer","attrs":{"dismiss":true},
             "bind":{"value":{"kind":"local","slot":"login_modal"}},
             "children":[{"type":"text","bind":{"value":{"kind":"default","default":"hi"}}}]}"##,
    );
    // 无 attrs / 无 bind / 无 children：三项全可选，省略即空
    roundtrip(r#"{"type":"layer","id":"bare"}"#);
    // data 槽绑定也合法（渲染侧两支都读）
    roundtrip(r#"{"type":"layer","id":"l2","bind":{"value":{"kind":"source","source":"modal"}}}"#);
}

#[test]
fn portal_roundtrip() {
    roundtrip(
        r##"{"type":"portal","attrs":{"to":"#login_layer"},
             "children":[{"type":"text","bind":{"value":{"kind":"default","default":"inside"}}}]}"##,
    );
    // children 省略
    roundtrip(r##"{"type":"portal","attrs":{"to":"#login_layer"}}"##);
}

#[test]
fn float_overlay_fields_are_gone() {
    // 撤掉 overlay/dismiss 后：编码结果里不再出现这两个键（旧帧多带的键被忽略）
    let f = roundtrip(r#"{"type":"float","attrs":{"overlay":true,"dismiss":true},"children":[]}"#);
    let v = serde_json::to_value(&f).expect("to_value");
    let s = v.to_string();
    assert!(
        !s.contains("overlay") && !s.contains("dismiss"),
        "overlay/dismiss 应已从 wire 撤掉，实际编码：{s}"
    );
    // 纯定位形状照旧
    roundtrip(r#"{"type":"float","attrs":{"bottom":"0.2rem","left":"1rem"},"children":[]}"#);
}