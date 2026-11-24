use crate::hooks::BundleState;
use crate::render::dispatch;
use accrete::{Accrete, AccreteOps};
use content::codec::ActiveCodec;
use content::{Content, PatchKind};
use leptos::prelude::*;
use serde_json::{Map, Value, to_value};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::{LazyLock, RwLock};
use transport::Transport;

/// Table of registered stamp definitions (ADR 0037: a definition is a node tree, a value is a whole-value replacement).
static TMPL: LazyLock<RwLock<accrete::template::Templates>> =
    LazyLock::new(|| RwLock::new(accrete::template::Templates::new()));

/// Globally shared state container: owns layout, data, lists, and transport send/receive.
///
/// `bundle` is the render-time environment: `bundle_` injects its own
/// `BundleState` and clones `Ctx` for the subtree, so ownership follows the
/// clone chain; sibling branches each keep their own clone and cannot pollute
/// each other. Nested bundles shadow the outer one.
/// per-key signal slot: the outer map holds only handles (the outer map is written only when the key set changes),
/// values are published through the inner signal — an update to one key does not notify subscribers of other keys.
/// List slot: one shared, immutable snapshot.
///
/// The outer `Arc` lets a render closure hold the whole list without copying it; the inner `Arc<Accrete>` makes "add one row" need only a
/// pointer instead of deep-copying every existing row — that once made every append an O(rows) deep copy, slower the more messages there were
/// (O(n²)). The row contents are shared, so changing a row needs `Arc::make_mut` (in place when exclusive, clone that row when shared).
pub type ListSlot = RwSignal<std::sync::Arc<Vec<std::sync::Arc<Accrete>>>>;
pub type DataSlot = RwSignal<Option<std::sync::Arc<Accrete>>>;
/// Bare Value slot (ADR 0008 local plane): event data is not a display shape,
/// it lives on a plane separate from data (Accrete).
pub type ValueSlot = RwSignal<Option<Value>>;

/// The two halves of a page option (ADR 0038 §2): `local` is the page's own choice, `remote` is the default given by the far side.
/// The effective value prefers local and falls back to remote — the same rule as reading `both` (ADR 0031 §1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigValue {
    pub local: Option<Value>,
    pub remote: Option<Value>,
}

impl ConfigValue {
    /// Effective value: the page's if it has chosen, otherwise the far side's; both halves empty = the component's own default.
    pub fn effective(&self) -> Option<&Value> {
        self.local.as_ref().or(self.remote.as_ref())
    }
}

/// A slot for one option: a per-key signal, writing one option does not notify subscribers of other options.
pub type ConfigSlot = RwSignal<ConfigValue>;

#[derive(Clone)]
pub struct Ctx {
    pub transport: LeptosTransport,
    pub codec: ActiveCodec,
    pub layout: RwSignal<Accrete>,
    pub data: RwSignal<HashMap<String, DataSlot>>,
    pub list: RwSignal<HashMap<String, ListSlot>>,
    /// local plane: named Value slots shared by where `kind: local` events land and by their subscription.
    pub vals: RwSignal<HashMap<String, ValueSlot>>,
    /// Page options (ADR 0038 §2): a namespace of their own, one signal per option, local/remote halves.
    /// It is not a member of the slot plane — slot names are the application's, and mixing them in would make "which names are configuration" unanswerable.
    pub config: RwSignal<HashMap<String, ConfigSlot>>,
    /// Drag session (ADR 0034): `Some` means "a drag is in flight", and what it holds is that source frame node
    /// itself — the cargo is its own subtree, so there is no second parallel structure (a copy and the original would sooner or later
    /// disagree). This is the only cross-component transient state in the renderer: source and drop target sit apart with other nodes between them, and a local signal
    /// cannot reach across. It enters no slot and never goes on the wire: "who is in flight, over what" is a fact of the browser, not
    /// of the producer's tree. There can be only one copy of it; each component keeping its own is the opposite of this design.
    pub drag: RwSignal<Option<Accrete>>,
    /// Permanent owner of the slots: the app root scope captured at Ctx::new.
    /// If a slot were built inside the scope of the frame-consuming Effect, an Effect rerun would dispose its storage,
    /// and what is left in the outer map is a dead handle (panic: already disposed).
    pub(crate) owner: Owner,
    pub bundle: Option<Arc<BundleState>>,
}

