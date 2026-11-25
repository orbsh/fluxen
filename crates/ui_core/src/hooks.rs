use crate::Ctx;
use accrete::{Accrete, AccreteOps, Sink, Source, classify::Classify};
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
pub fn source(accrete: &impl AccreteOps) -> Option<&Source> {
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
    bind: Option<&Source>,
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
pub fn value_of(ctx: &Ctx, bind: Option<&Source>, tracked: bool) -> Option<Value> {
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
    // The local plane decides first — MEMBER by member: a page that chose one member has not chosen the
    // others, so a path that misses here is not "no value", it is "the far side still decides this one"
    // (ADR 0038 §2 revised: options are ordinary values, and their precedence is the ordinary one).
    if b.scope.local()
        && let Some(v) = read_val(slot)
        && let Some(v) = extract(v, b.path.as_ref())
    {
        return Some(v);
    }
    if b.scope.remote()
        && let Some(node) = read_data(slot)
    {
        return match b.path.as_ref() {
            Some(p) => match node.get_at(p) {
                Ok(v) => Some(v),
                // The node's wire shape is one place a path can land; its PAYLOAD (`source.default`) is the
                // other, and it is where a slot's value actually lives (ADR 0012). Options are read this way:
                // the far side sends one node whose payload is the option object, and a page reads `/sketch`.
                Err(_) => node
                    .get_source()
                    .and_then(|s| s.default.clone())
                    .and_then(|d| extract(d, Some(&p.to_string()))),
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
pub fn use_target<'a>(ctx: Ctx, accrete: &impl AccreteOps) -> Option<impl Fn(Value)> {
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

/// One packed field: the `tag` the loom keys it by, the signal holding it, and the value it
/// started with (`reset: default` restores exactly this).
pub struct Field {
    pub name: String,
    pub sig: RwSignal<Value>,
    pub initial: Value,
}

/// What a loom hands down to its fields (ADR 0032): the keys it packs, when
/// it sends (`modes`), and where the packed object goes (`sink`). Built by
/// `loom_` and injected into `Ctx.loom`; fields and nested looms read it
/// from the ctx clone they were rendered with, so ownership follows the clone
/// chain, not render order.
pub struct LoomState {
    /// Keys in walk order, so a pack is deterministic.
    pub fields: Vec<Field>,
    pub modes: accrete::SubmitSet,
    /// What the fields become after a send (ADR 0032 §4).
    pub reset: accrete::ResetMode,
    /// How many sends were granted: `None` or `-1` = unlimited, `n` = n more allowed.
    pub sends: Option<i64>,
    /// Sends left. Starts at `sends`; each send takes one; zero locks the loom.
    pub remaining: RwSignal<i64>,
    /// Nested looms, so a reset travels down the same tree a pack travels up.
    pub children: StoredValue<Vec<std::sync::Arc<LoomState>>>,
    pub sink: Option<Sink>,
    /// This submission's instance id — not a field name.
    pub id: Option<String>,
    /// Enclosing loom, for `enter`/`button` bubbling.
    pub parent: Option<std::sync::Arc<LoomState>>,
    /// The parent's signal this loom's own packed object lands in (keyed by
    /// its `tag`, or its `id`). None for a top-level loom.
    pub mirror: Option<RwSignal<Value>>,
}

impl LoomState {
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

    /// Sends left; `-1` (or no `sends`) is unlimited.
    pub fn unlimited(&self) -> bool {
        self.sends.map_or(true, |n| n < 0)
    }

    /// One send happened: take one off, if there is anything to take.
    fn count_send(&self) {
        if self.unlimited() {
            return;
        }
        let left = self.remaining.get_untracked();
        if left > 0 {
            self.remaining.set(left - 1);
        }
    }

    /// After a send: this loom's `reset` governs **every field it packed**, nested looms included — a
    /// submission is one object, so the aftermath is one decision. A nested loom that sends on its own
    /// applies its own mode, over its own subtree only (it never reaches up).
    fn reset_fields(&self) {
        if !matches!(self.reset, accrete::ResetMode::Keep) {
            self.reset_subtree(self.reset);
            refresh_inner(self);
        }
    }

    /// Post-order, so a child's cleared pack lands in the parent's slot for it before the parent refreshes.
    fn reset_subtree(&self, mode: accrete::ResetMode) {
        for f in &self.fields {
            let next = match mode {
                accrete::ResetMode::Clear => empty_like(&f.initial),
                accrete::ResetMode::Default => f.initial.clone(),
                accrete::ResetMode::Keep => f.sig.get_untracked(),
            };
            f.sig.set(next);
        }
        self.children.with_value(|kids| {
            for k in kids {
                k.reset_subtree(mode);
            }
        });
        refresh_inner(self);
    }
}

/// The empty value of the same shape: what `reset: clear` writes. A string becomes `""`, a number `0`,
/// a bool `false`, an object `{}` — the shape stays what the field is, only its content goes.
pub fn empty_like(like: &Value) -> Value {
    match like {
        Value::String(_) => Value::String(String::new()),
        Value::Number(_) => Value::from(0.0),
        Value::Bool(_) => Value::Bool(false),
        Value::Array(_) => Value::Array(Vec::new()),
        Value::Object(_) => Value::Object(serde_json::Map::new()),
        Value::Null => Value::Null,
    }
}

/// The loom that owns the send count for this ctx: the nearest one that declares `sends`.
pub fn lock_owner(b: Option<&std::sync::Arc<LoomState>>) -> Option<std::sync::Arc<LoomState>> {
    let mut cur = b.cloned();
    while let Some(b) = cur {
        if b.sends.is_some() {
            return Some(b);
        }
        cur = b.parent.clone();
    }
    None
}

/// Used up? A loom that declared `sends` and has none left is locked: its fields are disabled and
/// `submit` refuses (the count is the page's own guard, not a permission — the far side still decides).
pub fn locked(b: Option<&std::sync::Arc<LoomState>>) -> bool {
    lock_owner(b).is_some_and(|b| !b.unlimited() && b.remaining.get() <= 0)
}

/// The key a loom contributes to its parent's object: its `tag`, and only its `tag`. Tag is the name a node
/// takes in the loom that packs it — the same rule a field follows (ADR 0032 §4). `id` is not a name: it is
/// this submission's identity, and letting it double as a key would make the parent's shape move when someone
/// gave a nested loom an id for matching replies. `dest` is not a name either: it is where the inner loom's
/// own sends go, which is a different fact (the autocomplete in §4 sends to `mentions` while the outer object
/// calls it `mention`). No tag = an unnamed group, which packs nothing into the parent. Empty strings are
/// absent, same rule as `node_id`.
pub fn loom_key(tag: &Option<String>) -> Option<String> {
    tag.clone().filter(|s| !s.is_empty())
}

/// Write this loom's packed object into its parent's slot, then do the same
/// up the chain: an outer pack reads the inner one, so the inner lands first.
pub fn refresh(b: &std::sync::Arc<LoomState>) {
    refresh_inner(b);
}

fn refresh_inner(b: &LoomState) {
    if let Some(m) = &b.mirror {
        m.set(b.pack());
    }
    if let Some(p) = &b.parent {
        refresh_inner(p);
    }
}

/// Send the loom's packed object to its own sink — the only exit for the
/// fields it claimed (ADR 0032). Refresh first so enclosing objects are current
/// even when the outer loom never sends.
pub fn submit(ctx: &Ctx, b: &std::sync::Arc<LoomState>) {
    if locked(Some(b)) {
        return;
    }
    refresh(b);
    if let Some(sink) = &b.sink {
        ctx.emit(sink, b.id.clone(), b.pack());
        // One send happened, so one comes off. Reset after the count, so a loom granted `sends: 1`
        // sends once and is locked while its fields are already back to their starting value.
        b.count_send();
    }
    b.reset_fields();
}

/// The nearest enclosing loom that declares `mode`. Nearest wins so one
/// interaction is one send: a nested loom cannot make its parent send again.
pub fn nearest(
    b: Option<&std::sync::Arc<LoomState>>,
    mode: accrete::SubmitMode,
) -> Option<std::sync::Arc<LoomState>> {
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
/// loom that owns `change` timing (if any declares it).
pub fn field_changed(ctx: &Ctx, b: Option<&std::sync::Arc<LoomState>>) {
    let Some(b) = b else { return };
    refresh(b);
    if let Some(owner) = nearest(Some(b), accrete::SubmitMode::Change) {
        submit(ctx, &owner);
    }
}

/// Enter was pressed in a field: the nearest loom declaring `enter` sends.
pub fn enter_pressed(ctx: &Ctx, b: Option<&std::sync::Arc<LoomState>>) {
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
    fn clear_keeps_the_shape_and_drops_the_content() {
        use super::empty_like;
        use serde_json::json;
        assert_eq!(empty_like(&json!("typed")), json!(""));
        assert_eq!(empty_like(&json!(42)), json!(0.0));
        assert_eq!(empty_like(&json!(true)), json!(false));
        assert_eq!(empty_like(&json!({"a": 1})), json!({}));
        assert_eq!(empty_like(&json!([1, 2])), json!([]));
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

    #[test]
    fn a_nested_loom_is_named_by_its_tag_and_nothing_else() {
        use super::loom_key;

        assert_eq!(loom_key(&Some("mention".into())), Some("mention".into()));
        assert_eq!(loom_key(&Some(String::new())), None);
        assert_eq!(loom_key(&None), None);
        // An id is identity, not a name: it must not decide the parent object's shape.
        let n = accrete(json!({"type": "loom", "id": "login-42"}));
        let Accrete::loom(b) = &n else {
            panic!("expected loom")
        };
        assert_eq!(loom_key(&b.tag), None);
    }
}
