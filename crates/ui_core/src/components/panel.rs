//! 面板：一个"出现/消失的内容区"（`accrete::Panel`）。
//!
//! 帧里只有 `panel` 一个名字，两条轴说清它是什么（ADR 0030）：
//!
//! ```text
//!   place: inline    占布局，展开时把下面的内容推下去     state: toggle   （原 fold）
//!   place: anchor    贴在触发器旁浮起，页面不动           state: toggle   （原 popup）
//!   place: viewport  盖住视口（遮罩 + 贴边或居中）        state: bound    （原 layer）
//!   place: viewport  同上，显隐由本地开关决定             state: toggle   （抽屉）
//! ```
//!
//! 这原来是三个文件（`fold.rs` / `popup.rs` / `layer.rs`），每种形状一套自己的几何与状态机，
//! 实测重写没有收益。所以三份实现按原样搬进本文件，作为私有函数与 `drawer_` 并列：
//!
//! ```text
//!   inline_   place: inline   state: toggle   —— 一行 grid：图标 + 头 + 体
//!   anchor_   place: anchor   state: toggle   —— 相对定位的触发器 + 绝对定位的浮层
//!   overlay_  place: viewport state: bound    —— 内联几何 + "所绑槽非空即显示"
//!   drawer_   place: viewport state: toggle   —— 贴边的边栏 + 遮罩点击 / Esc
//! ```
//!
//! 函数名取轴上的词（inline / anchor / overlay），不取退休的节点名（fold / popup / layer）：
//! 本文件里一个词只指一件事，而 `place` 说的正是它放在哪。CSS 类名（`.popup`、`.layer`、
//! `.panel-trigger` …）不改：那是页面样式表的词，不是模型里的词。
//!
//! 子节点的约定按 `place` 分：`inline` 的 `children` 全是体（整行就是开关）；其余三格
//! `children[0]` 是触发器、`children[1]` 是本体（`state: bound` 时触发器可省，因为显隐
//! 由槽决定）。少写本体是帧的错误，不是"可以渲染半个面板"。

use crate::Ctx;
use crate::components::canvas::{HIDDEN_STYLE, payload_is_empty};
use crate::ctx::{render_accrete, render_children};
use crate::hooks::{use_common_css, use_default};
use crate::rough;
use accrete::{AccreteOps, Bind, Direction, Panel, PanelAttr, PanelPlace, PanelState};
use leptos::ev::{click, keydown};
use leptos::html::*;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

pub fn panel_(accrete: Panel, ctx: &Ctx, id: String) -> AnyView {
    let attrs = accrete.attrs.clone().unwrap_or_default();
    match (attrs.place, attrs.state) {
        (PanelPlace::Inline, state) => {
            if state == PanelState::Bound {
                tracing::warn!(
                    "panel {id}: inline + bound 没有意义 —— 撑开布局的容器没有\"空即无\"这条规则，按 toggle 画"
                );
            }
            inline_(accrete, ctx, id, attrs)
        }
        // 贴触发器的浮层：显隐本来就是本地状态，bound 这一格没有对应实现。
        (PanelPlace::Anchor, state) => {
            if state == PanelState::Bound {
                tracing::warn!("panel {id}: anchor + bound 按 toggle 画（浮层只认本地开关）");
            }
            anchor_(accrete, ctx, id, attrs)
        }
        (PanelPlace::Viewport, PanelState::Bound) => overlay_(accrete, ctx, id, attrs),
        (PanelPlace::Viewport, PanelState::Toggle) => drawer_(accrete, ctx, id, attrs),
    }
}

/// `place: inline` + `state: toggle`：撑开布局的折叠区。
///
/// 一行 grid（图标 / 头 / 体第 3/4 列），整行可点。`children` 全是体 —— 这一格没有触发器
/// 与本体之分，打开就是把体放进网格；所以 `attrs.replace_header` 在这里没有头可换（它管的
/// 是 `item` 那份头，随 `fold` 一起退休了）。初始开合读 `source.default` 的布尔值。
fn inline_(accrete: Panel, ctx: &Ctx, id: String, _attrs: PanelAttr) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["grid"];
    css.push(id.as_str());
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let show = RwSignal::new(
        use_default(&accrete)
            .and_then(|x| x.as_bool())
            .unwrap_or_default(),
    );
    let children = accrete.children.clone().unwrap_or_default();

    move || -> AnyView {
        let s = show.get();
        let onclick = move |_: web_sys::MouseEvent| {
            show.set(!show.get());
        };

        let b: AnyView = if s {
            div().child(render_children(&ctx, &children)).into_any()
        } else {
            div().into_any()
        };

        let icon_class = if s { "icon open" } else { "icon close " };
        div()
            .id(id.as_str())
            .class(css.as_str())
            .style("grid-template-columns: auto 1fr;".to_string())
            .on(click, onclick)
            .child(
                div()
                    .class(icon_class)
                    .style("height: 100%; aspect-ratio: 1 / 1;".to_string()),
            )
            .child(div())
            .child(div())
            .child(b)
            .into_any()
    }
    .into_any()
}

