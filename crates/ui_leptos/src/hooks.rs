use crate::Ctx;
use brick::{Bind, BindVariant, Brick, BrickOps, classify::Classify};
use leptos::prelude::*;
use serde_json::Value;

/// 追加公共 CSS：非横向的 Box/Case/Rack/Text/Tab/Select 加 `col`，并扩展 class 列表。
pub fn use_common_css<'a, 'b: 'a, T>(css: &'a mut Vec<&'b str>, brick: &'b T)
where
    T: Classify + BrickOps,
{
    let t = brick.get_type();
    let mut v = ["Box", "Case", "Rack", "Text", "Tab", "Select"].contains(&t);
    if let Some(a) = brick.borrow_attrs() {
        if a.is_horizontal() {
            v = false;
        }
        if let Some(cc) = a.get_class() {
            let c = cc.iter().map(|x| &**x).collect::<Vec<_>>();
            css.extend(c);
        }
    }
    if v {
        css.push("col");
    }
}

/// 取 `bind["value"].default`。
pub fn use_default(brick: &impl BrickOps) -> Option<Value> {
    brick
        .get_bind()
        .and_then(|x| x.get("value"))
        .and_then(|x| x.default.clone())
}

/// 取 `bind["value"]` 的 Source 来源名。
pub fn use_source_id(brick: &impl BrickOps) -> Option<&String> {
    if let Bind {
        variant: BindVariant::Source { source },
        ..
    } = brick.get_bind().and_then(|x| x.get("value"))?
    {
        Some(source)
    } else {
        None
    }
}

/// 订阅 `ctx.list[source]` 的 per-key 槽（无 Source 绑定则 None）。
/// 其他键的更新不会触发本槽订阅者。
pub fn use_source_list(
    ctx: &Ctx,
    brick: &impl BrickOps,
    key: &str,
) -> Option<std::sync::Arc<Vec<Brick>>> {
    source_of(brick, key).map(|src| ctx.slot_for_list(src).get())
}

/// `bind[key]` 为 Source 时取其 source 名。
pub fn source_of<'a>(brick: &'a impl BrickOps, key: &str) -> Option<&'a String> {
    if let Bind {
        variant: BindVariant::Source { source },
        ..
    } = brick.get_bind().and_then(|x| x.get(key))?
    {
        Some(source)
    } else {
        None
    }
}

/// 取 `bind[key]` 对应的值：Source 则从 `ctx.data[source]` 槽订阅取值，
/// 否则取 brick 自身。
pub fn use_source(ctx: &Ctx, brick: &impl BrickOps, key: &str) -> Option<Value> {
    let from_source: Option<std::sync::Arc<Brick>> =
        source_of(brick, key).and_then(|src| ctx.slot_for_data(src).get());
    let comp: &dyn BrickOps = match &from_source {
        Some(d) => &**d,
        None => brick,
    };
    comp.get_bind().and_then(|b| b.get(key))?.default.clone()
}

/// `use_source(ctx, brick, "value")`。
pub fn use_source_value(ctx: &Ctx, brick: &impl BrickOps) -> Option<Value> {
    use_source(ctx, brick, "value")
}

/// `bind[key]` 为 Event 时，返回一个发送该事件的闭包。
pub fn use_target<'a>(ctx: Ctx, brick: &'a impl BrickOps, key: &'a str) -> Option<impl Fn(Value)> {
    if let Some(Bind {
        variant: BindVariant::Event { event },
        default: _,
        r#type: _,
    }) = brick.get_bind().and_then(|x| x.get(key))
    {
        let ev = event.clone();
        Some(move |val| {
            let ctx = ctx.clone();
            let ev = ev.clone();
            leptos::task::spawn_local(async move {
                ctx.send(ev, None, val).await;
            });
        })
    } else {
        None
    }
}

/// `use_target(ctx, brick, "value")`。
pub fn use_target_value(ctx: Ctx, brick: &impl BrickOps) -> Option<impl Fn(Value)> {
    use_target(ctx, brick, "value")
}
/// 表单信号共享：`form_` 构建本结构后注入 `Ctx.form` 并克隆下传，
/// `input_`/`button_` 从自己拿到的 ctx 读取——归属沿克隆链传播，
/// 不依赖渲染时序。
#[derive(Clone)]
pub struct FormState {
    pub fields: std::collections::HashMap<String, RwSignal<Value>>,
    pub confirm: RwSignal<Value>,
}
