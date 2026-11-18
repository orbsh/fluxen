//! 框：结构容器，也是数据区域的唯一形态（ADR 0027）。
//!
//! 这里原来有两个组件——`case`（画 children）和 `placeholder`（绑 `Source` 时
//! 画槽里的树，否则画 children）。它们不是两种东西：`placeholder` 的实现是
//! `case` 的超集，差别只有"绑了 source 读不读槽"。把它们分成两个组件等于要求
//! 作者在写布局时预判这块将来会不会被数据填——而今天静态、明天动态，类型就得
//! 跟着改。绑定能后加，类型不能。
//!
//! 所以只剩 `frame`：
//!
//!   bind.value.kind = source   槽有值 → 画槽里的那棵树（带一次淡入淡出），
//!                              槽空 → 画 children（默认内容）
//!   无 bind                    画 children
//!
//! `attrs.grid` 与 `attrs.class` 两条路都保留：grid 时只进 `g`、否则只进 `f`，
//! 因为 `.f` 与 `.g` 在同一层里 `.f` 在后，同时存在会把 `display:grid` 压掉
//! （`attrs.grid` 自出生起没生效过，没人用 grid 才没人踩到）。

use crate::Ctx;
use crate::ctx::{render_accrete, render_children};
use crate::hooks::use_common_css;
use accrete::{AccreteOps, BindVariant, Frame, FrameAttr};
use leptos::html::*;
use leptos::prelude::*;

/// 槽名（`bind.value.source`），没有绑定就是 `None`。
fn source_of(accrete: &Frame) -> Option<String> {
    accrete
        .get_bind()
        .and_then(|x| x.get("value"))
        .and_then(|b| match &b.variant {
            BindVariant::Source { source, .. } => Some(source.clone()),
            _ => None,
        })
}

pub fn frame_(accrete: Frame, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["frame"];
    if let Some(id) = &accrete.id {
        css.push(id.as_str());
    }

    let mut style = String::new();
    let mut use_grid = false;
    if let Some(FrameAttr { grid, .. }) = &accrete.attrs
        && let Some(g) = grid
    {
        use_grid = true;
        css.push("g");
        style = g
            .iter()
            .map(|(k, v)| format!("{}: {};", k, v.as_str().unwrap_or("")))
            .collect::<Vec<String>>()
            .join("\n");
    }
    if !use_grid {
        css.push("f");
    }
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let children = accrete.children.clone().unwrap_or_default();
    let Some(source) = source_of(&accrete) else {
        return div()
            .class(css.as_str())
            .style(style)
            .child(render_children(&ctx, &children))
            .into_any();
    };

    // 数据区域：整个容器是槽的镜子。`fade-in-and-out` 是 main.css 里的一次性
    // 动画——元素随槽值重建，动画自己重播，不需要 JS 去加/删类。
    move || -> AnyView {
        match ctx.slot_for_data(&source).get() {
            Some(data) => div()
                .class(format!("{css} fade-in-and-out"))
                .style(style.clone())
                .child(render_accrete(&ctx, &data))
                .into_any(),
            None => div()
                .class(css.clone())
                .style(style.clone())
                .child(render_children(&ctx, &children))
                .into_any(),
        }
    }
    .into_any()
}