/// The UI side holds the transport plus the downstream frame signal. wasm is single-threaded, shared through Rc.
#[derive(Clone)]
pub struct LeptosTransport {
    /// wasm is single-threaded; SendWrapper makes the Rc satisfy the Send bound of leptos component closures
    /// (the same approach as the former WebSocketHandle).
    pub inner: send_wrapper::SendWrapper<std::rc::Rc<dyn Transport>>,
    /// The **batch** of downstream frames not yet processed; bridged from the transport.on() read loop.
    ///
    /// A queue rather than "the latest frame": the read loop can take several frames back to back (an unbounded channel is always ready),
    /// and the Effect only runs at a microtask boundary. With a latest-value signal the later frame overwrites the earlier one; the symptom is "the layout pushed
    /// on connect is lost, the view is blank" — that is dropped frames, not a rendering problem. Order is semantic here: a layout must be applied before
    /// its subsequent set/append.
    pub frame: RwSignal<Vec<Vec<u8>>>,
}

impl Ctx {
    /// Assembly: pass in an already connected transport (lib.rs picks the implementation per feature).
    /// Framework bridge: `spawn_local` reads the stream and writes the `frame` signal, and an Effect consumes, decodes and dispatches.
    /// # Panics
    /// Construct only inside a leptos owner scope (at mount/render time); without an owner there is
    /// no reactive graph, and construction itself has no basis — this is a runtime invariant, not a returnable error.
    #[allow(clippy::expect_used)]
    pub fn new(transport: std::rc::Rc<dyn Transport>, codec: ActiveCodec) -> Self {
        let frame = RwSignal::new(Vec::new());
        let layout = RwSignal::new(Accrete::text(Default::default()));
        let data = RwSignal::new(HashMap::new());
        let list = RwSignal::new(HashMap::new());
        let vals = RwSignal::new(HashMap::new());
        let config = RwSignal::new(HashMap::new());
        let drag = RwSignal::new(None);

        // Read loop: transport.on() -> frame signal
        {
            let mut stream = transport.on();
            let frame_writer = frame;
            leptos::task::spawn_local(async move {
                use futures::StreamExt;
                while let Some(bytes) = stream.next().await {
                    frame_writer.update(|q| q.push(bytes));
                }
            });
        }

        let ctx = Ctx {
            transport: LeptosTransport {
                inner: send_wrapper::SendWrapper::new(transport),
                frame,
            },
            codec,
            layout,
            data,
            list,
            vals,
            config,
            drag,
            owner: Owner::current().expect("Ctx::new requires a reactive owner"),
            bundle: None,
        };

        // Consume downstream frames and dispatch
        let ctx_clone = ctx.clone();
        Effect::new(move |_| {
            let batch = ctx_clone.transport.frame.get();
            if batch.is_empty() {
                return;
            }
            // Take the whole batch, then dispatch: clearing must come before dispatch, dispatch itself retriggers this Effect, and an empty batch returns immediately.
            ctx_clone.transport.frame.set(Vec::new());
            for b in batch {
                // Receive side adapts to both formats: the first byte of the frame fixes the format, regardless of the send format pinned by ?codec=;
                // a decoded frame is the action list itself — the envelope belongs to the host (ADR 0021 decision 6).
                match ctx_clone.codec.decode_auto::<Vec<Content<Accrete>>>(&b) {
                    Ok(actions) => dispatch_msg(&actions, &ctx_clone),
                    // If a decode failure were silently dropped the symptom would be "ws fine but the view is blank" — it must leave a trace
                    Err(e) => tracing::error!("ws frame decode failed: {e}"),
                }
            }
        });

        ctx
    }

