use crate::Ctx;
use crate::ctx::render_children;
use crate::hooks::{use_common_css, use_default};
use accrete::{Path, Svg};
use leptos::html::*;
use leptos::prelude::*;
use leptos::svg;

/// SVG 容器：`SizeAttr::size_style()` 尺寸 + 公共 CSS。
pub fn svg_(accrete: Svg, ctx: &Ctx) -> AnyView {
    let mut css = vec!["svg"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");
    let style = accrete
        .attrs
        .as_ref()
        .map(|x| x.size_style())
        .unwrap_or_default();
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();
    svg::svg()
        .class(css.as_str())
        .style(style)
        .child(children)
        .into_any()
}

/// SVG 路径：`d` 取 `source`。
pub fn path_(accrete: Path, _ctx: &Ctx) -> AnyView {
    let mut css = vec!["path"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");
    if let Some(x) = use_default(&accrete)
        && let Some(d) = x.as_str()
    {
        let d = d.to_string();
        svg::path().class(css.as_str()).attr("d", d).into_any()
    } else {
        div().into_any()
    }
}
