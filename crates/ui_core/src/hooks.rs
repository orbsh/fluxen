use crate::Ctx;
use accrete::{Accrete, AccreteOps, Bind, classify::Classify};
use leptos::prelude::*;
use serde_json::Value;

/// 追加公共 CSS：非横向的 Box/Case/Rack/Text/Tab/Select 加 `col`，并扩展 class 列表。
pub fn use_common_css<'a, 'b: 'a, T>(css: &'a mut Vec<&'b str>, accrete: &'b T)
where
    T: Classify + AccreteOps,
{
    let t = accrete.get_type();
    let mut v = ["Box", "Frame", "Pages", "Rack", "Text", "Tab", "Select"].contains(&t);
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

/// The primary read channel (ADR 0031 §2: `source` is where a value comes from).
pub fn source(accrete: &impl AccreteOps) -> Option<&Bind> {
    accrete.get_source()
}

/// `source.default` — the literal, or the value to use while the slot is empty.
pub fn use_default(accrete: &impl AccreteOps) -> Option<Value> {
    source(accrete).and_then(|b| b.default.clone())
}

/// The slot name of the primary read channel; None = a literal.
pub fn use_source_id(accrete: &impl AccreteOps) -> Option<&String> {
    source(accrete).and_then(|b| b.slot.as_ref())
}

/// A source slot rendered as rows (`ctx.list`). No slot = no rows, not "one row":
/// a list channel without an address has nothing to page through.
pub fn use_source_list(
    ctx: &Ctx,
    accrete: &impl AccreteOps,
) -> Option<std::sync::Arc<Vec<std::sync::Arc<Accrete>>>> {
    bind_list(ctx, source(accrete))
}

/// Same, for a named second channel (`select.options`, `pages.select`).
pub fn bind_list(
    ctx: &Ctx,
    bind: Option<&Bind>,
) -> Option<std::sync::Arc<Vec<std::sync::Arc<Accrete>>>> {
    bind.and_then(|b| b.slot.as_ref())
        .map(|slot| ctx.slot_for_list(slot).get())
}

/// One field out of a slot's value (ADR 0008): the JSON Pointer the bind names,
/// or the value itself when there is none.
fn extract(v: Value, path: Option<&String>) -> Option<Value> {
    let Some(p) = path else { return Some(v) };
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

/// Resolve one channel to a value (ADR 0031 §1: one resolution path).
///
/// No slot: the channel is a literal. With a slot: `local` reads the page's
/// value plane, `remote` reads the slot's node, `both` prefers the local
/// assignment (a draft) and falls back to the far side. An empty slot falls
/// back to `default` — the value to show before anything was sent.
///
/// `tracked == false` is for the initial display of a component that also
/// emits: subscribing to a slot it writes would rebuild it from its own send.
pub fn value_of(ctx: &Ctx, bind: Option<&Bind>, tracked: bool) -> Option<Value> {
    let b = bind?;
    let Some(slot) = &b.slot else {
        return b.default.clone();
    };
    let read_data = |src: &str| -> Option<std::sync::Arc<Accrete>> {
        if tracked {
            ctx.slot_for_data(src).get()
        } else {
            ctx.slot_for_data(src).get_untracked()
        }
    };
    let read_val = |s: &str| -> Option<Value> {
        if tracked {
            ctx.slot_for_value(s).get()
        } else {
            ctx.slot_for_value(s).get_untracked()
        }
    };
    if b.scope.local()
        && let Some(v) = read_val(slot)
    {
        return extract(v, b.path.as_ref());
    }
    if b.scope.remote()
        && let Some(node) = read_data(slot)
    {
        return match b.path.as_ref() {
            Some(p) => match node.get_at(p) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("source {slot:?} path {p:?}: {e}");
                    None
                }
            },
            // Same-slot pass-through: the row's own value, else our default.
            None => node
                .get_source()
                .and_then(|s| s.default.clone())
                .or_else(|| b.default.clone()),
        };
    }
    b.default.clone()
}

pub fn use_source(ctx: &Ctx, accrete: &impl AccreteOps) -> Option<Value> {
    value_of(ctx, accrete.get_source(), true)
}

/// Non-reactive variant: the initial value of a component that emits.
pub fn use_source_untracked(ctx: &Ctx, accrete: &impl AccreteOps) -> Option<Value> {
    value_of(ctx, accrete.get_source(), false)
}

/// The write channel (ADR 0031 §2: `sink` is where a value goes). None = the
/// node does not write anywhere, so the caller keeps its local-only behaviour.
pub fn use_target<'a>(
    ctx: Ctx,
    accrete: &impl AccreteOps,
) -> Option<impl Fn(Value)> {
    let sink = accrete.get_sink().cloned()?;
    let id = node_id(accrete);
    Some(move |val| {
        let ctx = ctx.clone();
        ctx.emit(&sink, id.clone(), val);
    })
}

/// 发射节点自身的 id（ADR 0010：包装对象的 `id` = 发射节点 id）。
/// 空串按无身份处理——与行身份同一约定（`''` 不构成 id）。
pub(crate) fn node_id(accrete: &impl AccreteOps) -> Option<String> {
    accrete.get_id().clone().filter(|s| !s.is_empty())
}

/// 表单信号共享：`form_` 构建本结构后注入 `Ctx.form` 并克隆下传，
/// `input_`/`button_` 从自己拿到的 ctx 读取——归属沿克隆链传播，
/// 不依赖渲染时序。
#[derive(Clone)]
pub struct FormState {
    pub fields: std::collections::HashMap<String, RwSignal<Value>>,
    pub confirm: RwSignal<Value>,
}

#[cfg(test)]
mod tests {
    use super::node_id;
    use accrete::Accrete;
    use serde_json::{Value, json};

    fn accrete(v: Value) -> Accrete {
        serde_json::from_value(v).expect("valid accrete")
    }

    #[test]
    fn node_id_is_the_emitting_nodes_own_id() {
        let n = accrete(json!({"type": "input", "id": "in1"}));
        assert_eq!(node_id(&n).as_deref(), Some("in1"));
    }

    #[test]
    fn node_id_is_absent_without_an_id() {
        let n = accrete(json!({"type": "input"}));
        assert_eq!(node_id(&n), None);
    }

    #[test]
    fn node_id_treats_an_empty_id_as_absent() {
        let n = accrete(json!({"type": "input", "id": ""}));
        assert_eq!(node_id(&n), None);
    }
}