    pub async fn send(&self, event: impl AsRef<str>, id: Option<String>, content: Value) {
        self.transport
            .inner
            .emit(transport::Event {
                name: event.as_ref().to_string(),
                id,
                data: content,
            })
            .await;
    }

    pub fn set(&self, name: impl AsRef<str>, accrete: Accrete) {
        self.slot_for_data(name.as_ref())
            .set(Some(Arc::new(accrete)));
    }

    /// One way out for every write (ADR 0031 §2): the sink's `scope` decides the
    /// destination, and nothing else does. `local` assigns in this page and
    /// wraps the payload as `Outflow {dest: slot, id?, data}` so a subscriber
    /// cannot tell a remote write from an in-page emit (ADR 0010/0011);
    /// `remote` is the existing upward event; `both` does the two.
    /// False when the sink has no address — the caller keeps its own fallback.
    pub fn emit(&self, sink: &accrete::Bind, id: Option<String>, payload: Value) -> bool {
        let Some(slot) = sink.slot.clone() else {
            return false;
        };
        if sink.scope.local() {
            let wrapped = to_value(&content::Outflow {
                dest: slot.clone(),
                id: id.clone(),
                data: payload.clone(),
            })
            .unwrap_or(Value::Null);
            self.slot_for_value(&slot).set(Some(wrapped));
        }
        if sink.scope.remote() {
            let ctx = self.clone();
            leptos::task::spawn_local(async move {
                ctx.send(slot, id, payload).await;
            });
        }
        true
    }

    /// Get (or create on first use) the list slot of a source. The outer map is read untracked:
    /// subscribing to this slot is enough, and a change in the key set should not disturb the reader.
    pub fn slot_for_list(&self, source: &str) -> ListSlot {
        if let Some(s) = self.list.get_untracked().get(source).copied() {
            return s;
        }
        // Built under the root owner: storage lives as long as Ctx, and Effect/render scope rotation cannot dispose it
        let slot = self
            .owner
            .with(|| RwSignal::new(std::sync::Arc::new(Vec::new())));
        let name = source.to_string();
        self.list.update(|m| {
            m.entry(name).or_insert(slot);
        });
        self.list
            .get_untracked()
            .get(source)
            .copied()
            .unwrap_or(slot)
    }

    /// Get (or create on first use) the data slot of a source (same semantics as slot_for_list).
    pub fn slot_for_data(&self, source: &str) -> DataSlot {
        if let Some(s) = self.data.get_untracked().get(source).copied() {
            return s;
        }
        let slot = self.owner.with(|| RwSignal::new(None));
        let name = source.to_string();
        self.data.update(|m| {
            m.entry(name).or_insert(slot);
        });
        self.data
            .get_untracked()
            .get(source)
            .copied()
            .unwrap_or(slot)
    }

    /// Get (or create on first use) the local value slot of a slot (ADR 0008 local plane,
    /// same semantics as slot_for_data — per-key signal, built under the root owner).
    pub fn slot_for_value(&self, source: &str) -> ValueSlot {
        if let Some(s) = self.vals.get_untracked().get(source).copied() {
            return s;
        }
        let slot = self.owner.with(|| RwSignal::new(None));
        let name = source.to_string();
        self.vals.update(|m| {
            m.entry(name).or_insert(slot);
        });
        self.vals
            .get_untracked()
            .get(source)
            .copied()
            .unwrap_or(slot)
    }

    /// Get (or create on first use) the slot of an option (same semantics as slot_for_value — per-key signal, built under the root owner).
    /// Reading an option that has not appeared yet creates a slot for it: without a slot there is nothing to subscribe to, and "never written" and "does not exist" are
    /// the same state to begin with here, differing only in whether the value is empty.
    pub fn config_slot(&self, name: &str) -> ConfigSlot {
        if let Some(s) = self.config.get_untracked().get(name).copied() {
            return s;
        }
        let slot = self.owner.with(|| RwSignal::new(ConfigValue::default()));
        let key = name.to_string();
        self.config.update(|m| {
            m.entry(key).or_insert(slot);
        });
        self.config
            .get_untracked()
            .get(name)
            .copied()
            .unwrap_or(slot)
    }

