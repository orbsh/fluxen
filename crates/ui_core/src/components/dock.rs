//! 拖拽的落点（ADR 0019）。
//!
//! 落点同时是三件事的所有者：**事件名**（`bind["value"]` 上的 `event`）、
//! **落点身份**（帧的 `id` 就是它自己的 id）、以及**收什么**（`attrs.accept`
//! 与 `grab.kind` 配对，缺席即全收）。上行在这一侧发射——drop 事件本来就落在
//! 它的 DOM 上，不需要跨组件回调；「被拖来的是什么」跟着进 `data`。
//!
//! 三态都是本组件自己的：常态（`expand` 决定细条还是占位）、拖拽在飞
//! （`Ctx.drag` 有货，全局展开）、悬停在上（本地 `over`，只做高亮）。展开不动
//! 布局——宽度在覆盖层里长，否则光标下面的内容会移走。

use crate::Ctx;
use crate::ctx::{DragCargo, render_children};
use crate::hooks::use_common_css;
use accrete::{BindVariant, Dock, DockExpand};
use leptos::html::*;
use leptos::prelude::*;
use serde_json::{Map, Value};

/// 这个落点收不收这件货：`accept` 缺席 = 全收；写了就是闭集合比对，
/// 没有 `kind` 的货配不上任何一个写了 `accept` 的落点。
fn accepts(accept: &Option<Vec<String>>, cargo: &DragCargo) -> bool {
    match accept {
        None => true,
        Some(list) => cargo
            .kind
            .as_ref()
            .is_some_and(|k| list.iter().any(|x| x == k)),
    }
}

pub fn dock_(accrete: Dock, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["dock", "f"];
    use_common_css(&mut css, &accrete);
    css.push(
        match accrete.attrs.as_ref().map(|a| a.expand).unwrap_or_default() {
            DockExpand::Reveal => "dock-reveal",
            DockExpand::Reserved => "dock-reserved",
        },
    );
    let base = css.join(" ");

    let accept = accrete.attrs.as_ref().and_then(|a| a.accept.clone());
    // 落点元数据：挂在同一条 bind 的 `default` 上（bind 的 `event`/`slot` 已经
    // 用掉了变体，`default` 在这条 bind 上不表示初值）。
    let meta = accrete
        .bind
        .as_ref()
        .and_then(|b| b.get("value"))
        .and_then(|b| b.default.clone());
    let event = accrete
        .bind
        .as_ref()
        .and_then(|b| b.get("value"))
        .map(|b| b.variant.clone())
        .filter(|v| matches!(v, BindVariant::Event { .. } | BindVariant::Local { .. }));

    let over = RwSignal::new(false);

    // dragenter 与 dragover 都要 preventDefault：只挡其中一个，浏览器仍会
    // 把这次释放当成「不可落」，drop 根本不触发。两处用的是同一段判断，
    // 闭包不能克隆，所以由同一个工厂造两份。
    let make_enter = || {
        let ctx = ctx.clone();
        let accept = accept.clone();
        move |ev: web_sys::DragEvent| {
            let ok = ctx
                .drag
                .get_untracked()
                .is_some_and(|c| accepts(&accept, &c));
            if ok {
                ev.prevent_default();
                over.set(true);
            }
        }
    };
    let on_enter = make_enter();
    let on_over = make_enter();
    let on_leave = move |_ev: web_sys::DragEvent| over.set(false);

    let on_drop = {
        let ctx = ctx.clone();
        let accept = accept.clone();
        let id = id.clone();
        move |ev: web_sys::DragEvent| {
            ev.prevent_default();
            over.set(false);
            let Some(cargo) = ctx.drag.get_untracked() else {
                return;
            };
            if !accepts(&accept, &cargo) {
                return;
            }
            let mut data = Map::new();
            if let Some(item) = &cargo.item {
                data.insert("item".into(), Value::String(item.clone()));
            }
            if let Some(kind) = &cargo.kind {
                data.insert("kind".into(), Value::String(kind.clone()));
            }
            if let Some(payload) = &cargo.payload {
                data.insert("payload".into(), payload.clone());
            }
            if let Some(meta) = &meta {
                data.insert("meta".into(), meta.clone());
            }
            if let Some(event) = &event {
                // 帧：`event` = 落点声明的名字，`id` = 落点自己，
                // `data` = {item, kind, payload, meta}（空即无，ADR 0012）。
                ctx.emit(event, Some(id.clone()), Value::Object(data));
            }
            // 正常路径上源的 dragend 会清；这里再清一次，免得 drop 之后有
            // 谁把会话留在「在飞」状态。
            ctx.drag.set(None);
        }
    };

    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(&ctx, s))
        .unwrap_or_default();

    // 类名是唯一随拖拽状态变的那一处：`dock-armed` 跟会话，`dock-over` 跟本地
    // 悬停。子树只建一次——若把整棵 div 包在闭包里重建，每次 dragenter 都会
    // 重建落点里的内容，滚动位置和本地态跟着丢。
    let ctx_cls = ctx.clone();
    let class = move || {
        let mut cls = base.clone();
        if ctx_cls.drag.get().is_some() {
            cls.push_str(" dock-armed");
        }
        if over.get() {
            cls.push_str(" dock-over");
        }
        cls
    };

    div()
        .attr("class", class)
        .on(leptos::ev::dragenter, on_enter)
        .on(leptos::ev::dragover, on_over)
        .on(leptos::ev::dragleave, on_leave)
        .on(leptos::ev::drop, on_drop)
        .child(div().class("dock-panel").child(children))
        .into_any()
}