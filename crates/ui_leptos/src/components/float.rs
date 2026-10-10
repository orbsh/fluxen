use crate::Ctx;
use crate::ctx::render_children;
use crate::hooks::use_common_css;
use accrete::Float;
use leptos::ev::click;
use leptos::html::*;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// 浮动容器：`PositionAttr::into_style()` 定位样式 + 公共 CSS。
///
/// `overlay`（ADR 0013）= 模态形态：铺满视口、背景模糊、背后页面不可操作。
/// 几何归 CSS 类（`.float.overlay`），所以 overlay 模式下**不发内联定位样式**
/// （内联会盖过类规则），位置偏移随之无意义。`dismiss` = 点浮层自身（模糊面，
/// 而非模态内容）本地隐藏——本页瞬时状态，与 Popup 的 hover 同类，不上线协议。
pub fn float_(accrete: Float, ctx: &Ctx) -> AnyView {
    let overlay = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.overlay)
        .unwrap_or(false);
    let dismiss = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.dismiss)
        .unwrap_or(false);

    let mut css = vec!["float", "f"];
    use_common_css(&mut css, &accrete);
    if overlay {
        css.push("overlay");
    }
    let css = css.join(" ");
    // overlay 模式下位置偏移（h/v）无意义；几何必须内联，两条实测教训：
    // ① custom 层因 main.css 的 @import 定序实际排在最前（docs/PLAN.md §19），写在
    //    CSS 里的 position 会输给 element 层的 `.float{position:absolute}`；
    // ② 本机 headless chromium 上 `position: fixed` + `inset: 0` 的高度不撑开
    //    （干净 div 实测 20px），要显式给 height，否则覆盖层只有内容高。
    // 内联无层叠问题（ADR 0012 的 display 内联同款）。
    let style = if overlay {
        "position: fixed; inset: 0; height: 100vh; z-index: 100; align-items: center; justify-content: center;"
            .to_string()
    } else {
        accrete
            .attrs
            .as_ref()
            .map(|x| x.into_style())
            .unwrap_or_default()
    };
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();

    let onclick = move |ev: web_sys::MouseEvent| {
        if !(overlay && dismiss) {
            return;
        }
        // e.target === e.currentTarget：点的是浮层自己的空白（模糊面），
        // 点模态内容不算——事件在冒泡路径上也会走到这里，判等把内容排掉。
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
        if same
            && let Some(ct) = ev.current_target()
            && let Ok(el) = ct.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = el.style().set_property("display", "none");
        }
    };

    div()
        .class(css.as_str())
        .style(style)
        .on(click, onclick)
        .child(children)
        .into_any()
}