    /// The effective option value (local preferred, remote fallback). Reading subscribes to this one option — other options changing does not rerun.
    pub fn config_get(&self, name: &str) -> Option<Value> {
        self.config_slot(name).get().effective().cloned()
    }

    /// The page's own write form (ADR 0038 §2): the first pointer segment is the option name, the rest is written into that option's `local`
    /// half; an option-level `null` clears the local half, handing the option back to the far side. A write creates the containers along the path —
    /// a write has a path, so there is no such thing as "hitting the wrong target" (the rejection semantics of `patch` are another matter).
    pub fn set_config(&self, path: &str, value: Value) -> Result<(), String> {
        let segs = pointer_segments(path)?;
        let Some((name, rest)) = segs.split_first() else {
            return Err(format!("config path names no option: {path:?}"));
        };
        let slot = self.config_slot(name);
        let cur = slot.get_untracked();
        let local = if rest.is_empty() && value.is_null() {
            None
        } else {
            let mut acc = cur.local.clone().unwrap_or(Value::Null);
            write_into(&mut acc, rest, value)?;
            Some(acc)
        };
        let next = ConfigValue {
            local,
            remote: cur.remote.clone(),
        };
        if next != cur {
            slot.set(next);
        }
        Ok(())
    }

    /// Mirror every option onto the document root as `data-<name>` (ADR 0038 §3): one effect per option,
    /// a write sets the attribute, a clear removes it, and other options are not rewritten along with it. The option name goes into the attribute name as is (`skin` →
    /// `data-skin`), so CSS needs only one selector.
    pub fn mirror_config(&self) {
        let Some(root) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
        else {
            tracing::warn!("mirror_config: no document root to mirror onto");
            return;
        };
        // An effect only grows when the key set changes: options already attached keep following values with their own.
        let seen: std::rc::Rc<std::cell::RefCell<std::collections::HashSet<String>>> =
            Default::default();
        let ctx = self.clone();
        let owner = self.owner.clone();
        Effect::new(move |_| {
            for name in ctx.config.get().keys().cloned().collect::<Vec<_>>() {
                if !seen.borrow_mut().insert(name.clone()) {
                    continue;
                }
                let slot = ctx.config_slot(&name);
                let el = root.clone();
                let attr = format!("data-{name}");
                owner.with(|| {
                    let el = el.clone();
                    let attr = attr.clone();
                    Effect::new(move |_| {
                        // Subscribe to this one option only
                        let v = slot.get().effective().cloned();
                        match v {
                            Some(v) => {
                                let text = match &v {
                                    Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                };
                                if el.set_attribute(&attr, &text).is_err() {
                                    tracing::warn!("mirror_config: cannot set {attr}");
                                }
                            }
                            None => {
                                el.remove_attribute(&attr).ok();
                            }
                        }
                    });
                });
            }
        });
    }
}

/// One member of a frame lands in the remote half: `null` clears it, and an unchanged value returns None (no write, no notification).
fn merge_remote(cur: &ConfigValue, incoming: &Value) -> Option<ConfigValue> {
    let next = ConfigValue {
        local: cur.local.clone(),
        remote: if incoming.is_null() {
            None
        } else {
            Some(incoming.clone())
        },
    };
    (next != *cur).then_some(next)
}

/// JSON Pointer (RFC 6901) segments, with `~1`/`~0` unescaped — the same syntax as `Accrete::with_pointer`.
fn pointer_segments(path: &str) -> Result<Vec<String>, String> {
    if !path.starts_with('/') {
        return Err(format!("config path must start with '/': {path:?}"));
    }
    Ok(path[1..]
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect())
}

