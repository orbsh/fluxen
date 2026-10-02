use crate::Ctx;
use accrete::{Accrete, AccreteOps, Bind, BindVariant, classify::Classify};
use leptos::prelude::*;
use serde_json::Value;

/// 追加公共 CSS：非横向的 Box/Case/Rack/Text/Tab/Select 加 `col`，并扩展 class 列表。
pub fn use_common_css<'a, 'b: 'a, T>(css: &'a mut Vec<&'b str>, accrete: &'b T)
where
    T: Classify + AccreteOps,
{
    let t = accrete.get_type();
    let mut v = ["Box", "Case", "Pages", "Rack", "Text", "Tab", "Select"].contains(&t);
    if let Some(a) = accrete.borrow_attrs() {
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
pub fn use_default(accrete: &impl AccreteOps) -> Option<Value> {
    accrete
        .get_bind()
        .and_then(|x| x.get("value"))
        .and_then(|x| x.default.clone())
}

/// 取 `bind["value"]` 的 Source 来源名。
pub fn use_source_id(accrete: &impl AccreteOps) -> Option<&String> {
    if let Bind {
        variant: BindVariant::Source { source, .. },
        ..
    } = accrete.get_bind().and_then(|x| x.get("value"))?
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
    accrete: &impl AccreteOps,
    key: &str,
) -> Option<std::sync::Arc<Vec<Accrete>>> {
    source_of(accrete, key).map(|src| ctx.slot_for_list(src).get())
}

/// `bind[key]` 为 Source 时取其 source 名（path 只经 source_of_path 消费，
/// 避免 4 元组到处传递）。
pub fn source_of<'a>(accrete: &'a impl AccreteOps, key: &str) -> Option<&'a String> {
    if let Bind {
        variant: BindVariant::Source { source, .. },
        ..
    } = accrete.get_bind().and_then(|x| x.get(key))?
    {
        Some(source)
    } else {
        None
    }
}

/// Source 绑定的可选提取路径（ADR 0008）。
pub fn source_of_path<'a>(accrete: &'a impl AccreteOps, key: &str) -> Option<&'a String> {
    if let Bind {
        variant: BindVariant::Source { path: Some(p), .. },
        ..
    } = accrete.get_bind().and_then(|x| x.get(key))?
    {
        Some(p)
    } else {
        None
    }
}

/// 取 `bind[key]` 对应的值：`Source` 从 `ctx.data[source]` 订阅
/// （带 `path` 进槽内节点 WIRE SHAPE 提取），`Local` 从 `ctx.vals[slot]`
/// 订阅裸值平面（带 `path` 进槽内 Value 的 JSON Pointer 提取——形状归
/// 生产端），否则取 accrete 自身 `bind[key].default`。
/// `tracked == false` 用于发射组件的初值显示：自己 emit 写回的槽若被
/// tracked 读，会把发射方自己重建、刚发送的内容"还原"（e2e 抓到的回路）。
fn resolve_bind(ctx: &Ctx, accrete: &impl AccreteOps, key: &str, tracked: bool) -> Option<Value> {
    let read_data = |src: &str| -> Option<std::sync::Arc<Accrete>> {
        if tracked { ctx.slot_for_data(src).get() } else { ctx.slot_for_data(src).get_untracked() }
    };
    let read_val = |slot: &str| -> Option<Value> {
        if tracked { ctx.slot_for_value(slot).get() } else { ctx.slot_for_value(slot).get_untracked() }
    };
    let bind = accrete.get_bind().and_then(|b| b.get(key));
    match bind.map(|b| &b.variant) {
        Some(BindVariant::Source { source, path }) => {
            let node = read_data(source);
            match (node.as_deref(), path) {
                (Some(n), Some(p)) => match n.get_at(p) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        tracing::warn!("source {source:?} path {p:?}: {e}");
                        None
                    }
                },
                // 同 key 直传惯例：取槽节点自己 bind[key].default；
                // 槽空回退自身 default（原实现语义，渲染未喂先有值）。
                (Some(n), None) => n
                    .get_bind()
                    .and_then(|b| b.get(key))
                    .and_then(|b| b.default.clone())
                    .or_else(|| accrete.get_bind().and_then(|b| b.get(key))?.default.clone()),
                (None, _) => bind.and_then(|b| b.default.clone()),
            }
        }
        Some(BindVariant::Local { slot, path, .. }) => {
            let v = read_val(slot);
            match (v, path) {
                (Some(v), Some(p)) => {
                    let mut cur = v;
                    for seg in p.split('/').skip(1) {
                        let seg = seg.replace("~1", "/").replace("~0", "~");
                        cur = match cur {
                            Value::Object(m) => m.get(seg.as_str())?.clone(),
                            Value::Array(a) => a.get(seg.parse::<usize>().ok()?)?.clone(),
                            _ => return None,
                        };
                    }
                    Some(cur)
                }
                (Some(v), None) => Some(v),
                // 槽未写入：回退自身 default（emit 前的初始显示，与
                // Source 槽空回退同语义）
                (None, _) => bind.and_then(|b| b.default.clone()),
            }
        }
        _ => bind?.default.clone(),
    }
}

pub fn use_source(ctx: &Ctx, accrete: &impl AccreteOps, key: &str) -> Option<Value> {
    resolve_bind(ctx, accrete, key, true)
}

/// 非反应式变体：发射组件显示初值专用。
pub fn use_source_untracked(ctx: &Ctx, accrete: &impl AccreteOps, key: &str) -> Option<Value> {
    resolve_bind(ctx, accrete, key, false)
}

/// `use_source(ctx, accrete, "value")`。
pub fn use_source_value(ctx: &Ctx, accrete: &impl AccreteOps) -> Option<Value> {
    use_source(ctx, accrete, "value")
}

/// `bind[key]` 为 Event/Local 时，返回一个 emit 闭包（ADR 0008：
/// 落点由 Ctx::emit 统一路由——上行或本地槽）。
pub fn use_target<'a>(
    ctx: Ctx,
    accrete: &'a impl AccreteOps,
    key: &'a str,
) -> Option<impl Fn(Value)> {
    let variant = accrete
        .get_bind()
        .and_then(|x| x.get(key))
        .map(|b| b.variant.clone())?;
    match variant {
        BindVariant::Event { .. } | BindVariant::Local { .. } => Some(move |val| {
            let ctx = ctx.clone();
            ctx.emit(&variant, None, val);
        }),
        _ => None,
    }
}

/// `use_target(ctx, accrete, "value")`。
pub fn use_target_value(ctx: Ctx, accrete: &impl AccreteOps) -> Option<impl Fn(Value)> {
    use_target(ctx, accrete, "value")
}
/// 表单信号共享：`form_` 构建本结构后注入 `Ctx.form` 并克隆下传，
/// `input_`/`button_` 从自己拿到的 ctx 读取——归属沿克隆链传播，
/// 不依赖渲染时序。
#[derive(Clone)]
pub struct FormState {
    pub fields: std::collections::HashMap<String, RwSignal<Value>>,
    pub confirm: RwSignal<Value>,
}
