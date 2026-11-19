use crate::Ctx;
use crate::hooks::{node_id, use_common_css};
use accrete::{AccreteOps, Input, JsType};
use leptos::ev;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

fn default_option_jskind(v: &Option<JsType>) -> Value {
    v.as_ref()
        .map(|x| x.default_value())
        .unwrap_or_else(|| Value::String(String::new()))
}

/// 输入框：`sink` 是本字段的去向（ADR 0031 §2）。`scope: local` 且外层
/// 表单认领了这个槽名 = 写表单字段；其余情况在 Enter 时经 `Ctx::emit`
/// 按 scope 路由（本页值槽 / 上行 / 两者）。
pub fn input_(accrete: Input, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let node_id = node_id(&accrete);
    let mut css = vec!["input", "f", "shadow"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let sink = accrete.get_sink().cloned();
    // 类型可以在任一通道上：读侧给显示，写侧给上行载荷的解析。
    let kind = sink
        .as_ref()
        .and_then(|s| s.r#type.clone())
        .or_else(|| accrete.get_source().and_then(|b| b.r#type.clone()));
    // 表单字段 = 本页赋值且外层表单认领了这个槽名（槽名即字段名）。
    let field_sig = sink
        .as_ref()
        .filter(|s| s.scope.local())
        .and_then(|s| s.slot.as_ref())
        .and_then(|k| ctx.form.as_ref().and_then(|fs| fs.fields.get(k).copied()));
    let bind_type = if field_sig.is_some() {
        "field"
    } else if sink.is_some() {
        "send"
    } else {
        ""
    };

    // 初值：读侧的字面量，否则类型缺省——输入框不订阅自己要写的槽（自读自写
    // 的回路会把刚发出的内容还原）。
    let slot = field_sig.unwrap_or_else(|| {
        RwSignal::new(
            accrete
                .get_source()
                .and_then(|b| b.default.clone())
                .unwrap_or_else(|| default_option_jskind(&kind)),
        )
    });

    let oninput = {
        let k1 = kind.clone();
        move |event: web_sys::Event| {
            let event_value: String = event
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                .map(|e| e.value())
                .unwrap_or_default();
            let parsed = match k1.as_ref() {
                Some(JsType::bool) => Value::Bool(event_value == "true"),
                Some(JsType::number) => Value::from(event_value.parse::<f64>().unwrap_or(0.0)),
                _ => Value::String(event_value),
            };
            slot.set(parsed);
        }
    };

    let onkeydown = {
        let ctx = ctx.clone();
        let k2 = kind.clone();
        let sink_emit = sink.clone();
        let nid = node_id.clone();
        move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Enter" {
                match bind_type {
                    "field" => {
                        if let Some(sig) = field_sig {
                            sig.set(slot.get_untracked());
                        }
                    }
                    "send" => {
                        let Some(sink) = sink_emit.clone() else {
                            return;
                        };
                        let ctx = ctx.clone();
                        let kk = k2.clone();
                        let val = slot.get_untracked();
                        // 空值回车不触发事件
                        if val.as_str().is_some_and(|s| s.trim().is_empty()) {
                            return;
                        }
                        slot.set(default_option_jskind(&kk));
                        // 命令式清空 DOM（浏览器 value-attribute 脏值语义）：
                        // 用户键入会脏化 defaultValue，此后 attribute 更新
                        // 不再重置显示值；且本节点全程存活，信号写入无法
                        // 经任何属性路径触达（dioxus 虚拟 DOM 整体重建无此问题）。
                        if let Some(el) = ev
                            .target()
                            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                        {
                            el.set_value("");
                        }
                        // 一个出口（ADR 0031）：scope 决定写本页、上行，还是两者；
                        // `id` = 本输入节点自身的 id（ADR 0010）。
                        ctx.emit(&sink, nid.clone(), val);
                    }
                    _ => {}
                }
            }
        }
    };

    // 反应性下沉到属性闭包：value 包在闭包里走 tachys 的 DynProperty，
    // slot 一变即直写 DOM 的 value property。若把整棵元素树包在顶层
    // `move || -> AnyView` 闭包里，其 rebuild 对普通字符串 attribute 是
    // 脏值保护下的 no-op——症状即"信号已清空但输入框内容还在"
    // （dioxus 虚拟 DOM 全量 diff 无此问题，移植时须注意）。
    let value = move || {
        let v = slot.get();
        match &v {
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => v.as_str().unwrap_or("").to_string(),
        }
    };
    let checked = move || slot.get().as_bool().unwrap_or(false);

    let ty = match &kind {
        Some(JsType::number) => "number",
        Some(JsType::bool) => "checkbox",
        Some(x) => x.input_type(),
        None => "text",
    };
    let base = input().class(css.as_str()).r#type(ty);
    match &kind {
        Some(JsType::bool) => base
            .checked(checked)
            .on(ev::input, oninput)
            .on(ev::keydown, onkeydown)
            .into_any(),
        _ => base
            .value(value)
            .on(ev::input, oninput)
            .on(ev::keydown, onkeydown)
            .into_any(),
    }
}