/// Write the value into the position named by `segs`, growing missing containers as needed. An empty path = whole-value replacement.
fn write_into(root: &mut Value, segs: &[String], value: Value) -> Result<(), String> {
    let Some((seg, rest)) = segs.split_first() else {
        *root = value;
        return Ok(());
    };
    match root {
        Value::Object(map) => {
            let child = map.entry(seg.clone()).or_insert(Value::Null);
            write_into(child, rest, value)
        }
        Value::Array(arr) => {
            let at: usize = seg
                .parse()
                .map_err(|_| format!("config path segment {seg:?} is not an array index"))?;
            let child = arr
                .get_mut(at)
                .ok_or_else(|| format!("config path index {at} is out of range"))?;
            write_into(child, rest, value)
        }
        Value::Null => {
            // Never written = empty: grow an object at this segment, then keep going
            *root = Value::Object(Map::new());
            write_into(root, segs, value)
        }
        _ => Err(format!("config path hits a scalar at segment {seg:?}")),
    }
}

fn dispatch_msg(actions: &[Content<Accrete>], ctx: &Ctx) {
    // A frame is the action list: there is no message-level `ev`, and so no gate asking "is this frame for me"
    // (ADR 0021 decision 6 — the envelope, along with its ev, belongs to the host). `Influx.slot` is a data slot name,
    // a render-internal address unrelated to the shape of the frame.
    for c in actions {
        match c {
            Content::Tmpl(x) => {
                // Registration is validation (ADR 0037): every `$name` in the tree must match params one to one,
                // and a definition is a tree parsed once. On rejection the definition does not enter the table — the next registration of the same name is still
                // a clean attempt, not a silent reuse of the previous one.
                if let Err(e) = TMPL
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .register(&x.name, &x.params.iter().map(String::as_str).collect::<Vec<_>>(), &x.data)
                {
                    tracing::warn!("{e}");
                }
            }
            Content::Create(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().unwrap_or_else(std::sync::PoisonError::into_inner);
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                tracing::info!("create: layout set, root = {:.100?}", d);
                // Writes to signals inside an Effect are always set/untracked; for tracked reads see the comment above
                ctx.layout.set(d);
            }
            Content::Set(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().unwrap_or_else(std::sync::PoisonError::into_inner);
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                // One endpoint of a cycle can appear here (the value references some slot); the other endpoint is written elsewhere.
                // Every write asks once: would this value make rendering loop back to itself.
                if let Some(path) = slots::note(&x.slot, &d, true) {
                    tracing::error!(
                        "slot cycle {} — the value would render itself; dropped",
                        path.join(" -> ")
                    );
                    continue;
                }
                ctx.slot_for_data(&x.slot).set(Some(Arc::new(d)));
            }
            Content::Append(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().unwrap_or_else(std::sync::PoisonError::into_inner);
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                // untracked read: dispatch_msg runs inside the Effect that consumes frames,
                // and a tracked read would make the Effect subscribe to slots it writes → self-exciting loop.
                let slot = ctx.slot_for_list(&x.slot);
                let cur = slot.get_untracked();
                let mut list = std::sync::Arc::unwrap_or_clone(cur);
                // Row identity lives in the data (ADR 0005: `data.id`); an empty-string id = a positional row
                // (no identity, may be appended repeatedly), matching the DSL's no-id convention
                let id = d.get_id().clone().filter(|s| !s.is_empty());
                if let Some(id) = &id {
                    // Row id uniqueness is guarded at the boundary (ADR 0005): a duplicate append is refused,
                    // so keyed render identity is not left for downstream to fight over.
                    if list.iter().any(|i| i.get_id().as_deref() == Some(id.as_str())) {
                        tracing::warn!(
                            "append rejected: event {:?} already has row id {:?}",
                            x.slot,
                            id
                        );
                        continue;
                    }
                }
                // Landing spot: 0 = the end; a positive number counts inward from the end (+1 = before the last one); a negative one counts inward from the head
                // (-1 = the head). Out of range is refused and counted — clamping would put the row where the caller did not ask.
                let Some(spot) = content::append_index(list.len(), x.position) else {
                    tracing::warn!(
                        "append rejected: event {:?} has {} rows, position {} is out of range",
                        x.slot,
                        list.len(),
                        x.position
                    );
                    continue;
                };
                if let Some(path) = slots::note(&x.slot, &d, false) {
                    tracing::error!(
                        "slot cycle {} — the row would render itself; append dropped",
                        path.join(" -> ")
                    );
                    continue;
                }
                list.insert(spot, std::sync::Arc::new(d));
                slot.set(Arc::new(list));
            }
            Content::Remove(x) => {
                let slot = ctx.slot_for_list(&x.slot);
                let cur = slot.get_untracked();
                let mut list = std::sync::Arc::unwrap_or_clone(cur);
                let before = list.len();
                list.retain(|i| i.get_id().as_deref() != Some(x.id.as_str()));
                if list.len() != before {
                    slot.set(Arc::new(list));
                } else {
                    tracing::warn!("remove: slot {:?} has no row id {:?}", x.slot, x.id);
                }
            }
            Content::Patch(x) => {
                // Plane routing (ADR 0005): "" = the layout root; an id set = a list row;
                // anything else = a named data slot.
                let result = if x.slot.is_empty() {
                    let mut d = ctx.layout.get_untracked();
                    let r = apply_patch(&mut d, x);
                    // Write back to the layout signal, otherwise the reactive root (lib.rs tracked get) never rebuilds
                    if r.is_ok() {
                        ctx.layout.set(d);
                    }
                    r
                } else if let Some(id) = &x.id {
                    let slot = ctx.slot_for_list(&x.slot);
                    let cur = slot.get_untracked();
                    let mut list = std::sync::Arc::unwrap_or_clone(cur);
                    let r = match list
                        .iter_mut()
                        .find(|i| i.get_id().as_deref() == Some(id.as_str()))
                    {
                        // Rows are shared: change in place when exclusive, clone just this row when someone else still holds it.
                        Some(row) => apply_patch(std::sync::Arc::make_mut(row), x),
                        None => Err(format!("no row {id:?} in {:?}", x.slot)),
                    };
                    if r.is_ok() {
                        slot.set(Arc::new(list));
                    }
                    r
                } else {
                    let slot = ctx.slot_for_data(&x.slot);
                    match slot.get_untracked().map(|a| std::sync::Arc::unwrap_or_clone(a)) {
                        Some(mut d) => {
                            let r = apply_patch(&mut d, x);
                            if r.is_ok() {
                                slot.set(Some(Arc::new(d)));
                            }
                            r
                        }
                        None => Err(format!("no data slot {:?}", x.slot)),
                    }
                };
                if let Err(e) = result {
                    // A miss is a legitimate degradation (a patch may arrive before its create); warn keeps it observable.
                    tracing::warn!("patch {x:?} skipped: {e}");
                }
            }
            Content::Config(x) => {
                // Page options (ADR 0038 §2): merged per member, and only the remote half is written — a frame never overwrites
                // the page's own choice (the priority ADR 0031 §1 sets for `both`).
                for (name, v) in &x.data {
                    let slot = ctx.config_slot(name);
                    let cur = slot.get_untracked();
                    if let Some(next) = merge_remote(&cur, v) {
                        tracing::info!("config {name:?} = {:.120?}", v);
                        slot.set(next);
                    }
                }
            }
            Content::Empty => {}
        }
    }
}

