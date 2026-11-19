use crate::Ctx;
use crate::hooks::use_default;
use accrete::{AccreteOps, Button, ButtonAttr};
use leptos::ev::click;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;

/// 按钮：文本取 `source`；有 `sink` 就是表单的确认键。提交不是第三条通道
/// （ADR 0031 §2）：表单持有落点，按钮只声明"我朝外走"。
pub fn button_(accrete: Button, ctx: &Ctx) -> AnyView {
    let t = use_default(&accrete)
        .unwrap_or(Value::String("Ok".into()))
        .as_str()
        .unwrap_or("Ok")
        .to_owned();

    let oneshot = accrete
        .attrs
        .as_ref()
        .map(|ButtonAttr { oneshot, .. }| *oneshot)
        .unwrap_or(false);

    if accrete.get_sink().is_none() {
        return div().into_any();
    }

    let Some(confirm) = ctx.form.as_ref().map(|fs| fs.confirm) else {
        return div().into_any();
    };

    move || -> AnyView {
        let v = confirm.get().as_bool().unwrap_or(false);
        let mut css = vec!["button", "shadow"];
        css.push(if !v { "accent" } else { "disabled" });
        let css = css.join(" ");
        button()
            .class(css.as_str())
            .child(t.clone())
            .on(click, move |_| {
                if oneshot {
                    if !v {
                        confirm.set(Value::Bool(true));
                    }
                } else {
                    confirm.set(Value::Bool(!v));
                    let c = confirm;
                    leptos::task::spawn_local(async move {
                        c.set(Value::Bool(v));
                    });
                }
            })
            .into_any()
    }
    .into_any()
}
