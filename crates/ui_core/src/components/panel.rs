//! 面板：一个"出现/消失的内容区"（`accrete::Panel`）。
//!
//! 这原来是三个组件，按"占不占布局"和"谁决定出现"分：
//!
//!   fold    inline 展开，撑开布局          → place: inline   + state: toggle
//!   popup   覆盖在触发器旁，不动布局        → place: anchor   + state: toggle
//!   layer   全屏遮罩挂载点，显隐＝所绑槽非空 → place: viewport + state: source
//!
//! 两条轴上一共有四格，原来只填了三格 —— 贴视口 + 本地切换（抽屉）没有写法，
//! 于是"右边滑出一个边栏"要么硬拗 popup 的位置，要么借 layer 的壳却拿不到触发器状态。
//! 现在四格都在这一个名字下面，`place`/`state` 是唯一需要记的两件事。
//!
//! 实现上这一版把前三格**转成**原来那三个组件的形状再交给它们（三份实现已经过实测，
//! 重写没有收益），第四格（抽屉）在这里实现。要把三份实现并进来是下一步的事，
//! 术语先统一：帧里只写 `panel`。

use crate::Ctx;
use crate::components::{fold_, layer_, popup_};
use crate::ctx::render_accrete;
use accrete::{
    Direction, DirectionAttr, Fold, FoldAttr, Layer, LayerAttr, Panel, PanelAttr, PanelPlace,
    PanelState, Popup,
};
use leptos::ev::keydown;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;

pub fn panel_(accrete: Panel, ctx: &Ctx, id: String) -> AnyView {
    let attrs = accrete.attrs.clone().unwrap_or_default();
    match (attrs.place, attrs.state) {
        (PanelPlace::Inline, state) => {
            if state == PanelState::Source {
                tracing::warn!(
                    "panel {id}: inline + source 没有意义 —— 撑开布局的容器没有\"空即无\"这条规则，按 toggle 画"
                );
            }
            fold_(as_fold(&accrete, &attrs), ctx, id)
        }
        // 贴触发器的浮层：显隐本来就是本地状态，source 这一格没有对应实现。
        (PanelPlace::Anchor, state) => {
            if state == PanelState::Source {
                tracing::warn!("panel {id}: anchor + source 按 toggle 画（浮层只认本地开关）");
            }
            popup_(as_popup(&accrete, &attrs), ctx)
        }
        (PanelPlace::Viewport, PanelState::Source) => layer_(as_layer(&accrete, &attrs), ctx, id),
        (PanelPlace::Viewport, PanelState::Toggle) => drawer_(accrete, ctx, id, attrs),
    }
}

fn as_fold(p: &Panel, a: &PanelAttr) -> Fold {
    Fold {
        id: p.id.clone(),
        attrs: Some(FoldAttr {
            class: a.class.clone(),
            selector: a.selector.clone(),
            replace_header: a.replace_header,
            float_body: Some(false),
        }),
        item: None,
        bind: p.bind.clone(),
        children: p.children.clone(),
    }
}

fn as_popup(p: &Panel, a: &PanelAttr) -> Popup {
    Popup {
        id: p.id.clone(),
        attrs: Some(DirectionAttr {
            class: a.class.clone(),
            selector: a.selector.clone(),
            direction: a.direction.clone(),
        }),
        bind: p.bind.clone(),
        children: p.children.clone(),
    }
}

fn as_layer(p: &Panel, a: &PanelAttr) -> Layer {
    Layer {
        id: p.id.clone(),
        attrs: Some(LayerAttr {
            class: a.class.clone(),
            selector: a.selector.clone(),
            name: a.name.clone(),
            dismiss: a.dismiss,
        }),
        bind: p.bind.clone(),
        children: p.children.clone(),
    }
}