/// Route a patch's op onto the target Accrete node (Value-domain addressing).
fn apply_patch(target: &mut Accrete, p: &content::PatchOp) -> Result<(), String> {
    match p.op {
        PatchKind::Replace => target.replace_at(&p.path, p.value.clone()),
        PatchKind::Append => target.append_at(&p.path, p.value.clone()),
    }
}

/// Slot references, and the cycle they would close.
///
/// A slot's value that renders a node reading slot T gives the edge S → T. If the graph has a
/// cycle, rendering never finishes: S draws T, T draws back to S, and the wasm stack dies somewhere
/// unrelated (measured: `RangeError: Maximum call stack size exceeded` reported inside a navbar,
/// while the loop was `pages` → `island:x` → `pages`).
///
/// The check is here, at the WRITE, on data alone. It cannot live in the render functions: the loop
/// was measured entering each view closure at depth 1, one after another — never nested — so no
/// render-time stack (depth counter, in-flight set) can see it.
mod slots {
    use accrete::{Accrete, AccreteOps};
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    thread_local! {
        static REFS: RefCell<HashMap<String, HashSet<String>>> = RefCell::new(HashMap::new());
        /// Reused across calls: the walk allocates nothing beyond this set.
        static SCRATCH: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    }

    /// Every slot a value reads, walked directly over the tree — no serialising, no intermediate
    /// shape. `borrow_children` is the trait method every variant answers, so this stays correct
    /// when a variant is added.
    fn collect(accrete: &Accrete, out: &mut HashSet<String>) {
        // Both channels address a slot; which plane it lives in does not matter
        // to the cycle check, only that the read follows the slot.
        for b in [accrete.get_source(), accrete.get_sink()].into_iter().flatten() {
            if let Some(slot) = &b.slot {
                out.insert(slot.clone());
            }
        }
        if let Some(children) = accrete.borrow_children() {
            for c in children {
                collect(c, out);
            }
        }
    }

