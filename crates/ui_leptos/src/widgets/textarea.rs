use crate::Ctx;
use crate::hooks::{use_common_css, use_source_untracked, use_target_value};
use accrete::TextArea;
use leptos::ev;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::{Value, to_value};
use wasm_bindgen::JsCast;

/// 多行输入：初值取 `bind["value"]`（Source/Local/default 统一经
/// use_source 解析），Enter 经统一 `Ctx::emit` 路由落点
/// （`kind: event` 上行 / `kind: local` 写本地值槽，ADR 0008）。
pub fn textarea_(accrete: TextArea, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["textarea", "shadow"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    // 发射组件初值一律 untracked 读：本函数运行在父级 layout 渲染闭包
    // 内，tracked 读会把父闭包订阅到自己 emit 写的槽上——emit → 父闭包
    // 重跑 → 整棵子树重建、刚发送的内容从槽里"还原"（e2e 抓到的回路；
    // 订阅联动是 text/placeholder 这类纯读方的事，它们在 render 闭包里
    // tracked 读）。
    let initial = use_source_untracked(&ctx, &accrete, "value").unwrap_or_else(|| to_value("").unwrap());
    let slot = RwSignal::new(initial);
    let placeholder =
        use_source_untracked(&ctx, &accrete, "placeholder").and_then(|d| d.as_str().map(String::from));

    move || -> AnyView {
        let ctx = ctx.clone();
        let accrete = accrete.clone();
        let p = placeholder.clone();
        let oninput = move |event: web_sys::Event| {
            let v = event
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                .map(|e| e.value())
                .unwrap_or_default();
            slot.set(to_value(v).unwrap());
        };
        let onkeydown = move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Enter" {
                let emitter = use_target_value(ctx.clone(), &accrete);
                if emitter.is_none() {
                    return; // 无落点绑定：textarea 纯本地编辑，Enter 不做事
                }
                let val = slot.get_untracked();
                // 空值回车不触发（与 input_ 同约定）
                if val.as_str().is_some_and(|s| s.trim().is_empty()) {
                    return;
                }
                // 先清空再 emit：上行 future 可能迟迟不 resolve，信号写
                // 必须在 send 之前落定（dioxus 移植教训）。
                slot.set(to_value("").unwrap());
                // 命令式清空 DOM：用户键入脏化 defaultValue 后，属性级
                // 更新不再重置显示值，本节点全程存活（同 input_ 缺陷修复）。
                if let Some(el) = ev
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                {
                    el.set_value("");
                }
                let _ = emitter.map(|e| e(val));
            }
        };

        // 显示不订阅 slot（slot 只是 emit 前的暂存）：用户键入即脏值，
        // 任何响应式文本复写都会把已发送内容"还原"（e2e 抓到）。
        // 初值在组件构建时取一次；Enter 后命令式清空。
        let val = slot.get_untracked();
        let text = match &val {
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => val.as_str().unwrap_or("").to_string(),
        };
        let base = textarea()
            .class(css.as_str())
            .child(text)
            .on(ev::input, oninput)
            .on(ev::keydown, onkeydown);
        match p {
            Some(ph) => base.placeholder(ph.as_str()).into_any(),
            None => base.into_any(),
        }
    }
    .into_any()
}
