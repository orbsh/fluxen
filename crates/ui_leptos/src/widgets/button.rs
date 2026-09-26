use crate::Ctx;
use crate::hooks::use_default;
use accrete::{AccreteOps, Bind, BindVariant, Button, ButtonAttr};
use leptos::ev::click;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::{Value, to_value};

/// 按钮：默认文本取 `bind["value"].default`；`Submit` 变体切换 form 的确认信号。
pub fn button_(accrete: Button, ctx: &Ctx) -> AnyView {
    let t = use_default(&accrete)
        .unwrap_or(to_value("Ok").unwrap())
        .as_str()
        .unwrap_or("Ok")
        .to_owned();

    let oneshot = accrete
        .attrs
        .as_ref()
        .map(|ButtonAttr { oneshot, .. }| *oneshot)
        .unwrap_or(false);

    let Some(Bind {
        variant: BindVariant::Submit { .. },
        ..
    }) = accrete.get_bind().and_then(|x| x.get("value").cloned())
    else {
        return div().into_any();
    };

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
