use crate::Ctx;
use crate::hooks::use_common_css;
use brick::{Bind, BindVariant, BrickOps, Input, JsType};
use leptos::ev;
use leptos::prelude::*;
use leptos::html::*;
use serde_json::{Value, to_value};
use wasm_bindgen::JsCast;

fn default_option_jskind(v: &Option<JsType>) -> Value {
    v.as_ref()
        .map(|x| x.default_value())
        .unwrap_or_else(|| to_value("").unwrap())
}

/// 输入框：`Field` 绑定写 form 字段信号；`Event` 绑定在 Enter 时发送事件。
pub fn input_(brick: Input, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["input", "f", "shadow"];
    use_common_css(&mut css, &brick);
    let css = css.join(" ");

    let (bind_type, key, kind) = brick
        .get_bind()
        .and_then(|x| x.get("value"))
        .cloned()
        .map(|x| match x {
            Bind {
                variant: BindVariant::Field { field, .. },
                r#type: kind,
                ..
            } => ("field", field, kind),
            Bind {
                variant: BindVariant::Event { event },
                r#type: kind,
                ..
            } => ("event", event, kind),
            _ => ("", "".to_string(), Default::default()),
        })
        .unwrap_or(("", "".to_string(), Default::default()));

    let field_sig = if bind_type == "field" {
        ctx.form
            .as_ref()
            .and_then(|fs| fs.fields.get(&key).copied())
    } else {
        None
    };

    let slot = field_sig.unwrap_or_else(|| RwSignal::new(default_option_jskind(&kind)));

    let oninput = {
        let k1 = kind.clone();
        move |event: web_sys::Event| {
            let event_value: String = event
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                .map(|e| e.value())
                .unwrap_or_default();
            let parsed = match k1.as_ref() {
                Some(JsType::bool) => to_value(event_value == "true"),
                Some(JsType::number) => to_value(event_value.parse::<f64>().unwrap_or(0.0)),
                _ => to_value(event_value),
            }
            .unwrap();
            slot.set(parsed);
        }
    };

    let onkeydown = {
        let ctx = ctx.clone();
        let k2 = kind.clone();
        let k3 = key.clone();
        move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Enter" {
                match bind_type {
                    "field" => {
                        if let Some(sig) = field_sig {
                            sig.set(slot.get_untracked());
                        }
                    }
                    "event" => {
                        let ctx = ctx.clone();
                        let key = k3.clone();
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
                        leptos::task::spawn_local(async move {
                            ctx.send(key, None, val).await;
                        });
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
