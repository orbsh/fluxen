use crate::Ctx;
use crate::ctx::render_accrete;
use crate::ctx::render_children;
use crate::hooks::use_common_css;
use accrete::{AccreteOps, BindVariant, Case, CaseAttr, Placeholder};
use leptos::html::*;
use leptos::prelude::*;

/// 容器：`case` class + grid 样式 + 公共 CSS。
pub fn case_(accrete: Case, ctx: &Ctx) -> AnyView {
    // grid 与非 grid 二选一：此前 "f" 在初值里、grid 分支又补一次，
    // class 变成 `case f f g`（或 grid 时 `case f g`）。main.css 同一
    // @layer 里 .f 声明在 .g 之后，display:flex 压过 display:grid，
    // grid-template-* 全部失效——`attrs.grid` 自出生起就没生效过
    // （全仓 grep：examples/ 里没有任何 grid 用例，所以无人踩到）。
    // 修法=grid 时只进 "g"，否则只进 "f"。
    let mut css = vec!["case"];
    if let Some(id) = &accrete.id {
        css.push(id.as_str());
    }
    let mut style = String::new();
    let mut use_grid = false;
    if let Some(CaseAttr { grid, .. }) = &accrete.attrs {
        if let Some(g) = grid {
            use_grid = true;
            css.push("g");
            style = g
                .iter()
                .map(|(k, v)| format!("{}: {};", k, v.as_str().unwrap_or("")))
                .collect::<Vec<String>>()
                .join("\n");
        }
    }
    if !use_grid {
        css.push("f");
    }
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_else(|| ().into_any());
    div()
        .class(css.as_str())
        .style(style)
        .child(children)
        .into_any()
}

/// 占位符：绑定 `Source` 时从 `ctx.data` 取源渲染并做淡入淡出，否则渲染 children。
pub fn placeholder_(accrete: Placeholder, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["placeholder", "f"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");
    let id_ = id.clone();

    let source = accrete
        .get_bind()
        .and_then(|x| x.get("value"))
        .and_then(|b| match &b.variant {
            BindVariant::Source { source, .. } => Some(source.clone()),
            _ => None,
        });

    move || -> AnyView {
        if let Some(source) = &source
            && let Some(data) = ctx.slot_for_data(source).get()
        {
            crate::dom::eval(&format!(
                r#"
                let x = document.getElementById('{id_extra}');
                x.classList.add('fade-in-and-out');
                setTimeout(() => x.classList.remove('fade-in-and-out'), 1000);
                "#,
                id_extra = id_
            ));
            let ctx = ctx.clone();
            div()
                .id(id_.as_str())
                .class(css.as_str())
                .child(render_accrete(&ctx, &data))
                .into_any()
        } else {
            let children = accrete
                .children
                .as_deref()
                .map(|s| render_children(&ctx, s))
                .unwrap_or_else(|| ().into_any());
            div()
                .id(id_.as_str())
                .class(css.as_str())
                .child(children)
                .into_any()
        }
    }
    .into_any()
}
