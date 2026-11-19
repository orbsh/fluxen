//! ADR 0014 的 `layer` 在 ADR 0030 里并进了 `panel`（place: viewport, state: bound）。
//! `portal` 已删除：跨处出现靠"服务端写槽、谁绑谁显示"，不需要搬运子树。
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
        r##"{"type":"panel","id":"login_layer","attrs":{"place":"viewport","state":"bound","dismiss":true},
             "bind":{"value":{"kind":"local","slot":"login_modal"}},
             "children":[{"type":"text","bind":{"value":{"kind":"default","default":"hi"}}}]}"##,
    );
    // 无 attrs / 无 bind / 无 children：三项全可选，省略即空
    roundtrip(r#"{"type":"panel","id":"bare","attrs":{"place":"viewport","state":"bound"}}"#);
    // data 槽绑定也合法（渲染侧两支都读）
    roundtrip(r#"{"type":"panel","id":"l2","attrs":{"place":"viewport","state":"bound"},"bind":{"value":{"kind":"source","source":"modal"}}}"#);
}

#[test]
fn float_is_not_a_word_any_more() {
    // `float` 整个删掉了（ADR 0031）：贴角的东西是 panel 的 viewport 那一格 + 位置。
    let err = serde_json::from_str::<Accrete>(r#"{"type":"float","attrs":{"top":"1rem"}}"#)
        .expect_err("float 应当解码失败");
    assert!(err.to_string().contains("unknown variant"), "{err}");

    // 同一个形状换成 panel：位置键被展平进 attrs，不需要额外一层
    let p = roundtrip(
        r#"{"type":"panel","attrs":{"place":"viewport","state":"bound","right":"1rem","top":"1rem"},
            "bind":{"value":{"kind":"source","source":"notice"}},"children":[]}"#,
    );
    let back = serde_json::to_value(&p).expect("to_value").to_string();
    assert!(back.contains("\"right\"") && back.contains("\"top\""), "{back}");
}