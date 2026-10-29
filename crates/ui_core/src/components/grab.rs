//! 拖拽的源（ADR 0019）。
//!
//! 一层**不加样式**的包装：它不设 padding、边框、背景，也不消费额外尺寸——尺寸
//! 全由 children 给。它唯一说的是两件事：这件东西可以被拖走，以及它的货物是什么。
//!
//! 拖动本身不上报。`dragstart` 把货物写进 `Ctx` 的页面级拖拽会话，上行发生在
//! 落点（`dock_`）——手势归浏览器，那一帧归落点。

use crate::Ctx;
use crate::ctx::render_children;
use accrete::Grab;
use leptos::html::*;
use leptos::prelude::*;

pub fn grab_(accrete: Grab, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let drag = ctx.drag;

    // 会话里放的就是这个节点本身：id／kind／payload 不复制第二份，否则两份迟早
    // 对不上（ADR 0019）。节点在渲染期读一次就够——这几个字段都是字面值。
    let node = accrete.clone();
    let on_start = {
        let node = node.clone();
        move |ev: web_sys::DragEvent| {
            // Firefox 不在 dataTransfer 里写东西就不认这次拖；货物本身走会话，
            // 不塞 dataTransfer（那儿只放一个能看的标签）。
            if let Some(dt) = ev.data_transfer() {
                dt.set_effect_allowed("move");
                let _ = dt.set_data("text/plain", node.kind.as_deref().unwrap_or("item"));
            }
            drag.set(Some(node.clone()));
        }
    };
    let on_end = move |_ev: web_sys::DragEvent| drag.set(None);

    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(&ctx, s))
        .unwrap_or_default();

    div()
        .class("grab f")
        .attr("draggable", "true")
        .on(leptos::ev::dragstart, on_start)
        .on(leptos::ev::dragend, on_end)
        .child(children)
        .into_any()
}