    /// Records what `slot` will reference and returns the cycle that would close, if any.
    /// `replacing` = a `set` replaces the contents; otherwise the references are added to them.
    ///
    /// Cost on the hot path is one walk of the value with no allocation beyond a reused set; a value
    /// that reads nothing (a text row, most data frames) returns here without touching the graph.
    pub fn note(slot: &str, accrete: &Accrete, replacing: bool) -> Option<Vec<String>> {
        SCRATCH.with(|s| {
            let mut refs = s.borrow_mut();
            refs.clear();
            collect(accrete, &mut refs);
            if refs.is_empty() {
                // Reads nothing: this value cannot be part of a cycle, and the graph is untouched.
                return None;
            }
            let refs = &*refs;
            REFS.with(|r| {
                let mut graph = r.borrow_mut();
                if replacing {
                    graph.insert(slot.to_string(), refs.clone());
                } else {
                    graph
                        .entry(slot.to_string())
                        .or_default()
                        .extend(refs.iter().cloned());
                }
                let mut path = vec![slot.to_string()];
                let mut seen = HashSet::from([slot.to_string()]);
                if walk(&graph, slot, slot, &mut path, &mut seen) {
                    Some(path)
                } else {
                    None
                }
            })
        })
    }

    fn walk(
        graph: &HashMap<String, HashSet<String>>,
        from: &str,
        target: &str,
        path: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) -> bool {
        let Some(next) = graph.get(from) else {
            return false;
        };
        for n in next {
            if n == target {
                path.push(n.clone());
                return true;
            }
            if !seen.insert(n.clone()) {
                continue;
            }
            path.push(n.clone());
            if walk(graph, n, target, path, seen) {
                return true;
            }
            path.pop();
        }
        false
    }
}

/// Render depth guard (recursive rendering in wasm eats the stack; the symptom is `memory access out of bounds` or
/// `Maximum call stack size exceeded` — the crash lands in malloc or `Option<String>::clone`,
/// dozens of frames away from the real recursion point, and looks like a memory bug).
///
/// Once some data can render itself (a value in a slot holds a node bound to that same slot again), the crash is a certainty,
/// and after wasm dies the **whole page** stops responding, not just one region. So depth is capped here: when the limit is exceeded,
/// log one line naming which node and which id, then render as empty — one bad node should not take the whole page with it.
const MAX_RENDER_DEPTH: usize = 64;

