use crate::hooks::LoomState;
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
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Table of registered stamp definitions (ADR 0037: a definition is a node tree, a value is a whole-value replacement).
static TMPL: LazyLock<RwLock<accrete::template::Templates>> =
    LazyLock::new(|| RwLock::new(accrete::template::Templates::new()));

/// Globally shared state container: owns layout, data, lists, and transport send/receive.
///
/// `loom` is the render-time environment: `loom_` injects its own
/// `LoomState` and clones `Ctx` for the subtree, so ownership follows the
/// clone chain; sibling branches each keep their own clone and cannot pollute
/// each other. Nested looms shadow the outer one.
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

/// The slot a page's options live in unless the mount options name another (ADR 0038 §2, revised): ONE slot holding
/// one object whose members are the options. An option is not a kind of thing — it is a value, read through
/// the ordinary planes with the ordinary `path`, which is why nothing below it needs machinery of its own.
pub const DEFAULT_CONFIG_SLOT: &str = "__CONFIG__";

/// The seams a host takes over (ADR 0047). Empty by default: a page that names none behaves exactly
/// as before, which is how a renderer grows a hook surface without changing anyone's page.
///
/// Callbacks are `Rc<dyn Fn>` because `Ctx` is cloned freely across the tree; a hook is shared, not
/// copied, so the host registers one closure and every clone sees it.
/// One slot observer: `(name, value)` after the write (ADR 0047 §4).
pub type SlotObserver = std::sync::Arc<dyn Fn(&str, Option<&Accrete>) + Send + Sync>;
/// One uplink rewriter: given `(dest, data)`, return a replacement payload or `None` to leave it.
pub type DestRewriter = std::sync::Arc<dyn Fn(&str, Value) -> Option<Value> + Send + Sync>;

#[derive(Clone, Default)]
pub struct Hooks {
    /// `(slot, callback)`: a value landed in that slot — `"*"` matches every name. Called AFTER the
    /// write, deferred, so a slow observer cannot hold up the render that the write just triggered.
    pub slots: Vec<(String, SlotObserver)>,
    /// The root was replaced (`create`): the whole page, not one slot (ADR 0047 §3). Called after the
    /// layout is set, and it does not produce "all slots are now empty" notifications.
    pub create: Option<std::sync::Arc<dyn Fn(&Accrete) + Send + Sync>>,
    /// `(dest, callback)`: the uplink, before it is encoded. `None` leaves the payload as it is; a
    /// value replaces it. Synchronous by necessity — the frame cannot leave without the answer.
    pub dests: Vec<(String, DestRewriter)>,
}

