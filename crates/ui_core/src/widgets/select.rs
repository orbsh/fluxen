use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{use_common_css, use_source_list, use_source_untracked, use_target_value};
use accrete::{Accrete, AccreteOps, Bind, BindVariant, Select, classify::Classify};
use leptos::html::*;
use leptos::prelude::*;
use serde_json::{Value, to_value};

/// 表单里的一个选择：`Field` 绑定时写表单字段（同 `input_`），选项来自
/// `bind["value"].payload.options` —— 计划只声明"选哪个"，控件由这里给。
///
/// 父表单在 `ctx.form` 里注入了字段信号；没有它（例如单独出现的 select）就
/// 退回原来的路子：选项取 list 槽、选择上行到 dest/local。
fn select_as_field(
    accrete: &Select,
    ctx: &Ctx,
    field: &str,
    signal: RwSignal<Value>,
    payload: Option<&Value>,
) -> AnyView {
    let ctx = ctx.clone();
    let field = field.to_string();
    let mut css = vec!["select", "f"];
    use_common_css(&mut css, accrete);
    let css = css.join(" ");
    let payload = payload.cloned();

    // 选项：计划里声明的是名字列表；这里换成 select 认识的行节点（id 即答案值）
    let names: Vec<String> = payload
        .as_ref()
        .and_then(|p| p.get("options"))
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .filter_map(|o| o.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let default = payload
        .as_ref()
        .and_then(|p| p.get("default"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| names.first().cloned())
        .unwrap_or_default();
    // 初值只在 effect 里写一次：渲染期写信号会触发"render 中改状态"的告警，
    // 也让同一次渲染的结果取决于它自己。
    if !default.is_empty() {
        let d = default.clone();
        Effect::new(move |_| {
            if signal.get().as_str().unwrap_or("").is_empty() {
                signal.set(Value::String(d.clone()));
            }
        });
    }

    let rows: Vec<Accrete> = names
        .iter()
        .map(|name| {
            serde_json::from_value::<Accrete>(serde_json::json!({
                "type": "text",
                "id": name,
                "bind": { "value": { "kind": "default", "default": name } },
            }))
            .expect("an option is a text node")
        })
        .collect();

    move || -> AnyView {
        let chosen = signal.get().as_str().unwrap_or("").to_string();
        let children = rows.iter().enumerate().map(|(idx, child)| {
            let key = child.get_id().clone().unwrap_or(idx.to_string());
            let mut row = child.clone();
            row.add_class("f as-stretch");
            if chosen == key {
                row.add_class("selected");
                return div().child(render_accrete(&ctx, &row)).into_any();
            }
            let c = signal;
            let f = field.clone();
            let value = key.clone();
            div()
                .on(leptos::ev::click, move |_| {
                    c.set(Value::String(value.clone()));
                    let _ = &f;
                })
                .child(render_accrete(&ctx, &row))
                .into_any()
        });
        div()
            .class(css.as_str())
            .child(Vec::from_iter(children))
            .into_any()
    }
    .into_any()
}

/// 下拉选择：`options` 从 `ctx.list[source]` 取，`current` 取 `bind["value"].default`。
pub fn select_(accrete: Select, ctx: &Ctx) -> AnyView {
    // 表单字段：写 ctx.form 里的信号，而不是上行事件
    if let Some(Bind {
        variant: BindVariant::Field { field, payload, .. },
        ..
    }) = accrete.get_bind().and_then(|x| x.get("value")).cloned()
    {
        if let Some(signal) = ctx
            .form
            .as_ref()
            .and_then(|fs| fs.fields.get(&field).copied())
        {
            return select_as_field(&accrete, &ctx, &field, signal, payload.as_ref());
        }
    }
    let ctx = ctx.clone();
    let mut css = vec!["select", "f"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    // 发射组件初值 untracked 读（同 textarea_：避免父渲染闭包订阅到自己
    // emit 写的槽，emit 即重建子树还原选择）
    let current = RwSignal::new(
        use_source_untracked(&ctx, &accrete, "value")
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
                let v = to_value(&key).unwrap_or_else(|_| Value::String(String::new()));
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
