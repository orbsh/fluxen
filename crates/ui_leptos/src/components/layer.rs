use crate::Ctx;
use crate::components::canvas::{HIDDEN_STYLE, payload_is_empty};
use crate::ctx::render_children;
use crate::hooks::use_common_css;
use accrete::{AccreteOps, Bind, Layer};
use leptos::ev::click;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;

/// 模态层（ADR 0014）：应用在骨架里显式声明的全屏挂载点，portal 把内容传送进来。
///
/// 几何必须内联，两条实测教训：① custom 层的定序缺陷在 main.css 层序声明归位前
/// 会输给 element 层（PLAN §19，2026-10-10 已修，但内联本无层叠歧义，与 ADR 0012
/// 的 display 内联同款）；② 本机 headless chromium 上 `position: fixed` + `inset: 0`
/// 的高度不撑开（干净 div 实测 20px），要显式给 `height`。遮罩（模糊/色调）走
/// custom.css 的 `::before`。
///
/// 显隐只有一条规则：**所绑槽非空即显示**（复用 ADR 0012 的"空即无"）。
/// `dismiss` 点层自身（模糊区，而非模态内容）时**清空所绑的槽**——清空即关，
/// 不新造第二个掩蔽状态；槽空了，下次写入必是新值，同值短路不会误挡重开。
/// 故 dismiss 需要槽绑定：inline default 的层无槽可清，只 warn。
pub fn layer_(accrete: Layer, ctx: &Ctx, id: String) -> AnyView {
    let dismiss = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.dismiss)
        .unwrap_or(false);
    let mut css = vec!["layer", "f"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let (local_slot, data_source, inline) = bind_slot(accrete.bind.as_ref());
    // 构建那一刻的值（首次 Effect 前）：与 canvas 宿主同款，交给下面的 Effect 同步
    let style_now = if payload_is_empty(&read_payload(ctx, &local_slot, &data_source, &inline)) {
        HIDDEN_STYLE.to_string()
    } else {
        LAYER_STYLE.to_string()
    };

    let nr = NodeRef::<Div>::new();
    {
        let ctx = ctx.clone();
        let (l, s, i) = (local_slot.clone(), data_source.clone(), inline.clone());
        Effect::new(move |_| {
            let want = if payload_is_empty(&read_payload(&ctx, &l, &s, &i)) {
                HIDDEN_STYLE
            } else {
                LAYER_STYLE
            };
            if let Some(el) = nr.get() {
                el.set_attribute("style", want).ok();
            }
        });
    }

    // 闭包要 'static：Ctx 与 id 都得先克隆进作用域（ctx 是 Rc，clone 即计数加一）
    let onclick = {
        let ctx = ctx.clone();
        let id = id.clone();
        move |ev: web_sys::MouseEvent| {
            if !dismiss {
                return;
            }
            // e.target === e.currentTarget：点的是层自己的空白（模糊面），点模态内容
            // 不算——事件在冒泡路径上也会走到这里，判等把内容排掉。
            let same = match (ev.target(), ev.current_target()) {
                (Some(t), Some(c)) => {
                    // 显式要 JsValue 视图：EventTarget 的 AsRef 有三个 impl，
                    // 不标注会撞 E0283（类型推导无法在三个 AsRef 里选）
                    let tv: &wasm_bindgen::JsValue = t.as_ref();
                    let cv: &wasm_bindgen::JsValue = c.as_ref();
                    tv == cv
                }
                _ => false,
            };
            if !same {
                return;
            }
            if let Some(slot) = &local_slot {
                ctx.slot_for_value(slot).set(None);
            } else if let Some(src) = &data_source {
                ctx.slot_for_data(src).set(None);
            } else {
                tracing::warn!(
                    "layer {id}: dismiss 需要槽绑定（清空槽 = 关），inline default 无槽可清"
                );
            }
        }
    };

    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();

    div()
        .id(id)
        .class(css.as_str())
        .style(style_now)
        .on(click, onclick)
        .child(children)
        .node_ref(nr)
        .into_any()
}

/// 层几何：铺满视口 + 居中模态内容。z-index 高于普通浮层（float 不设 z）。
const LAYER_STYLE: &str = "position: fixed; inset: 0; height: 100vh; z-index: 100; \
                           display: flex; align-items: center; justify-content: center;";

/// `bind["value"]` → (local 槽名, data 槽名, inline 默认值)，三者至多其一。
fn bind_slot(
    bind: Option<&std::collections::HashMap<String, Bind>>,
) -> (Option<String>, Option<String>, Option<Value>) {
    match bind.and_then(|b| b.get("value")) {
        Some(Bind {
            variant: accrete::BindVariant::Local { slot, .. },
            ..
        }) => (Some(slot.clone()), None, None),
        Some(Bind {
            variant: accrete::BindVariant::Source { source, .. },
            ..
        }) => (None, Some(source.clone()), None),
        Some(Bind { default, .. }) => (None, None, default.clone()),
        None => (None, None, None),
    }
}

/// 当前载荷：local 槽裸值 / data 槽节点（其 `bind.value.default` 优先，否则整节点
/// JSON）/ inline 默认值。槽没写过 = Null = 空（"空即无"的判据见 payload_is_empty）。
fn read_payload(
    ctx: &Ctx,
    local: &Option<String>,
    source: &Option<String>,
    inline: &Option<Value>,
) -> Value {
    if let Some(slot) = local {
        return ctx.slot_for_value(slot).get().unwrap_or(Value::Null);
    }
    if let Some(src) = source {
        return match ctx.slot_for_data(src).get().map(|a| (*a).clone()) {
            Some(node) => node
                .get_bind()
                .and_then(|b| b.get("value"))
                .and_then(|v| v.default.clone())
                .unwrap_or_else(|| serde_json::to_value(&node).unwrap_or(Value::Null)),
            None => Value::Null,
        };
    }
    inline.clone().unwrap_or(Value::Null)
}