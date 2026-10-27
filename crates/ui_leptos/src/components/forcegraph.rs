//! 关系图组件（ADR 0018）：原生 leptos + wasm 渲染器，不是 canvas 模块。
//!
//! 这里的代码只做三件事——建宿主元素、把数据槽的载荷喂给渲染器、把渲染器
//! 发出的事件交给 Ctx。渲染、布局、相机、拾取全在 `forcegraph` crate 里，
//! 它不认识 Accrete，也不认识 leptos。

use std::cell::RefCell;
use std::rc::Rc;

use accrete::classify::Classify;
use accrete::{ForceGraph, ForceGraphStyle};
use leptos::html::Div;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

use crate::{Ctx, hooks};

pub fn forcegraph_(accrete: ForceGraph, ctx: &Ctx, id: String) -> AnyView {
    let style = style_of(&accrete.style);
    let css = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.get_class().clone().map(|c| c.join(" ")))
        .unwrap_or_default();
    let size = accrete
        .attrs
        .as_ref()
        .map(|a| a.size_style())
        .unwrap_or_default();
    // 一个有着色器的画布必须有实尺寸：没给 attrs 时也要给一个可画的高度，
    // 否则 canvas 是 0×0，画面"什么都没有"却不是渲染失败。
    let size = if size.is_empty() {
        "height: 360px;".to_string()
    } else {
        size
    };

    let nr = NodeRef::<Div>::new();
    let host_style = size.clone();
    let view: Rc<RefCell<Option<forcegraph::View>>> = Rc::new(RefCell::new(None));

    {
        // `on_cleanup` wants a `Send` closure; the view is thread-local by
        // nature (GL context, DOM), so it rides a `SendWrapper` — cleanup runs
        // on the main thread in CSR, where the wrapper is transparent.
        let view = send_wrapper::SendWrapper::new(Rc::clone(&view));
        on_cleanup(move || {
            if let Some(mut v) = view.borrow_mut().take() {
                v.dispose();
            }
        });
    }

    let node_id = id.clone();
    let ctx_load = ctx.clone();
    let accrete_load = accrete.clone();
    let view_load = Rc::clone(&view);
    let nr_load = nr;
    nr_load.on_load(move |el| {
        let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() else {
            tracing::warn!("forcegraph {node_id}: host element is not an HtmlElement");
            return;
        };
        let emit_id = node_id.clone();
        let emit_ctx = ctx_load.clone();
        let mounted = forcegraph::View::mount(&el, style, move |event, data| {
            let ctx = emit_ctx.clone();
            let id = emit_id.clone();
            let event = event.to_string();
            // 与 Form 的 submit 同一条上行腿：事件名 + 载荷，宿主不解释。
            leptos::task::spawn_local(async move {
                ctx.send(event, Some(id), data).await;
            });
        });
        match mounted {
            Some(v) => *view_load.borrow_mut() = Some(v),
            None => {
                tracing::warn!("forcegraph {node_id}: WebGL2 context unavailable");
                el.set_attribute("data-forcegraph-failed", "webgl2").ok();
                return;
            }
        }

        // 数据区：载荷是 `{nodes, links, focus?}`，整份快照推给渲染器。
        // 空载荷 = 不占位（ADR 0012），与 canvas/chart 同一条规则。
        let ctx2 = ctx_load.clone();
        let view2 = Rc::clone(&view_load);
        let accrete2 = accrete_load.clone();
        let host_style = host_style.clone();
        let host_el = el.clone();
        Effect::new(move |_| {
            let payload = hooks::use_source(&ctx2, &accrete2, "value");
            let empty = payload
                .as_ref()
                .map(super::canvas::payload_is_empty)
                .unwrap_or(true);
            // 占位随载荷生效（ADR 0012）：空 data 区 = 收起容器，非空写回
            // 节点自己的尺寸。改属性而不是加类：`.f` 与 `.hide` 会互相压制。
            let want = if empty {
                super::canvas::HIDDEN_STYLE.to_string()
            } else {
                host_style.clone()
            };
            host_el.set_attribute("style", &want).ok();
            let Some(value) = payload else { return };
            if empty {
                return;
            }
            let mut holder = view2.borrow_mut();
            let Some(view) = holder.as_mut() else {
                return;
            };
            match serde_json::from_value::<forcegraph::Snapshot>(value) {
                Ok(snapshot) => view.set_payload(&snapshot),
                Err(e) => tracing::warn!("forcegraph payload rejected: {e}"),
            }
        });
    });

    let mut classes = vec!["box"];
    if !css.is_empty() {
        classes.push(css.as_str());
    }
    let class = classes.join(" ");
    view! {
        <div class=class style=size node_ref=nr></div>
    }
    .into_any()
}

/// DSL 旋钮 → 渲染器旋钮。两侧字段一一对应，唯一的映射点在这里。
fn style_of(style: &ForceGraphStyle) -> forcegraph::Style {
    forcegraph::Style {
        node_size_px: style.node_size_px,
        node_size_step_px: style.node_size_step_px,
        node_tiers: style.node_tiers,
        link_width_px: style.link_width_px,
        link_opacity: style.link_opacity,
        charge: style.charge,
        link_distance: style.link_distance,
        link_strength: style.link_strength,
        velocity_decay: style.velocity_decay,
        labels: style.labels,
        label_px: style.label_px,
        label_max_nodes: style.label_max_nodes,
        auto_fit: style.auto_fit,
        fit_padding_px: style.fit_padding_px,
        background: style.background.clone(),
        palette: style.palette.clone(),
    }
}

/// `Value` → 快照，给没有 serde 直接路径的调用方（测试）用。
pub fn snapshot_of(value: &Value) -> Option<forcegraph::Snapshot> {
    serde_json::from_value(value.clone()).ok()
}