/// 贴视口 + 本地切换：抽屉。
///
/// `children[0]` 是触发器，`children[1]` 是边栏本体（缺一个就什么都不画）。本体贴 `direction`
/// 说的那条边（缺省 L），宽度由 `attrs.width` 或 class 决定。关闭有两条路，与另外两格同一套
/// 词汇：点遮罩（`dismiss`，缺省开）与 Esc。
///
/// 动效不由框架给：本体一旦挂出来就带着 class，滑入是 CSS 的事（与 rack 的 enter 同一分工）。
fn drawer_(accrete: Panel, ctx: &Ctx, id: String, attrs: PanelAttr) -> AnyView {
    let subs = accrete.children.clone().unwrap_or_default();
    let Some(trigger) = subs.first().cloned() else {
        tracing::warn!("panel {id}: 抽屉缺触发器（children[0]），不画");
        return ().into_any();
    };
    let Some(body) = subs.get(1).cloned() else {
        tracing::warn!("panel {id}: 抽屉缺内容（children[1]），不画");
        return ().into_any();
    };

    let show = RwSignal::new(false);
    let dismiss = attrs.dismiss.unwrap_or(true);
    let edge = attrs.direction.unwrap_or(Direction::L);
    let vertical = matches!(edge, Direction::U | Direction::D);
    let size = attrs.width.clone().unwrap_or_else(|| {
        if vertical {
            "min(60vh, 100%)".to_string()
        } else {
            "min(34rem, 92vw)".to_string()
        }
    });

    // Esc 关。与 popup 同一条策略：面板的显隐是本地状态，键盘只是另一只手。
    let esc = window_event_listener(keydown, move |ev| {
        if ev.key() == "Escape" {
            show.set(false);
        }
    });
    on_cleanup(move || esc.remove());

    let align = match edge {
        Direction::L => "justify-content: flex-start;",
        Direction::R => "justify-content: flex-end;",
        Direction::U => "align-items: flex-start;",
        Direction::D => "align-items: flex-end;",
    };
    let body_style = if vertical {
        format!("height: {size}; width: 100%;")
    } else {
        format!("width: {size}; height: 100%;")
    };

    // 抽屉本体也是挂载点（`portal: to: <name>` 可以指进一个打开的边栏）。
    // 只在开着的时候登记：关着的面板没有元素可投，portal 照旧 warn 并等 epoch。
    let body_ref = NodeRef::<leptos::html::Div>::new();
    if let Some(name) = attrs.name.clone() {
        let held: StoredValue<Option<web_sys::Element>> = StoredValue::new(None);
        let name_reg = name.clone();
        Effect::new(move |_| {
            if show.get() {
                if held.get_value().is_some() {
                    return;
                }
                if let Some(el) = body_ref.get() {
                    let node: web_sys::Element = el.into();
                    crate::components::mounts::register(&name_reg, node.clone());
                    held.set_value(Some(node));
                }
            } else if let Some(node) = held.get_value() {
                crate::components::mounts::unregister(&name_reg, &node);
                held.set_value(None);
            }
        });
        let name_clean = attrs.name.clone().unwrap_or_default();
        on_cleanup(move || {
            if let Some(node) = held.get_value() {
                crate::components::mounts::unregister(&name_clean, &node);
            }
        });
    }

    let trigger_ctx = ctx.clone();
    let body_ctx = ctx.clone();
    view! {
        <div id=id class="panel f col">
            <div
                class="panel-trigger f"
                on:click=move |ev: web_sys::MouseEvent| {
                    ev.stop_propagation();
                    show.update(|v| *v = !*v);
                }
            >
                {render_accrete(&trigger_ctx, &trigger)}
            </div>
            {move || {
                if !show.get() {
                    return ().into_any();
                }
                let mask = if dismiss {
                    Some(view! {
                        <div
                            class="panel-mask"
                            style="position: absolute; inset: 0; background: rgba(0, 0, 0, 0.28);"
                            on:click=move |_| show.set(false)
                        ></div>
                    })
                } else {
                    None
                };
                view! {
                    <div
                        class="panel-viewport"
                        style=format!(
                            "position: fixed; inset: 0; height: 100vh; z-index: 100; display: flex; {align} align-items: stretch;"
                        )
                    >
                        {mask}
                        <div
                            class="panel-body f col"
                            node_ref=body_ref
                            style=format!(
                                "position: relative; {body_style} background: var(--bg, Canvas); box-shadow: 0 0 2rem rgba(0,0,0,.25); overflow: auto;"
                            )
                            on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()
                        >
                            {render_accrete(&body_ctx, &body)}
                        </div>
                    </div>
                }
                .into_any()
            }}
        </div>
    }
    .into_any()
}