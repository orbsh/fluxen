//! 弹窗：触发器 + 覆盖层（ADR 0026）。
//!
//! 曾经的显隐是 CSS 的 `:hover`（`main.css` 里 `.popup:hover > .body`）——那是一条
//! 只有指针设备才有的状态：触屏没有 hover，弹窗根本打不开，而触屏恰恰是最需要
//! "点一下弹出来"的地方。现在状态是本地信号 `show`：触发器点击切换，Esc 与点击
//! 外部关闭。
//!
//! 三个容器的分工按"这一层占不占布局"和"谁决定它出现"来分，不按名字：
//!
//!   fold    inline 展开，撑开布局，状态跟着节点走
//!   popup   覆盖在触发器旁，不动布局
//!   layer   全屏遮罩挂载点，显隐＝所绑槽非空
//!
//! 所以 popup 不并进 fold（会挤走下面的内容），也不并进 layer（那是模态，不是
//! 贴着触发器的浮层）。三者都该由信号驱动，没有一个该靠 CSS 状态。
//!
//! `sub[0]` 是触发器，`sub[1]` 是浮层内容；缺一个就什么都不画（少写一个子节点是
//! 帧的错误，不是"可以渲染半个弹窗"）。

use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::use_common_css;
use accrete::Popup;
use leptos::ev::click;
use leptos::html::*;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

pub fn popup_(accrete: Popup, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["popup", "flex"];
    use_common_css(&mut css, &accrete);
    let base = css.join(" ");
    let style = accrete
        .attrs
        .as_ref()
        .map(|x| x.into_style())
        .unwrap_or_default();

    let subs = accrete.children.clone().unwrap_or_default();
    let trigger = subs.first().cloned();
    let body = subs.get(1).cloned();

    let show = RwSignal::new(false);
    let root = NodeRef::<Div>::new();

    // 关闭的两条腿：Esc，以及"点到浮层外面"。两者都挂在 window 上——浮层是覆盖
    // 层，点击可能落在它外面的任何地方，只在容器里监听会漏掉。监听器跟着组件的
    // 作用域销毁（句柄在 cleanup 里摘掉），否则每次重建都会多一个。
    {
        let on_key = window_event_listener(leptos::ev::keydown, move |ev| {
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