/// `place: anchor` + `state: toggle`：贴着触发器的浮层（ADR 0026）。
///
/// 曾经的显隐是 CSS 的 `:hover`（`core.css` 里 `.popup:hover > .body`）——那是一条只有指针
/// 设备才有的状态：触屏没有 hover，浮层根本打不开。现在状态是本地信号 `show`：触发器点击
/// 切换，Esc 与点击外部关闭。`sub[0]` 是触发器，`sub[1]` 是浮层内容；缺一个就什么都不画。
fn anchor_(accrete: Panel, ctx: &Ctx, id: String, attrs: PanelAttr) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["popup", "flex"];
    use_common_css(&mut css, &accrete);
    let base = css.join(" ");
    let style = attrs.direction_style();

    let subs = accrete.children.clone().unwrap_or_default();
    let trigger = subs.first().cloned();
    let body = subs.get(1).cloned();

    let show = RwSignal::new(false);
    let root = NodeRef::<Div>::new();

    // 关闭的两条腿：Esc，以及"点到浮层外面"。两者都挂在 window 上——浮层是覆盖层，点击可能
    // 落在它外面的任何地方，只在容器里监听会漏掉。监听器跟着组件的作用域销毁（句柄在 cleanup
    // 里摘掉），否则每次重建都会多一个。
    {
        let on_key = window_event_listener(keydown, move |ev| {
            if ev.key() == "Escape" {
                show.set(false);
            }
        });
        let on_click = window_event_listener(click, move |ev| {
            if !show.get_untracked() {
                return;
            }
            // 点在自己的子树里（含触发器）不算外面；触发器自己会把状态翻过去。
            let inside = root
                .get_untracked()
                .zip(ev.target().and_then(|t| t.dyn_into::<web_sys::Node>().ok()))
                .map(|(el, target)| el.contains(Some(&target)))
                .unwrap_or(false);
            if !inside {
                show.set(false);
            }
        });
        on_cleanup(move || {
            on_key.remove();
            on_click.remove();
        });
    }

    let trigger_view = {
        let ctx = ctx.clone();
        move || match &trigger {
            Some(t) => render_accrete(&ctx, t),
            None => ().into_any(),
        }
    };
    let body_view = {
        let ctx = ctx.clone();
        move || -> AnyView {
            if !show.get() {
                return ().into_any();
            }
            match &body {
                Some(b) => {
                    let ctx = ctx.clone();
                    div()
                        .class("flex body")
                        .child(render_accrete(&ctx, b))
                        .into_any()
                }
                None => ().into_any(),
            }
        }
    };

    div()
        // 身份：一个面板要能被找到（`portal` 指进来、别人的选择器指过来、调试时看得到）。
        .id(id)
        .class(move || {
            if show.get() {
                format!("{base} open")
            } else {
                base.clone()
            }
        })
        .style(style)
        .node_ref(root)
        .child(
            div()
                .class("flex trigger")
                .on(click, move |_| show.update(|s| *s = !*s))
                .child(trigger_view),
        )
        .child(body_view)
        .into_any()
}

