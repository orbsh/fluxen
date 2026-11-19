use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{use_common_css, use_source_list, use_source_untracked, use_target};
use accrete::{AccreteOps, Select, classify::Classify};
use leptos::html::*;
use leptos::prelude::*;
use serde_json::{Value, to_value};

/// 下拉选择（ADR 0031 §2）：选项走 `options`（list 平面），当前值走 `source`，
/// 选择走 `sink`。
///
/// 表单字段是 `sink` 的一种：`scope: local` 且外层表单认领了这个槽名（槽名即
/// 字段名），就写表单信号而不是发射——与 `input_` 同一条判据。
pub fn select_(accrete: Select, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["select", "f"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let field = accrete
        .get_sink()
        .filter(|s| s.scope.local())
        .and_then(|s| s.slot.as_ref())
        .and_then(|k| ctx.form.as_ref().and_then(|fs| fs.fields.get(k).copied()));

    // 发射组件初值 untracked 读（同 textarea_：避免父渲染闭包订阅到自己
    // emit 写的槽，emit 即重建子树还原选择）
    let initial = match field {
        Some(sig) => sig.get_untracked(),
        None => use_source_untracked(&ctx, &accrete)
            .unwrap_or_else(|| Value::String(String::new())),
    };
    let current = RwSignal::new(
        initial
            .as_str()
            .map(String::from)
            .unwrap_or_default(),
    );

    move || -> AnyView {
        let option = use_source_list(&ctx, &accrete).unwrap_or_default();
        let children = option.iter().enumerate().map(|(idx, child)| {
            let key = child.get_id().clone().unwrap_or(idx.to_string());
            let selected = current.get() == key;
            // 行是共享的，而 `add_class` 要改它：拷这一行（不是整张表）。
            let mut option = child.as_ref().clone();
            option.add_class("f as-stretch");
            if selected {
                option.add_class("selected");
                div().child(render_accrete(&ctx, &option)).into_any()
            } else {
                let v = to_value(&key).unwrap_or_else(|_| Value::String(String::new()));
                let c = current;
                let click_ctx = ctx.clone();
                let b = accrete.clone();
                div()
                    .on(leptos::ev::click, move |_| {
                        match field {
                            Some(sig) => sig.set(v.clone()),
                            None => {
                                if let Some(emit) = use_target(click_ctx.clone(), &b) {
                                    emit(v.clone());
                                }
                            }
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