#[derive(Clone)]
pub struct Ctx {
    pub transport: LeptosTransport,
    pub codec: ActiveCodec,
    pub layout: RwSignal<Accrete>,
    pub data: RwSignal<HashMap<String, DataSlot>>,
    pub list: RwSignal<HashMap<String, ListSlot>>,
    /// local plane: named Value slots shared by where `kind: local` events land and by their subscription.
    pub vals: RwSignal<HashMap<String, ValueSlot>>,
    /// The name of the slot this page's options live in (ADR 0038 §2, revised); `None` when the page asked
    /// for none (`config: ""` in the mount options). Options are not a plane of their own: the slot is a slot, the object
    /// in it is a value, and the page's own half is the local plane every other control already writes.
    pub config: RwSignal<Option<String>>,
    /// Drag session (ADR 0034): `Some` means "a drag is in flight", and what it holds is that source frame node
    /// itself — the cargo is its own subtree, so there is no second parallel structure (a copy and the original would sooner or later
    /// disagree). This is the only cross-component transient state in the renderer: source and drop target sit apart with other nodes between them, and a local signal
    /// cannot reach across. It enters no slot and never goes on the wire: "who is in flight, over what" is a fact of the browser, not
    /// of the producer's tree. There can be only one copy of it; each component keeping its own is the opposite of this design.
    pub drag: RwSignal<Option<Accrete>>,
    /// Permanent owner of the slots: the app root scope captured at Ctx::new.
    /// If a slot were built inside the scope of the frame-consuming Effect, an Effect rerun would dispose its storage,
    /// and what is left in the outer map is a dead handle (panic: already disposed).
    pub owner: Owner,
    pub loom: Option<Arc<LoomState>>,
    /// The host's seams (ADR 0047): empty until a page names one.
    pub hooks: Hooks,
    /// Slot values written since the last drain, one entry per name — the newest (ADR 0047 §4). A map
    /// instead of a queue: a host that watches a slot wants its current value, not every intermediate.
    pub pending: RwSignal<HashMap<String, Arc<Accrete>>>,
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
        let config = RwSignal::new(Some(DEFAULT_CONFIG_SLOT.to_string()));
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
            loom: None,
            hooks: Hooks::default(),
            pending: RwSignal::new(HashMap::new()),
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
            // The batch is applied, the page has been told to redraw: only now do the host's observers
            // hear about it (ADR 0047 §4).
            ctx_clone.drain_slots();
        });

        ctx
    }

    pub async fn send(&self, event: impl AsRef<str>, id: Option<String>, content: Value) {
        let name = event.as_ref();
        // The host's rewriters run before the frame is built (ADR 0047 §5): registered order, `"*"`
        // after any name that matches. `None` means "leave it alone", which is why a rewriter that only
        // wants to look costs nothing. Synchronous by necessity — the frame cannot leave without an
        // answer — but it blocks this frame only, never a render.
        let mut content = content;
        for (watched, rewrite) in &self.hooks.dests {
            if watched == "*" || watched == name {
                if let Some(replaced) = rewrite(name, content.clone()) {
                    content = replaced;
                }
            }
        }
        self.transport
            .inner
            .emit(transport::Event {
                name: name.to_string(),
                id,
                data: content,
            })
            .await;
    }

    pub fn set(&self, name: impl AsRef<str>, accrete: Accrete) {
        let name = name.as_ref();
        let value = Arc::new(accrete);
        self.slot_for_data(name).set(Some(value.clone()));
        // The host's observers run after the write and outside it (ADR 0047 §4): a deferred task, so
        // the reactive update above is not waiting on anyone's callback. `"*"` matches every name.
        // Coalesce, do not queue (ADR 0047 §4): the newest value per name is the only one worth
        // delivering, and overwriting a map needs no scheduler — `drain_slots` runs at the end of the
        // pass that consumed the frames, so a host's callback never sits inside the write that caused it.
        if !self.hooks.slots.is_empty() {
            // A pass can outlive its owner: the root was replaced (`create`) or the mount was torn down
            // with frames still in flight. A disposed signal is not an error to shout about — there is
            // nobody left to hear it — so the write is dropped instead of panicking.
            let _ = self.pending.try_update(|pending| {
                pending.insert(name.to_owned(), value);
            });
        }
    }

    /// Deliver what accumulated since the last pass: one callback per name, with its newest value.
    pub fn drain_slots(&self) {
        let Some(taken) = self.pending.try_update(std::mem::take) else {
            return; // disposed: nothing to deliver to
        };
        for (name, value) in taken {
            for (watched, notify) in &self.hooks.slots {
                if watched == "*" || watched == &name {
                    notify(&name, Some(&value));
                }
            }
        }
    }

    /// One way out for every write (ADR 0031 §2): the sink's `scope` decides the
    /// destination, and nothing else does. `local` assigns in this page and
    /// wraps the payload as `Outflow {dest: slot, id?, data}` so a subscriber
    /// cannot tell a remote write from an in-page emit (ADR 0010/0011);
    /// `remote` is the existing upward event; `both` does the two.
    /// False when the sink has no address — the caller keeps its own fallback.
    pub fn emit(&self, sink: &accrete::Sink, id: Option<String>, payload: Value) -> bool {
        let Some(dest) = sink.dest.clone() else {
            return false;
        };
        if sink.scope.local() {
            match sink.path.as_deref() {
                // A sink WITH a path writes a value into the local plane, merged at that path (ADR 0031 §2):
                // the control owns one member of the value and does not have to know the rest of it.
                Some(path) => self.write_local_at(&dest, path, payload.clone()),
                // A sink with no path is a message: the plane holds the event and pages read it back with
                // `path: /data`.
                None => {
                    let wrapped = to_value(&content::Outflow {
                        dest: dest.clone(),
                        id: id.clone(),
                        data: payload.clone(),
                    })
                    .unwrap_or(Value::Null);
                    self.slot_for_value(&dest).set(Some(wrapped));
                }
            }
        }
        if sink.scope.remote() {
            let ctx = self.clone();
            leptos::task::spawn_local(async move {
                ctx.send(dest, id, payload).await;
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

    /// The slot this page's options live in, if the page has any.
    pub fn config_slot(&self) -> Option<String> {
        self.config.get_untracked()
    }

    /// Point the page's options at another slot — `config` in the mount options — or turn them off.
    pub fn set_config_slot(&self, name: Option<String>) {
        self.config.set(name);
    }

    /// One option's effective value: the page's own half first, the far side's after it, decided member by
    /// member (ADR 0031 §1 read through the two ordinary planes). A control that owns `sketch` therefore does
    /// not darken `density`: the far side's members stay readable while the page has not touched them.
    pub fn config_get(&self, member: &str) -> Option<Value> {
        let slot = self.config.get_untracked()?;
        self.config_members(&slot).remove(member)
    }

    /// A sink that names a `path` writes INTO the local plane at that path, merging (ADR 0031 §2): several
    /// controls can own several members of one value, and none of them has to know the others. A sink with no
    /// path keeps the event shape (`{dest, id, data}`) — that is what a slot used as a message channel carries,
    /// and pages read it back with `path: /data`.
    pub fn write_local_at(&self, slot: &str, path: &str, value: Value) {
        let segs = match pointer_segments(path) {
            Ok(segs) => segs,
            Err(e) => {
                tracing::warn!("local write into {slot:?} skipped: {e}");
                return;
            }
        };
        let sig = self.slot_for_value(slot);
        let mut acc = sig.get_untracked().unwrap_or(Value::Null);
        // A null at one segment removes that member: "the page stops choosing this", which hands the member
        // back to the far side on the next read.
        let result = match (value.is_null(), segs.as_slice()) {
            (true, [one]) => remove_member(&mut acc, one),
            _ => write_into(&mut acc, &segs, value),
        };
        match result {
            Ok(()) => sig.set(Some(acc)),
            Err(e) => tracing::warn!("local write {path:?} into {slot:?} skipped: {e}"),
        }
    }

    /// Mirror the option slot onto the document root as `data-<member>` (ADR 0038 §3): one effect for the slot,
    /// one attribute per scalar member of the effective value. A member that goes away takes its attribute with
    /// it, and an attribute this never wrote is never touched.
    pub fn mirror_config(&self) {
        let Some(root) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
        else {
            tracing::warn!("mirror_config: no document root to mirror onto");
            return;
        };
        let ctx = self.clone();
        let written: std::rc::Rc<std::cell::RefCell<std::collections::HashSet<String>>> =
            Default::default();
        Effect::new(move |_| {
            let members = match ctx.config.get() {
                Some(slot) => ctx.config_members(&slot),
                None => Default::default(),
            };
            let mut still: std::collections::HashSet<String> = Default::default();
            for (member, value) in &members {
                let Some(text) = scalar_text(value) else {
                    continue;
                };
                let attr = format!("data-{member}");
                if root.set_attribute(&attr, &text).is_err() {
                    tracing::warn!("mirror_config: cannot set {attr}");
                    continue;
                }
                still.insert(attr);
            }
            let gone: Vec<String> = written
                .borrow()
                .iter()
                .filter(|attr| !still.contains(*attr))
                .cloned()
                .collect();
            for attr in gone {
                root.remove_attribute(&attr).ok();
            }
            *written.borrow_mut() = still;
        });
    }

    /// The effective option object: the far side's members, written over by the page's own.
    fn config_members(&self, slot: &str) -> std::collections::BTreeMap<String, Value> {
        // The far side's object is the slot's PAYLOAD — `source.default` of the node that carries it, which is
        // how every other slot value is carried (ADR 0012: an absent payload is the empty value).
        let remote = self
            .slot_for_data(slot)
            .get()
            .and_then(|node| node.get_source().and_then(|s| s.default.clone()));
        let local = self.slot_for_value(slot).get();
        merge_members(remote.as_ref(), local.as_ref())
    }
}

/// The effective options: the far side's members, written over by the page's own (ADR 0038 §2 revised).
/// Per member, not per object — a page that chose `sketch` has said nothing about `density`.
fn merge_members(
    remote: Option<&Value>,
    local: Option<&Value>,
) -> std::collections::BTreeMap<String, Value> {
    let mut members = std::collections::BTreeMap::new();
    for src in [remote, local].into_iter().flatten() {
        if let Value::Object(map) = src {
            for (k, v) in map {
                members.insert(k.clone(), v.clone());
            }
        }
    }
    members
}

/// A scalar as an attribute value; an object or an array has no one text, so it is not mirrored.
fn scalar_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Null | Value::Object(_) | Value::Array(_) => None,
        other => Some(other.to_string()),
    }
}

/// Drop one member of a local object: the write that says "the page stops choosing this".
fn remove_member(root: &mut Value, member: &str) -> Result<(), String> {
    match root {
        Value::Object(map) => {
            map.remove(member);
            Ok(())
        }
        Value::Null => Ok(()),
        _ => Err(format!(
            "the local value is not an object, cannot drop {member:?}"
        )),
    }
}

/// JSON Pointer (RFC 6901) segments, with `~1`/`~0` unescaped — the same syntax as `Accrete::with_pointer`.
fn pointer_segments(path: &str) -> Result<Vec<String>, String> {
    if !path.starts_with('/') {
        return Err(format!("config path must start with '/': {path:?}"));
    }
    // "/" is the root itself: no segments. That is how a control writes its own members into the slot
    // without owning the rest of it (a switch sets `{sketch: true}` and leaves the other options alone).
    if path == "/" {
        return Ok(Vec::new());
    }
    Ok(path[1..]
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect())
}

/// Write the value into the position named by `segs`, growing missing containers as needed. An empty path
/// is the root: two objects merge (a control owns the members it writes and not the slot), anything else
/// replaces.
fn write_into(root: &mut Value, segs: &[String], value: Value) -> Result<(), String> {
    let Some((seg, rest)) = segs.split_first() else {
        match (root, value) {
            (Value::Object(into), Value::Object(from)) => {
                for (k, v) in from {
                    into.insert(k, v);
                }
            }
            (slot, value) => *slot = value,
        }
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
                    .register(
                        &x.name,
                        &x.params.iter().map(String::as_str).collect::<Vec<_>>(),
                        &x.data,
                    )
                {
                    tracing::warn!("{e}");
                }
            }
            Content::Create(x) => {
                let mut d = x.data.clone();
                let env = TMPL
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                tracing::info!("create: layout set, root = {:.100?}", d);
                // Writes to signals inside an Effect are always set/untracked; for tracked reads see the comment above
                ctx.layout.set(d.clone());
                // The page was rebuilt: a host that holds DOM references wants to know (ADR 0047 §3).
                if let Some(announce) = &ctx.hooks.create {
                    announce(&d);
                }
            }
            Content::Set(x) => {
                let mut d = x.data.clone();
                let env = TMPL
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                let env = TMPL
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                    if list
                        .iter()
                        .any(|i| i.get_id().as_deref() == Some(id.as_str()))
                    {
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
            Content::Move(x) => {
                let slot = ctx.slot_for_list(&x.slot);
                let cur = slot.get_untracked();
                let mut list = std::sync::Arc::unwrap_or_clone(cur);
                // The row has to be there: `move` reorders, it never inserts (ADR 0005). A miss is
                // refused and counted, the same boundary `append` draws against a duplicate id.
                let Some(from) = list
                    .iter()
                    .position(|i| i.get_id().as_deref() == Some(x.id.as_str()))
                else {
                    tracing::warn!("move rejected: slot {:?} has no row id {:?}", x.slot, x.id);
                    continue;
                };
                // The count is taken with the row lifted, so the domain is one narrower than append's.
                let Some(spot) = content::move_index(list.len(), x.position) else {
                    tracing::warn!(
                        "move rejected: slot {:?} has {} rows, position {} is out of range",
                        x.slot,
                        list.len(),
                        x.position
                    );
                    continue;
                };
                // Landing back where it started is not a change: writing the slot would rebuild the
                // list's view for nothing.
                if spot == from {
                    continue;
                }
                // No new node joins the list, so no cycle can close: the row was already rendered here.
                let typing = typing();
                let row = list.remove(from);
                list.insert(spot, row);
                slot.set(Arc::new(list));
                // The row's node is reused (the list is keyed), but a DOM move still takes the node
                // out and puts it back, and a focused element that leaves the document is blurred.
                resume(typing);
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
                    match slot
                        .get_untracked()
                        .map(|a| std::sync::Arc::unwrap_or_clone(a))
                    {
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
            Content::Empty => {}
        }
    }
}

/// What the user was typing in, if anything: the element and where the caret was. A list move keeps
/// a row's node but still moves it in the DOM, so the caret has to be put back by hand.
struct Typing {
    el: web_sys::Element,
    caret: Option<(Option<u32>, Option<u32>)>,
}

fn typing() -> Option<Typing> {
    let el = web_sys::window()?.document()?.active_element()?;
    // Nobody is typing: the document's own body is what a page with no focus reports, and taking it
    // back on the way out would steal the next click.
    if el.tag_name() == "BODY" {
        return None;
    }
    let caret = if let Some(i) = el.dyn_ref::<web_sys::HtmlInputElement>() {
        Some((
            i.selection_start().ok().flatten(),
            i.selection_end().ok().flatten(),
        ))
    } else if let Some(t) = el.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        Some((
            t.selection_start().ok().flatten(),
            t.selection_end().ok().flatten(),
        ))
    } else {
        None
    };
    Some(Typing { el, caret })
}

/// Put the caret back after the move has been drawn, and only if nothing else took the focus in the
/// meantime — a user who clicked elsewhere while the frame was in flight is not to be dragged back.
fn resume(typing: Option<Typing>) {
    let Some(t) = typing else { return };
    let restore = Closure::once_into_js(move || {
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let focused = doc.active_element();
        let free = match &focused {
            None => true,
            Some(el) => {
                el.tag_name() == "BODY" || !el.is_connected() || focused.as_ref() == Some(&t.el)
            }
        };
        if !free || !t.el.is_connected() {
            return;
        }
        if let Some(h) = t.el.dyn_ref::<web_sys::HtmlElement>() {
            let _ = h.focus();
        }
        if let Some((Some(start), Some(end))) = t.caret {
            if let Some(i) = t.el.dyn_ref::<web_sys::HtmlInputElement>() {
                let _ = i.set_selection_range(start, end);
            } else if let Some(x) = t.el.dyn_ref::<web_sys::HtmlTextAreaElement>() {
                let _ = x.set_selection_range(start, end);
            }
        }
    });
    if let Some(w) = web_sys::window() {
        let _ = w.request_animation_frame(restore.unchecked_ref());
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
        if let Some(slot) = accrete.get_source().and_then(|b| b.slot.as_ref()) {
            out.insert(slot.clone());
        }
        if let Some(dest) = accrete.get_sink().and_then(|b| b.dest.as_ref()) {
            out.insert(dest.clone());
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
    use super::{merge_members, pointer_segments, remove_member, scalar_text, write_into};
    use serde_json::{Value, json};

    /// One control owns one member; the far side keeps the members the page never touched.
    #[test]
    fn local_members_win_one_at_a_time() {
        let remote = json!({"sketch": false, "density": "compact"});
        let local = json!({"sketch": true});
        let members = merge_members(Some(&remote), Some(&local));
        assert_eq!(members["sketch"], json!(true));
        assert_eq!(
            members["density"],
            json!("compact"),
            "an untouched member is still the far side's"
        );
    }

    #[test]
    fn a_page_that_chose_nothing_reads_the_far_side() {
        let remote = json!({"sketch": true});
        assert_eq!(merge_members(Some(&remote), None)["sketch"], json!(true));
        assert!(merge_members(None, None).is_empty());
    }

    /// The write a control makes is a merge: `{sketch: true}` written after `{density: ...}` adds a member,
    /// it does not replace the object.
    #[test]
    fn a_path_write_merges_into_the_local_object() {
        let mut acc = json!({"density": "compact"});
        write_into(&mut acc, &pointer_segments("/sketch").unwrap(), json!(true)).unwrap();
        assert_eq!(acc, json!({"density": "compact", "sketch": true}));
    }

    #[test]
    fn a_null_member_drops_the_pages_choice() {
        let mut acc = json!({"sketch": true, "density": "compact"});
        remove_member(&mut acc, "sketch").unwrap();
        assert_eq!(
            acc,
            json!({"density": "compact"}),
            "the far side decides that member again"
        );
        assert!(
            remove_member(&mut Value::Null, "sketch").is_ok(),
            "dropping what is not there is not an error"
        );
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
        assert!(pointer_segments("sketch").is_err());
    }

    #[test]
    fn pointer_escapes_are_unescaped() {
        assert_eq!(
            pointer_segments("/a~1b/c~0d").unwrap(),
            vec!["a/b".to_string(), "c~d".to_string()]
        );
    }

    /// A CSS attribute is one text: a bool or a number has one, an object does not.
    #[test]
    fn only_scalars_are_mirrored() {
        assert_eq!(scalar_text(&json!(true)).as_deref(), Some("true"));
        assert_eq!(scalar_text(&json!("chalk")).as_deref(), Some("chalk"));
        assert_eq!(scalar_text(&json!(3)), Some("3".to_string()));
        assert_eq!(scalar_text(&json!({"a": 1})), None);
        assert_eq!(scalar_text(&Value::Null), None);
    }
}
