use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{use_common_css, use_source_list, use_source_value, use_target_value};
use accrete::{AccreteOps, Select, classify::Classify};
use leptos::html::*;
use leptos::prelude::*;
use serde_json::to_value;

/// 下拉选择：`options` 从 `ctx.list[source]` 取，`current` 取 `bind["value"].default`。
pub fn select_(accrete: Select, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["select", "f"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let current = RwSignal::new(
        use_source_value(&ctx, &accrete)
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default(),
    );

    move || -> AnyView {
        let option = use_source_list(&ctx, &accrete, "options").unwrap_or_default();
        let children = option.iter().enumerate().map(|(idx, child)| {
            let key = child.get_id().clone().unwrap_or(idx.to_string());
            let selected = current.get() == key;
            let mut option = child.clone();
            option.add_class("f as-stretch");
            if selected {
                option.add_class("selected");
                div().child(render_accrete(&ctx, &option)).into_any()
            } else {
                let v = to_value(&key).unwrap_or_else(|_| to_value("").unwrap());
                let c = current;
                let click_ctx = ctx.clone();
                let b = accrete.clone();
                div()
                    .on(leptos::ev::click, move |_| {
                        if let Some(emitter) = use_target_value(click_ctx.clone(), &b) {
                            emitter(v.clone());
                        }
                        if let Some(s) = v.as_str() {
                            c.set(s.to_string());
                        }
                    })
                    .child(render_accrete(&ctx, &option))
                    .into_any()
            }
        });
        div()
            .class(css.as_str())
            .child(Vec::from_iter(children))
            .into_any()
    }
    .into_any()
}
