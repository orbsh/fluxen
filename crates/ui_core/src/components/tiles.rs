//! `tiles` — one container for a slot's items, cut into blocks (ADR 0031 §4).
//!
//! Two axes, not two components: `stage` says how many items are on it (`all` lays every one out,
//! `one` gives the area to a single item), and `step` says what advances it (a clock the node owns,
//! or a slot the world writes). `rack` and `pages` were this node with different steppers, so the
//! machinery below is theirs: the keyed list, the position index, the rotation, the dwell, the
//! follow-the-tail scroll.
//!
//! One rule runs through all of it: **every item has a key**, read once when the item arrives
//! (ADR 0031 §4, "Every item has a key"). Selection, removal and replacement address items by
//! that key, never by a position recomputed from the current list.

use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{self, use_common_css, use_source_id};
use accrete::classify::Classify;
use accrete::{Accrete, AccreteOps, Bind, Scope, Stage, Step, Tiles};
use leptos::html::*;
use leptos::leptos_dom::helpers::set_interval_with_handle;
use leptos::prelude::*;
use leptos::tachys::reactive_graph::OwnedView;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

/// The layout a tile is drawn with: templates indexed by `selector`, plus the default for items
/// whose own selector matches none (ADR 0020). The mechanism is unchanged — only the field it
/// arrives on is (`children`, not `item`).
#[derive(Debug, Clone, Default)]
struct Layout {
    default: Option<Accrete>,
    index: HashMap<String, Accrete>,
}

impl From<Vec<Accrete>> for Layout {
    fn from(data: Vec<Accrete>) -> Self {
        let mut default = None;
        let mut index = HashMap::new();
        for l in &data {
            if let Some(x) = l.get_selector() {
                index.insert(x.to_owned(), l.clone());
            } else {
                default = Some(l.clone());
            }
        }
        Layout { index, default }
    }
}

impl Layout {
    fn select(&self, child: &Accrete) -> Option<Accrete> {
        if let Some(s) = child.get_selector()
            && let Some(i) = self.index.get(s)
        {
            return Some(i.clone());
        }
        self.default.clone()
    }
}

/// The key of every item, in list order (ADR 0031 §4).
///
/// An item that carries an `id` is addressed by it. One that does not borrows the position it had
/// when it entered, and `born` is what makes "when it entered" a fact instead of a guess: the
/// position is recorded the first time the item is seen and NEVER recomputed — recomputing it is
/// what makes removing the first item rename every item behind it, so the thing on stage silently
/// becomes a different thing.
///
/// An item without an id is addressed by the object itself (its address), because that is all the
/// slot gives us: rows are shared by pointer, so an item keeps its address while it sits in the
/// list. Rows that left are forgotten on the same pass, so a recycled address cannot inherit a
/// dead key.
fn tile_keys(rows: &[Arc<Accrete>], born: &mut HashMap<usize, String>) -> Vec<String> {
    let mut live = HashSet::with_capacity(rows.len());
    let keys: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let at = Arc::as_ptr(row) as usize;
            live.insert(at);
            row.get_id()
                .clone()
                .unwrap_or_else(|| born.entry(at).or_insert_with(|| format!("#{idx}")).clone())
        })
        .collect();
    born.retain(|at, _| live.contains(at));
    keys
}

/// Key → where that item sits now. Rebuilt whenever the list changes; this is the one thing a
/// position is still good for, and it is never the item's identity.
fn positions(keys: &[String]) -> HashMap<String, usize> {
    let mut pos = HashMap::with_capacity(keys.len());
    for (idx, k) in keys.iter().enumerate() {
        // Duplicate keys take the first hit, the same rule a linear `find` had.
        pos.entry(k.clone()).or_insert(idx);
    }
    pos
}

/// What a step slot says is on stage: a string is the key, and an emit wrapper (`{dest, id, data}`
/// — the one shape a local write takes, ADR 0010) is asked for its payload. Anything else names no
/// item, so nothing gets the stage rather than something arbitrary.
fn staged_key(v: Option<Value>) -> Option<String> {
    match v? {
        Value::String(k) => Some(k),
        Value::Object(m) => m.get("data")?.as_str().map(String::from),
        _ => None,
    }
}

