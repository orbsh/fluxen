use crate::Ctx;
use crate::ctx::render_children;
use crate::hooks::use_common_css;
use accrete::Float;
use leptos::html::*;
use leptos::prelude::*;

/// 浮动容器：`PositionAttr::into_style()` 定位样式 + 公共 CSS。
/// 模态语义（铺满视口 / 模糊背景 / 点空白关闭）已归 layer（ADR 0014）——
/// 遮罩属于应用外壳，不该从消息行的子树里靠 `fixed` 逃出去。
pub fn float_(accrete: Float, ctx: &Ctx) -> AnyView {
    let mut css = vec!["float", "f"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");
    let style = accrete
        .attrs
        .as_ref()
        .map(|x| x.into_style())
        .unwrap_or_default();
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();

    div()
        .class(css.as_str())
        .style(style)
        .child(children)
        .into_any()
}