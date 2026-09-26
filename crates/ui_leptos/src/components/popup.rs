use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::use_common_css;
use accrete::Popup;
use leptos::html::*;
use leptos::prelude::*;

/// 弹窗：sub[0] 为触发占位，sub[1] 为模态内容。
pub fn popup_(accrete: Popup, ctx: &Ctx) -> AnyView {
    let mut css = vec!["popup", "f"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");
    let style = accrete
        .attrs
        .as_ref()
        .map(|x| x.into_style())
        .unwrap_or_default();

    if let Some(subs) = accrete.children.as_deref()
        && let Some(placeholder) = subs.first()
        && let Some(modal) = subs.get(1)
    {
        div()
            .class(css.as_str())
            .style(style)
            .child(div().class("f").child(render_accrete(ctx, placeholder)))
            .child(div().class("f body").child(render_accrete(ctx, modal)))
            .into_any()
    } else {
        div().into_any()
    }
}