thread_local! {
    static RENDER_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct DepthGuard;

impl Drop for DepthGuard {
    fn drop(&mut self) {
        RENDER_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Render an accrete into a view (for external triggers).
pub fn render_accrete(ctx: &Ctx, accrete: &Accrete) -> AnyView {
    let depth = RENDER_DEPTH.with(|d| {
        let v = d.get() + 1;
        d.set(v);
        v
    });
    if depth > MAX_RENDER_DEPTH {
        RENDER_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        tracing::error!(
            "render depth {} exceeded at {} (id {:?}) — this node renders itself; drawing nothing",
            MAX_RENDER_DEPTH,
            accrete.get_type(),
            accrete.get_id()
        );
        return ().into_any();
    }
    let _guard = DepthGuard;
    dispatch(accrete, ctx)
}

/// Render a group of child accretes.
pub fn render_children(ctx: &Ctx, subs: &[Accrete]) -> Vec<AnyView> {
    subs.iter().map(|b| render_accrete(ctx, b)).collect()
}

#[cfg(test)]
mod config_tests {
    use super::{ConfigValue, merge_remote, pointer_segments, write_into};
    use serde_json::{Value, json};

    fn remote(v: Value) -> ConfigValue {
        ConfigValue {
            local: None,
            remote: Some(v),
        }
    }

    #[test]
    fn a_new_member_lands_in_the_remote_half() {
        let next = merge_remote(&ConfigValue::default(), &json!("sketch")).unwrap();
        assert_eq!(next.remote, Some(json!("sketch")));
        assert_eq!(next.local, None);
    }

    #[test]
    fn an_equal_member_is_not_written() {
        let cur = remote(json!("sketch"));
        assert_eq!(merge_remote(&cur, &json!("sketch")), None);
    }

    #[test]
    fn null_clears_the_remote_half_and_keeps_the_local_one() {
        let cur = ConfigValue {
            local: Some(json!("normal")),
            remote: Some(json!("sketch")),
        };
        let next = merge_remote(&cur, &Value::Null).unwrap();
        assert_eq!(next.remote, None);
        assert_eq!(next.local, Some(json!("normal")));
    }

    #[test]
    fn clearing_an_absent_half_writes_nothing() {
        let cur = ConfigValue {
            local: Some(json!("normal")),
            remote: None,
        };
        assert_eq!(merge_remote(&cur, &Value::Null), None);
    }

    #[test]
    fn local_wins_and_remote_is_the_fallback() {
        let both = ConfigValue {
            local: Some(json!("normal")),
            remote: Some(json!("sketch")),
        };
        assert_eq!(both.effective(), Some(&json!("normal")));
        let only_remote = remote(json!("sketch"));
        assert_eq!(only_remote.effective(), Some(&json!("sketch")));
        assert_eq!(ConfigValue::default().effective(), None);
    }

    #[test]
    fn a_pointer_creates_the_object_it_names() {
        let mut acc = Value::Null;
        let segs = pointer_segments("/density/number").unwrap();
        write_into(&mut acc, &segs, json!(4)).unwrap();
        assert_eq!(acc, json!({"density": {"number": 4}}));
    }

    #[test]
    fn a_pointer_through_a_scalar_is_refused() {
        let mut acc = json!({"density": "compact"});
        let segs = pointer_segments("/density/number").unwrap();
        assert!(write_into(&mut acc, &segs, json!(4)).is_err());
    }

    #[test]
    fn a_path_without_a_leading_slash_is_refused() {
        assert!(pointer_segments("skin").is_err());
    }

    #[test]
    fn pointer_escapes_are_unescaped() {
        assert_eq!(
            pointer_segments("/a~1b/c~0d").unwrap(),
            vec!["a/b".to_string(), "c~d".to_string()]
        );
    }
}