/// `place: viewport` + `state: bound`：模态层（ADR 0014）。
///
/// 显隐只有一条规则：**所绑槽非空即显示**（ADR 0012 的"空即无"）。`dismiss` 点层自身
/// （模糊区，而非模态内容）时**清空所绑的槽**——清空即关，不新造第二个掩蔽状态；槽空了，
/// 下次写入必是新值，同值短路不会误挡重开。故 dismiss 需要槽绑定：inline default 的层无槽
/// 可清，只 warn。
///
/// 几何必须内联，两条实测教训：① custom 层的定序缺陷在 core.css 层序声明归位前会输给
/// element 层（PLAN §19），而内联本无层叠歧义，与 ADR 0012 的 display 内联同款；② 本机
/// headless chromium 上 `position: fixed` + `inset: 0` 的高度不撑开（干净 div 实测 20px），
/// 要显式给 `height`。遮罩（模糊/色调）走 custom.css 的 `.layer::before`。
fn overlay_(accrete: Panel, ctx: &Ctx, id: String, attrs: PanelAttr) -> AnyView {
    let dismiss = attrs.dismiss.unwrap_or(false);
    let mut css = vec!["layer", "flex"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let (local_slot, data_source, inline) = bind_slot(accrete.source.as_ref());
    // 位置：给了 `right`/`top` 之类就贴在页面那一角（原来的 `float` 就是这个），没给就铺满
    // 视口居中 —— 两种都是"层"，只是几何不同。
    let shown_style = attrs
        .pos
        .as_ref()
        .map(|p| format!("position: fixed; {} z-index: 100; display: flex;", p.into_style()))
        .unwrap_or_else(|| LAYER_STYLE.to_string());
    // 构建那一刻的值（首次 Effect 前）：与 canvas 宿主同款，交给下面的 Effect 同步
    let style_now = if payload_is_empty(&read_payload(ctx, &local_slot, &data_source, &inline)) {
        HIDDEN_STYLE.to_string()
    } else {
        shown_style.clone()
    };

    let nr = NodeRef::<Div>::new();
    {
        let ctx = ctx.clone();
        let (l, s, i) = (local_slot.clone(), data_source.clone(), inline.clone());
        let shown = shown_style.clone();
        Effect::new(move |_| {
            let want = if payload_is_empty(&read_payload(&ctx, &l, &s, &i)) {
                HIDDEN_STYLE.to_string()
            } else {
                shown.clone()
            };
            if let Some(el) = nr.get() {
                el.set_attribute("style", &want).ok();
            }
        });
    }

    // 闭包要 'static：Ctx 与 id 都得先克隆进作用域（ctx 是 Rc，clone 即计数加一）
    let onclick = {
        let ctx = ctx.clone();
        let id = id.clone();
        move |ev: web_sys::MouseEvent| {
            if !dismiss {
                return;
            }
            // e.target === e.currentTarget：点的是层自己的空白（模糊面），点模态内容不算——
            // 事件在冒泡路径上也会走到这里，判等把内容排掉。
            let same = match (ev.target(), ev.current_target()) {
                (Some(t), Some(c)) => {
                    // 显式要 JsValue 视图：EventTarget 的 AsRef 有三个 impl，不标注会撞 E0283
                    let tv: &wasm_bindgen::JsValue = t.as_ref();
                    let cv: &wasm_bindgen::JsValue = c.as_ref();
                    tv == cv
                }
                _ => false,
            };
            if !same {
                return;
            }
            if let Some(slot) = &local_slot {
                ctx.slot_for_value(slot).set(None);
            } else if let Some(src) = &data_source {
                ctx.slot_for_data(src).set(None);
            } else {
                tracing::warn!(
                    "panel {id}: dismiss 需要槽绑定（清空槽 = 关），inline default 无槽可清"
                );
            }
        }
    };

    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();

    // 手绘皮肤（ADR 0038 §4）：层只有自己声明了 `is-border` 才有。
    let layer = rough::border_layer(
        ctx,
        accrete.attrs.as_ref().and_then(|a| a.class.as_ref()),
        Some(&id),
    );
    div()
        .id(id)
        .class(css.as_str())
        .style(style_now)
        .on(click, onclick)
        .child(children)
        .child(layer)
        .node_ref(nr)
        .into_any()
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

    // Esc 关。与 anchor 同一条策略：面板的显隐是本地状态，键盘只是另一只手。
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

    let trigger_ctx = ctx.clone();
    let body_ctx = ctx.clone();
    view! {
        <div id=id class="panel flex col">
            <div
                class="panel-trigger flex"
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
                            class="panel-body flex col"
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

/// 层几何：铺满视口 + 居中模态内容。z-index 高于普通浮层。
const LAYER_STYLE: &str = "position: fixed; inset: 0; height: 100vh; z-index: 100; \
                           display: flex; align-items: center; justify-content: center;";

/// `source` → (local 槽名, data 槽名, inline 默认值), decided by `scope` (ADR 0031 §1).
/// `both` gives both addresses: the local assignment wins while it exists, the far side is
/// the fallback.
fn bind_slot(bind: Option<&Bind>) -> (Option<String>, Option<String>, Option<Value>) {
    match bind {
        Some(b) => match &b.slot {
            Some(slot) if b.scope.local() => (
                Some(slot.clone()),
                b.scope.remote().then(|| slot.clone()),
                None,
            ),
            Some(slot) => (None, Some(slot.clone()), None),
            None => (None, None, b.default.clone()),
        },
        None => (None, None, None),
    }
}

/// 当前载荷：local 槽裸值 / data 槽节点（其 `source.default` 优先，否则整节点 JSON）/ literal
/// 默认值。槽没写过 = Null = 空（"空即无"的判据见 payload_is_empty）。
fn read_payload(
    ctx: &Ctx,
    local: &Option<String>,
    source: &Option<String>,
    inline: &Option<Value>,
) -> Value {
    // `scope: both`: the local assignment is the draft, the remote slot the fallback — an
    // empty local plane must not hide a value that did arrive.
    if let Some(slot) = local
        && let Some(v) = ctx.slot_for_value(slot).get()
    {
        return v;
    }
    if let Some(src) = source {
        return match ctx.slot_for_data(src).get().map(|a| (*a).clone()) {
            Some(node) => node
                .get_source()
                .and_then(|v| v.default.clone())
                .unwrap_or_else(|| serde_json::to_value(&node).unwrap_or(Value::Null)),
            None => Value::Null,
        };
    }
    inline.clone().unwrap_or(Value::Null)
}