/// How many LEADING items have outlived their stay. Arrivals are ordered and a stay is uniform, so
/// what has expired is always the head of the list — one number, not a deadline per item. `stamps`
/// holds the tick each item went on stage on; an item with no stamp has just arrived.
fn expired_prefix(keys: &[String], stamps: &HashMap<String, u64>, tick: u64, stay: u64) -> usize {
    let mut gone = 0;
    for key in keys {
        match stamps.get(key) {
            Some(born) if tick.saturating_sub(*born) >= stay => gone += 1,
            _ => break,
        }
    }
    gone
}

/// A slot's items, cut into blocks (ADR 0031 §4).
///
/// `stage: all` lays every item out; `stage: one` gives the area to a single one, chosen by
/// `step` — a slot that names its key, or a clock that rotates. With neither, the newest item
/// holds the area: a frame that asks for "one at a time" must not show nothing because nobody ever
/// set a cursor.
pub fn tiles_(accrete: Tiles, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    // Rows get their owner from here: the subscriptions of a row outlive the render pass that
    // built it, so the node's own scope (not the caller's) is what they hang on.
    #[allow(clippy::expect_used)]
    let parent_owner = Owner::current().expect("tiles_ requires a reactive owner");

    let stage = accrete.attrs.as_ref().map(|a| a.stage).unwrap_or_default();
    let step = accrete.attrs.as_ref().and_then(|a| a.step.clone());

    let mut css = vec!["tiles", "f"];
    if stage == Stage::One {
        // One at a time is a stack of blocks with one of them opaque: CSS draws it, the class
        // says which container it is.
        css.push("rot");
    }
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let layout: Layout = accrete.children.clone().unwrap_or_default().into();
    // No slot = nothing to tile. A list channel without an address has no items, not one.
    let Some(source) = use_source_id(&accrete).cloned() else {
        return div().id(id.as_str()).class(css.as_str()).into_any();
    };
    let slot = ctx.slot_for_list(&source);

    // Where the arrival positions live. A storage, not a signal: who minted which key is not
    // something anything subscribes to, and it has to outlive every render pass that reads it.
    let born = StoredValue::new(HashMap::<usize, String>::new());
    let keys: Memo<Arc<Vec<String>>> = Memo::new(move |_| {
        let rows = slot.get();
        let mut ks = Vec::new();
        born.update_value(|m| ks = tile_keys(&rows[..], m));
        Arc::new(ks)
    });
    let index: Memo<Arc<HashMap<String, usize>>> =
        Memo::new(move |_| Arc::new(positions(&keys.get()[..])));

    // The cursor is a KEY, never a position: an item removed ahead of it must not slide the thing
    // on screen to its neighbour.
    let on = RwSignal::new(None::<String>);
    // WHO IS ON STAGE, as the render pass asks it. A slot names the key (`step: {slot}`), a clock
    // names the next one (`step: {timer}`), and with no stepper at all the newest item holds the
    // area.
    let step_for_stage = step.clone();
    let staged: Memo<Option<String>> = Memo::new(move |_| match step_for_stage {
        Some(_) => on.get(),
        None => keys.get().last().cloned(),
    });

    // A slot the world writes: that is what `pages` was — a selection driven from outside (a nav
    // click, a route, another component). No clock.
    if let Some(Step::Slot { slot: name }) = &step {
        // Read the way every other channel is read (ADR 0031 §1): a local assignment wins, and the
        // far side's slot is what is left when there is none.
        let bind = Bind {
            slot: Some(name.clone()),
            scope: Scope::Both,
            ..Bind::default()
        };
        let ctx2 = ctx.clone();
        Effect::new(move |_| match staged_key(hooks::value_of(&ctx2, Some(&bind), true)) {
            Some(k) => on.set(Some(k)),
            // Nothing written yet: nothing on stage, which is a page area before the first click.
            None => on.set(None),
        });
    }

    // A clock the node owns, in ms. The floor of 200 is the renderer's: a slower turn is a policy,
    // a zero-length one is a spin. One interval per node, cleared when the node goes away.
    let every = step.as_ref().and_then(|s| match s {
        Step::Timer { timer } => Some(timer.max(&200).to_owned()),
        Step::Slot { .. } => None,
    });
    let rotating = every.is_some() && stage == Stage::One;
    // DWELL: every item carries its own clock and leaves when its stay is over — all of them are on
    // stage at once, unlike a rotation, which takes turns. The item stays in the slot; the renderer
    // only stops SHOWING it.
    let dwelling = every.is_some() && stage == Stage::All;

    // How many items have had their turn. Rotation walks the queue in the order it was said — an
    // item arriving while another is being read waits behind it, which is what a queue is for.
    let shown = RwSignal::new(0usize);
    if rotating {
        let every = every.unwrap_or(4000);
        let tick_keys = keys;
        // The timer must die with the node: without a handle to clear, an unmounted `tiles` keeps a
        // live callback ticking against signals that no longer exist (a wasm trap, not a leak).
        let tick = set_interval_with_handle(
            move || {
                let ks = tick_keys.get_untracked();
                if ks.is_empty() {
                    on.set(None);
                    return;
                }
                // The item after the one on stage; nobody on stage means the next item that has
                // not had a turn. A key that is gone reads the same as nobody — its item left.
                let next = match on.get_untracked().and_then(|k| ks.iter().position(|x| *x == k)) {
                    Some(pos) => pos + 1,
                    None => shown.get_untracked(),
                };
                if next < ks.len() {
                    shown.set(next + 1);
                    on.set(Some(ks[next].clone()));
                } else {
                    // The turn ends with nobody, never by wrapping back to the first item: a
                    // sentence already said should not return every so often.
                    shown.set(ks.len());
                    on.set(None);
                }
            },
            Duration::from_millis(every),
        );
        match tick {
            // `on_cleanup` wants a `Send` closure; the handle is thread-local (a JS timer), so it
            // rides a `SendWrapper` — cleanup runs on the main thread in CSR, where it is the same
            // thing.
            Ok(handle) => {
                let handle = send_wrapper::SendWrapper::new(handle);
                on_cleanup(move || handle.take().clear());
            }
            Err(e) => tracing::warn!("tiles: could not start the rotation timer: {e:?}"),
        }
        // An empty area shows the next unread item at once, so a refusal never waits for a turn
        // that has nothing to wait behind.
        let watch = keys;
        let last_len = RwSignal::new(0usize);
        Effect::new(move |_| {
            let ks = watch.get();
            let before = last_len.get_untracked();
            last_len.set(ks.len());
            if ks.len() < before {
                // A removal addresses items by key (ADR 0031 §4): everything behind the one that
                // left keeps its key and its turn. Only a stage whose item is gone goes blank.
                if on
                    .get_untracked()
                    .is_some_and(|k| !ks.iter().any(|x| *x == k))
                {
                    on.set(None);
                }
                let seen = shown.get_untracked().min(ks.len());
                shown.set(seen);
                return;
            }
            if ks.len() > before && on.get_untracked().is_none() {
                let next = shown.get_untracked();
                if next < ks.len() {
                    shown.set(next + 1);
                    on.set(Some(ks[next].clone()));
                }
            }
        });
    }

    // How many leading items have left the stage, and when each one got there. The tick counter is
    // the clock: arrival and expiry read the same number, so a throttled tab simply runs slower
    // instead of expiring everything at once, and no wall-clock source is needed.
    let retired = RwSignal::new(0usize);
    let stamps = RwSignal::new(HashMap::<String, u64>::new());
    let tick_no = RwSignal::new(0u64);
    if dwelling {
        let stay = every.unwrap_or(4000);
        // Finer than the stay, so an item leaves close to its own deadline instead of up to a
        // whole stay late.
        let tick_ms = (stay / 8).clamp(50, 250);
        let ticks_per_stay = stay.div_ceil(tick_ms).max(1);
        let tick_keys = keys;
        let tick = set_interval_with_handle(
            move || {
                let now = tick_no.get_untracked() + 1;
                tick_no.set(now);
                let ks = tick_keys.get_untracked();
                let gone = expired_prefix(&ks[..], &stamps.get_untracked(), now, ticks_per_stay);
                if gone != retired.get_untracked() {
                    retired.set(gone);
                }
            },
            Duration::from_millis(tick_ms),
        );
        match tick {
            Ok(handle) => {
                let handle = send_wrapper::SendWrapper::new(handle);
                on_cleanup(move || handle.take().clear());
            }
            Err(e) => tracing::warn!("tiles: could not start the dwell timer: {e:?}"),
        }
        // An item's clock starts when it lands in the slot. Items the sender removed take their
        // stamps with them, and the counter never points past the list.
        let watch = keys;
        Effect::new(move |_| {
            let ks = watch.get();
            let now = tick_no.get_untracked();
            stamps.update(|m| {
                let live: HashSet<&String> = ks.iter().collect();
                m.retain(|k, _| live.contains(k));
                for k in ks.iter() {
                    m.entry(k.clone()).or_insert(now);
                }
            });
            if retired.get_untracked() > ks.len() {
                retired.set(ks.len());
            }
        });
    }

    // Many items in one area follow the tail. Which area has a tail is the frame's business (its
    // CSS decides whether it scrolls); following is what `all` means when it does.
    let follows = stage == Stage::All;

    move || -> AnyView {
        // Subscribing to this one slot: writes to other keys do not re-run the container.
        let rows = slot.get();
        let ks = keys.get();
        // A dwell clips the head of the list: those items have had their turn and are off stage,
        // but they are still in the slot (a log bound to the same slot shows every one).
        let gone = if dwelling {
            retired.get().min(rows.len())
        } else {
            0
        };
        let visible: Vec<String> = ks.iter().skip(gone).cloned().collect();
        let lcx = ctx.clone();
        let layout2 = layout.clone();
        let po = parent_owner.clone();
        // Keyed list: the key set is diffed on every slot write, so a new item builds one node and
        // the items above it keep theirs. A row's content is a reactive closure in its own right,
        // so a merge updates that row in place.
        let children = leptos::tachys::view::keyed::keyed(
            visible,
            |k: &String| k.clone(),
            move |_index: usize, key: String| {
                // One owner per tile, under the container's: the keyed diff does not dispose the
                // owners of items that did not change, so their memos outlive an outer re-run.
                let owner = po.with(Owner::new);
                let view = owner.with(|| {
                    let layout = layout2.clone();
                    let k2 = key.clone();
                    // Per-row memo: a slot write recomputes the row this key names, and nothing
                    // else. The lookup is a table read (key → position), not a scan.
                    let memo = Memo::new(move |_| {
                        let pos = *index.get().get(&k2)?;
                        slot.get().get(pos).cloned()
                    });
                    let rctx = lcx.clone();
                    let name = key;
                    move || -> AnyView {
                        let Some(child) = memo.get() else {
                            return div().into_any();
                        };
                        let inner = match layout.select(child.as_ref()) {
                            Some(mut template) => {
                                // The template is a shell: its `children` are replaced by exactly
                                // this item, everything else on it stays (ADR 0020).
                                template.set_children(vec![(*child).clone()]);
                                render_accrete(&rctx, &template)
                            }
                            None => render_accrete(&rctx, child.as_ref()),
                        };
                        if stage != Stage::One {
                            return inner;
                        }
                        // One at a time: the tile needs a handle that can be switched. Enter and
                        // leave are CSS; the class only says which one is current.
                        let n = name.clone();
                        let cls = move || {
                            if staged.get().as_deref() == Some(n.as_str()) {
                                "slide active"
                            } else {
                                "slide"
                            }
                        };
                        div().class(cls).child(inner).into_any()
                    }
                    .into_any()
                });
                (
                    move |_index: usize| {},
                    OwnedView::new_with_owner(view, owner),
                )
            },
        );

        if follows {
            let id_ = id.clone();
            leptos::task::spawn_local(async move {
                // Following the tail = "open AND at the bottom" (PLAN §17): the old test — "within a
                // screen of the bottom" — dragged a reader who scrolled up a little straight back
                // down. The flag is kept by a scroll listener installed once (our own scrolls keep
                // it true); scrolling up stops the follow, returning to the bottom resumes it.
                //
                // Two triggers, both needed: ① this closure re-runs when the slot is written (an
                // appended item); ② the container or a child changes size — a growing message, a
                // module host expanding (ADR 0012), a chart band rendering — and none of those
                // write the slot. So ② comes from a ResizeObserver plus a subtree MutationObserver,
                // and the test is the same "only follow from the bottom".
                //
                // Growth arrives over several frames, so a single jump stops at an intermediate
                // value (measured 20–480px short). A triggered follow therefore re-pins each tick
                // until the extent holds for three ticks or the reader leaves.
                crate::dom::eval(&format!(
                    r#"
                    var e = document.getElementById("{id_extra}");
                    if (e) {{
                        if (!e.dataset.tailInit) {{
                            e.dataset.tailInit = "1";
                            // Last position we pinned: it tells "this scroll was ours" from "the
                            // reader moved" — growth often arrives after the scroll event, so
                            // reading the live value would count our own pin as a leave.
                            var expect = -1;
                            var chasing = false;
                            var maxOf = function () {{ return e.scrollHeight - e.clientHeight; }};
                            var pin = function () {{
                                if (e.dataset.atTail === "0") {{ return; }}
                                var max = maxOf();
                                expect = max;
                                e.scrollTop = max;
                            }};
                            // setTimeout, not rAF: on slow machines rAF was measured at second
                            // scale and the chase fell behind.
                            var chase = function () {{
                                pin();
                                if (chasing) {{ return; }}
                                chasing = true;
                                var last = -1, stable = 0, t0 = Date.now();
                                var tick = function () {{
                                    pin();
                                    var now = maxOf();
                                    stable = (now === last) ? stable + 1 : 0;
                                    last = now;
                                    if (e.dataset.atTail === "0" || stable >= 3 ||
                                        Date.now() - t0 > 2000) {{
                                        chasing = false;
                                        return;
                                    }}
                                    setTimeout(tick, 40);
                                }};
                                setTimeout(tick, 40);
                            }};
                            var atTail = function () {{
                                // At the end, or back at the position we pinned, still counts as
                                // following (a reader who scrolls to the end lands in the first
                                // case); only a clear move away from both means they left.
                                var max = maxOf();
                                if (Math.abs(e.scrollTop - max) <= 2 ||
                                    (expect >= 0 && Math.abs(e.scrollTop - expect) <= 2)) {{
                                    e.dataset.atTail = "1";
                                    return;
                                }}
                                e.dataset.atTail = "0";
                            }};
                            var ro = new ResizeObserver(chase);
                            var observeChildren = function () {{
                                ro.disconnect();
                                ro.observe(e);
                                for (var i = 0; i < e.children.length; i++) {{
                                    ro.observe(e.children[i]);
                                }}
                            }};
                            new MutationObserver(function (recs) {{
                                for (var i = 0; i < recs.length; i++) {{
                                    if (recs[i].type === "childList") {{ observeChildren(); break; }}
                                }}
                                chase();
                            }}).observe(e, {{ childList: true, subtree: true,
                                              characterData: true, attributes: true }});
                            observeChildren();
                            e.addEventListener("scroll", atTail, {{ passive: true }});
                            e.dataset.atTail =
                                (e.scrollHeight - e.scrollTop - e.clientHeight <= 2) ? "1" : "0";
                        }}
                        chase();
                    }}
                    "#,
                    id_extra = id_,
                ));
            });
        }

        div()
            .id(id.as_str())
            .class(css.as_str())
            .child(children)
            .into_any()
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(id: Option<&str>) -> Arc<Accrete> {
        let mut v = json!({
            "type": "text",
            "source": { "default": "x" }
        });
        if let Some(i) = id {
            v.as_object_mut().unwrap().insert("id".into(), i.into());
        }
        Arc::new(serde_json::from_value(v).unwrap())
    }

    #[test]
    fn an_item_with_an_id_is_addressed_by_it() {
        let rows = [row(Some("a")), row(Some("b"))];
        let mut born = HashMap::new();
        assert_eq!(tile_keys(&rows, &mut born), vec!["a", "b"]);
    }

    /// The rule the whole file follows: an item without a key borrows the position it had when it
    /// arrived, and that number is never recomputed. Removing the head must not rename the rest.
    #[test]
    fn a_positional_key_is_read_once_at_insertion() {
        let a = row(None);
        let b = row(None);
        let c = row(None);
        let mut born = HashMap::new();

        assert_eq!(tile_keys(&[a.clone(), b.clone()], &mut born), vec!["#0", "#1"]);

        // `a` is removed: `b` is now first in the list, but it is still the item that arrived
        // second, and it keeps the key it was given.
        assert_eq!(tile_keys(&[b.clone()], &mut born), vec!["#1"]);

        // A new item entering at the front is read at the front, once.
        assert_eq!(
            tile_keys(&[c.clone(), b.clone()], &mut born),
            vec!["#0", "#1"],
            "the newcomer arrived at 0; `b` keeps its own key"
        );
        assert_eq!(tile_keys(&[b.clone()], &mut born), vec!["#1"]);
    }

    /// A key must not outlive its item: a later item that happens to land in the same allocation
    /// would otherwise inherit a dead key and look like the item that left.
    #[test]
    fn a_departed_item_is_forgotten() {
        let a = row(None);
        let mut born = HashMap::new();
        assert_eq!(tile_keys(&[a.clone()], &mut born), vec!["#0"]);
        drop(a);
        assert!(tile_keys(&[], &mut born).is_empty());
        assert!(born.is_empty(), "the map holds no dead arrivals");
    }

    #[test]
    fn positions_follow_the_list_not_the_key() {
        let pos = positions(&["b".to_string(), "a".to_string()]);
        assert_eq!(pos["b"], 0);
        assert_eq!(pos["a"], 1);
    }

    #[test]
    fn duplicate_keys_keep_the_first_hit() {
        // Two rows with one id: the first wins, as a linear `find` did.
        assert_eq!(positions(&["dup".into(), "dup".into()])["dup"], 0);
    }

    #[test]
    fn a_step_slot_names_a_key() {
        assert_eq!(staged_key(Some(json!("home"))), Some("home".into()));
        // The emit wrapper every local write takes (ADR 0010): the payload is the key.
        assert_eq!(
            staged_key(Some(json!({ "dest": "page", "id": "tabs", "data": "docs" }))),
            Some("docs".into())
        );
        // Anything else names nothing rather than something arbitrary.
        assert_eq!(staged_key(Some(json!({ "dest": "page" }))), None);
        assert_eq!(staged_key(Some(json!(2))), None);
        assert_eq!(staged_key(None), None);
    }

    /// A dwell retires the head of the list and only the head: items arrive in order and live
    /// equally long, so one that outlived its stay sits in front of every one that has not.
    #[test]
    fn a_dwell_retires_the_head_of_the_list() {
        let keys: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let mut stamps = HashMap::new();
        stamps.insert("a".to_string(), 0u64);
        stamps.insert("b".to_string(), 0u64);
        stamps.insert("c".to_string(), 5u64);

        // Nothing has served its full stay yet (8 ticks), so nothing has left.
        assert_eq!(expired_prefix(&keys, &stamps, 7, 8), 0);
        // At 8 the first two are done and the third, which arrived at 5, is not — and the walk
        // stops there even though the one behind it will also be done later.
        assert_eq!(expired_prefix(&keys, &stamps, 8, 8), 2);
        // An item with no stamp (just arrived) holds the stage: the prefix cannot jump over it.
        stamps.remove("c");
        assert_eq!(expired_prefix(&keys, &stamps, 20, 8), 2);
        // Everything retired is reported as such; the caller clamps it to the list length.
        assert_eq!(expired_prefix(&keys, &stamps, 100, 8), 2);
    }
}