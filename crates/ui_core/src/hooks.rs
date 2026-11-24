use crate::Ctx;
use accrete::{Accrete, AccreteOps, Bind, classify::Classify};
use leptos::prelude::*;
use serde_json::Value;

/// Append common CSS: a non-horizontal Box/Case/Text/Tab/Select gets `col`, and the class list is extended.
///
/// `Tiles` is left out: it names its own direction from `attrs.dir` (ADR 0035), and a second opinion
/// here would fight it — `dir: row` has to be able to say row, whatever this function would add.
pub fn use_common_css<'a, 'b: 'a, T>(css: &'a mut Vec<&'b str>, accrete: &'b T)
where
    T: Classify + AccreteOps,
{
    let t = accrete.get_type();
    let mut v = t != "Tiles" && ["Box", "Frame", "Entry", "Text", "Tab", "Select"].contains(&t);
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

/// The emitting node's own id (ADR 0010: the wrapper object's `id` = the emitting node's id).
/// An empty string counts as no identity — the same convention as row identity (`''` does not make an id).
pub(crate) fn node_id(accrete: &impl AccreteOps) -> Option<String> {
    accrete.get_id().clone().filter(|s| !s.is_empty())
}

/// One packed field: the `tag` the bundle keys it by, and the signal holding it.
pub struct Field {
    pub name: String,
    pub sig: RwSignal<Value>,
}

/// What a bundle hands down to its fields (ADR 0032): the keys it packs, when
/// it sends (`modes`), and where the packed object goes (`sink`). Built by
/// `bundle_` and injected into `Ctx.bundle`; fields and nested bundles read it
/// from the ctx clone they were rendered with, so ownership follows the clone
/// chain, not render order.
pub struct BundleState {
    /// Keys in walk order, so a pack is deterministic.
    pub fields: Vec<Field>,
    pub modes: accrete::SubmitSet,
    pub sink: Option<Bind>,
    /// This submission's instance id — not a field name.
    pub id: Option<String>,
    /// Enclosing bundle, for `enter`/`button` bubbling.
    pub parent: Option<std::sync::Arc<BundleState>>,
    /// The parent's signal this bundle's own packed object lands in (keyed by
    /// its `tag`, or its `id`). None for a top-level bundle.
    pub mirror: Option<RwSignal<Value>>,
}

impl BundleState {
    pub fn field(&self, tag: &str) -> Option<RwSignal<Value>> {
        self.fields.iter().find(|f| f.name == tag).map(|f| f.sig)
    }

    /// The packed object: `{tag: value, …}` in walk order (untracked — a send
    /// reads a snapshot and must not make the reader a subscriber).
    pub fn pack(&self) -> Value {
        let mut m = serde_json::Map::new();
        for f in &self.fields {
            m.insert(f.name.clone(), f.sig.get_untracked());
        }
        Value::Object(m)
    }
}

/// The key a bundle contributes to its parent's object: its `tag`, or its `id`
/// when there is no tag. Neither = an unnamed group, which packs nothing into
/// the parent. Empty strings are absent, same rule as `node_id`.
pub fn bundle_key(tag: &Option<String>, id: &Option<String>) -> Option<String> {
    tag.clone()
        .filter(|s| !s.is_empty())
        .or_else(|| id.clone().filter(|s| !s.is_empty()))
}

/// Write this bundle's packed object into its parent's slot, then do the same
/// up the chain: an outer pack reads the inner one, so the inner lands first.
pub fn refresh(b: &std::sync::Arc<BundleState>) {
    if let Some(m) = &b.mirror {
        m.set(b.pack());
    }
    if let Some(p) = &b.parent {
        refresh(p);
    }
}

/// Send the bundle's packed object to its own sink — the only exit for the
/// fields it claimed (ADR 0032). Refresh first so enclosing objects are current
/// even when the outer bundle never sends.
pub fn submit(ctx: &Ctx, b: &std::sync::Arc<BundleState>) {
    refresh(b);
    if let Some(sink) = &b.sink {
        ctx.emit(sink, b.id.clone(), b.pack());
    }
}

/// The nearest enclosing bundle that declares `mode`. Nearest wins so one
/// interaction is one send: a nested bundle cannot make its parent send again.
pub fn nearest(
    b: Option<&std::sync::Arc<BundleState>>,
    mode: accrete::SubmitMode,
) -> Option<std::sync::Arc<BundleState>> {
    let mut cur = b.cloned();
    while let Some(b) = cur {
        if b.modes.has(mode) {
            return Some(b);
        }
        cur = b.parent.clone();
    }
    None
}

/// A field's value changed: keep enclosing objects current, then send from the
/// bundle that owns `change` timing (if any declares it).
pub fn field_changed(ctx: &Ctx, b: Option<&std::sync::Arc<BundleState>>) {
    let Some(b) = b else { return };
    refresh(b);
    if let Some(owner) = nearest(Some(b), accrete::SubmitMode::Change) {
        submit(ctx, &owner);
    }
}

/// Enter was pressed in a field: the nearest bundle declaring `enter` sends.
pub fn enter_pressed(ctx: &Ctx, b: Option<&std::sync::Arc<BundleState>>) {
    if let Some(owner) = nearest(b, accrete::SubmitMode::Enter) {
        submit(ctx, &owner);
    }
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