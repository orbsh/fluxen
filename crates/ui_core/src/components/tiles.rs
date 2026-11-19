//! `tiles` — one container for a slot's items, cut into blocks (ADR 0031 §4).
//!
//! Two axes, not two components: `window` says how many items are on stage at once (absent lays
//! every one out, `{size: 1}` gives the area to a single item, `{size: N}` is a moving window of N
//! consecutive items), and `step` says what advances it (a clock the node owns, or a slot the world
//! writes). `rack` and `pages` were this node with different steppers, so the machinery below is
//! theirs: the keyed list, the position index, the rotation, the follow-the-tail scroll.
//!
//! One rule runs through all of it: **every item has a key**, read once when the item arrives
//! (ADR 0031 §4, "Every item has a key"). Selection, removal and replacement address items by
//! that key, never by a position recomputed from the current list.

use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{self, use_common_css, use_source_id};
use accrete::classify::Classify;
use accrete::{Accrete, AccreteOps, Align, Bind, Column, Header, Scope, Step, Tiles};
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

/// Is position `at` inside the window that starts at `start` and is `width` items wide? The one
/// rule a window has: N CONSECUTIVE items, from the cursor. Past the end of the list the window is
/// simply shorter — there is nothing to pad it with.
fn in_window(at: usize, start: usize, width: usize) -> bool {
    at >= start && at < start.saturating_add(width)
}

/// The field a column projects: the record's child whose `selector` is the key. This is ADR 0020's
/// rule reused — a selector already names a child among its siblings, and a record's fields are
/// exactly that, so a column can address one without the vocabulary growing a second key.
///
/// The first hit wins, the same rule `positions` follows: two fields with one name are one field, and
/// the first is the one the frame wrote first.
fn field<'a>(record: &'a Accrete, key: &str) -> Option<&'a Accrete> {
    record
        .borrow_children()?
        .iter()
        .find(|c| c.get_selector().as_deref() == Some(key))
}

/// The records a field holds, when the field holds a LIST of them (ADR 0031 §4, "Spans are not a
/// feature; hierarchy is"). All children must be records: a mix of records and values is a frame
/// mistake, and drawing it as the plain cell it also is keeps the mistake visible instead of
/// silently swallowing the values.
fn child_records(field: &Accrete) -> Vec<&Accrete> {
    let Some(children) = field.borrow_children() else {
        return Vec::new();
    };
    if !children.is_empty() && children.iter().all(|c| matches!(c, Accrete::record(_))) {
        children.iter().collect()
    } else {
        Vec::new()
    }
}

/// A column key's two halves for a two-row header: `price/retail` → (`price`, `retail`). A key with
/// no `/` is its own group, which is how a field with no parent reads in a real header too.
fn key_parts(key: &str) -> (&str, &str) {
    match key.rfind('/') {
        Some(at) => (&key[..at], &key[at + 1..]),
        None => (key, key),
    }
}

/// The group row of a two-row header: contiguous runs of columns sharing a group, and how many
/// columns each covers. Runs rather than a map, because the header must read left to right in column
/// order — the same order the cells below it sit in.
fn head_groups(columns: &[Column]) -> Vec<(String, usize)> {
    let mut groups: Vec<(String, usize)> = Vec::new();
    for column in columns {
        let (group, _) = key_parts(&column.key);
        match groups.last_mut() {
            Some((last, span)) if last == group => *span += 1,
            _ => groups.push((group.to_string(), 1)),
        }
    }
    groups
}

/// One cell: the column's hooks (its `selectors`), the projection's key for CSS to reach, and the
/// field drawn inside as it is. No field = an empty cell, because a missing value is a hole in the
/// record and drawing nothing is what a hole looks like.
fn cell_view(ctx: &Ctx, field: Option<&Accrete>, column: &Column) -> AnyView {
    let mut class = String::from("tiles-cell");
    for selector in column.selectors.iter().flatten() {
        class.push(' ');
        class.push_str(selector);
    }
    let style = match column.align.unwrap_or_default() {
        Align::Start => String::new(),
        Align::Center => "justify-self: center;".to_string(),
        Align::End => "justify-self: end;".to_string(),
    };
    let body = match field {
        Some(f) => render_accrete(ctx, f),
        None => ().into_any(),
    };
    div()
        .class(class)
        .attr("data-key", column.key.clone())
        .style(style)
        .child(body)
        .into_any()
}

/// A row: the cells of one record. `state` is the stage (`active` / `off`) when one item holds the
/// area, and `depth` is how far down the child rows this one sits — the indentation is CSS on
/// `.tiles-child`, the model never says how deep anything looks.
fn row_view(cells: Vec<AnyView>, state: &str, depth: usize) -> AnyView {
    let mut class = String::from("tiles-row");
    if depth > 0 {
        class.push_str(" tiles-child");
    }
    if !state.is_empty() {
        class.push(' ');
        class.push_str(state);
    }
    // The level travels as a number, not as markup: CSS decides what a level looks like.
    let style = if depth > 0 {
        format!("--depth: {depth};")
    } else {
        String::new()
    };
    div().class(class).style(style).child(cells).into_any()
}

/// A record as rows: its own row, then the rows of every field that holds records, recursively
/// (ADR 0031 §4). The child rows are siblings of their parent, not children of it — a grid row
/// cannot contain a grid row without inventing the nested column set a span would have faked.
fn record_rows(
    ctx: &Ctx,
    record: &Accrete,
    columns: &[Column],
    state: &str,
    depth: usize,
) -> Vec<AnyView> {
    let mut nested: Vec<&Accrete> = Vec::new();
    let cells: Vec<AnyView> = columns
        .iter()
        .map(|column| {
            let found = field(record, &column.key);
            match found.map(child_records) {
                Some(children) if !children.is_empty() => {
                    nested.extend(children);
                    cell_view(ctx, None, column)
                }
                _ => cell_view(ctx, found, column),
            }
        })
        .collect();
    let mut rows = vec![row_view(cells, state, depth)];
    for child in nested {
        rows.extend(record_rows(ctx, child, columns, state, depth + 1));
    }
    rows
}

/// The header: one row of cells when `rows` is 1, a group row over a leaf row when it is 2. Every
/// cell carries `header.selector` — that is the whole mechanism, since a header cell is not an
/// element a frame can name (ADR 0031 §4).
fn head_view(header: &Header, columns: &[Column]) -> AnyView {
    let mut cells: Vec<AnyView> = Vec::new();
    if header.rows >= 2 {
        for (group, span) in head_groups(columns) {
            // A group covering several columns is one cell that covers them: the grid placement is
            // the renderer expressing the hierarchy the keys already have, not a span the frame can
            // ask for.
            let style = if span > 1 {
                format!("grid-column: span {span};")
            } else {
                String::new()
            };
            cells.push(
                div()
                    .class(format!("tiles-hcell group {}", header.selector))
                    .style(style)
                    .attr("data-group", group.clone())
                    .child(group)
                    .into_any(),
            );
        }
    }
    for column in columns {
        let (_, leaf) = key_parts(&column.key);
        let title = column.title.clone().unwrap_or_else(|| leaf.to_string());
        cells.push(
            div()
                .class(format!("tiles-hcell leaf {}", header.selector))
                .attr("data-key", column.key.clone())
                .child(title)
                .into_any(),
        );
    }
    div().class("tiles-head").child(cells).into_any()
}

/// A slot's items, cut into blocks (ADR 0031 §4).
///
/// No `window` lays every item out. `window: {size: 1}` gives the area to a single item, chosen by
/// `step` — a slot that names its key, or a clock that rotates; with no stepper the newest item
/// holds the area, because a frame that asks for "one at a time" must not show nothing for want of
/// a cursor. `window: {size: N}` is the same rule with N consecutive items starting at the cursor:
/// the marquee, the page, and a page of rows are one thing.
pub fn tiles_(accrete: Tiles, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    // Rows get their owner from here: the subscriptions of a row outlive the render pass that
    // built it, so the node's own scope (not the caller's) is what they hang on.
    #[allow(clippy::expect_used)]
    let parent_owner = Owner::current().expect("tiles_ requires a reactive owner");

    // The window is the number of items on stage at once; absent = all of them. A stepper only
    // exists to move a window, so a `step` with no `window` is refused at load (stage/proto.rs).
    let window = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.window)
        .map(|w| w.size);
    let step = accrete.attrs.as_ref().and_then(|a| a.step.clone());
    // `columns` decides the data shape (ADR 0031 §4): present means each item is a RECORD and the
    // node is a grid; absent means each item is drawn as it is. An empty list of columns says
    // nothing, so it is the same as absent — a grid with no columns has no cells to place.
    let columns = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.columns.clone())
        .filter(|c| !c.is_empty());
    let header = accrete.attrs.as_ref().and_then(|a| a.header.clone());

    let mut css = vec!["tiles", "f"];
    if window.is_some() {
        // One class says the container is a stage, one custom property says how wide it is: a
        // size-N hook rather than a class per answer, because N is a number and CSS cannot count
        // items. `size: 1` keeps the stack (`.rot`) for the non-grid case — one item in the area,
        // the others behind it — which is what "one at a time" always looked like.
        css.push("window");
        if window == Some(1) && columns.is_none() {
            css.push("rot");
        }
    }
    if columns.is_some() {
        css.push("cols");
    }
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    // Every row of a grid is its own track list, all read from one custom property: the header and
    // the body cannot drift apart, and a frame still overrides any of it with CSS.
    let mut decls: Vec<String> = Vec::new();
    if let Some(size) = window {
        decls.push(format!("--tile-window: {size};"));
    }
    if let Some(cols) = columns.as_ref() {
        let tracks: Vec<String> = cols
            .iter()
            .map(|c| c.width.clone().unwrap_or_else(|| "minmax(0, 1fr)".into()))
            .collect();
        decls.push(format!("--tile-cols: {};", tracks.join(" ")));
    }
    let style = if decls.is_empty() {
        None
    } else {
        Some(decls.join(" "))
    };

    let layout: Layout = accrete.children.clone().unwrap_or_default().into();
    // No slot = nothing to tile. A list channel without an address has no items, not one.
    let Some(source) = use_source_id(&accrete).cloned() else {
        return div()
            .id(id.as_str())
            .class(css.as_str())
            .style(style.unwrap_or_default())
            .into_any();
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

    // The cursor is a KEY, never a position: an item removed ahead of it must not slide the window
    // onto its neighbour. It names the FIRST item of the window; with `size: 1` that is the whole
    // window, so this is the single-item stage in the only case that ever existed.
    let on = RwSignal::new(None::<String>);
    // The window's first key, as the render pass asks it. A slot names it (`step: {slot}`), a
    // clock names the next one (`step: {timer}`), and with no stepper the window follows the tail
    // — the last N items, which for `size: 1` is the newest one that used to hold the area.
    let stepped = step.is_some() && window.is_some();
    let width = window.unwrap_or(0);
    let first: Memo<Option<String>> = Memo::new(move |_| {
        if stepped {
            return on.get();
        }
        let ks = keys.get();
        ks.get(ks.len().saturating_sub(width)).cloned()
    });
    // The window as positions: `start .. start + width`. Empty while a step slot has named no key
    // yet — a page before the first click shows nothing, not something arbitrary.
    let start: Memo<Option<usize>> = Memo::new(move |_| {
        let key = first.get()?;
        index.get().get(&key).copied()
    });
    // Is this key inside the window? No window at all = everything is drawn, so the answer is
    // yes; a window whose start names no item = nothing is.
    let in_stage = move |key: &str| -> bool {
        match (start.get(), window) {
            (Some(s), Some(n)) => index
                .get()
                .get(key)
                .is_some_and(|at| in_window(*at, s, n)),
            (None, Some(_)) => false,
            (_, None) => true,
        }
    };

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
    let rotating = every.is_some() && window.is_some();

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
                if next + width <= ks.len() {
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
                if next + width <= ks.len() {
                    shown.set(next + 1);
                    on.set(Some(ks[next].clone()));
                }
            }
        });
    }

    // Many items in one area follow the tail. Which area has a tail is the frame's business (its
    // CSS decides whether it scrolls); following is what "no window" means when it does.
    let follows = window.is_none();

    move || -> AnyView {
        // Subscribing to this one slot: writes to other keys do not re-run the container.
        let _rows = slot.get();
        let ks = keys.get();
        // Every item stays in the list; the window only decides which of them are ON it, so a
        // paged table keeps the state of the pages around it (that is why `pages` kept every row
        // mounted rather than swapping one in).
        let visible: Vec<String> = ks.iter().cloned().collect();
        let lcx = ctx.clone();
        let layout2 = layout.clone();
        let columns2 = columns.clone();
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
                    let columns = columns2.clone();
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
                        // A grid projects each item's fields into cells; the key the item arrived
                        // with stays the identity of ALL of its rows, so stepping and selection
                        // address the same thing they addressed before `columns` existed.
                        if let Some(columns) = &columns {
                            let state = if in_stage(&name) { "" } else { "off" };
                            return record_rows(&rctx, child.as_ref(), columns, state, 0)
                                .into_any();
                        }
                        let inner = match layout.select(child.as_ref()) {
                            Some(mut template) => {
                                // The template is a shell: its `children` are replaced by exactly
                                // this item, everything else on it stays (ADR 0020).
                                template.set_children(vec![(*child).clone()]);
                                render_accrete(&rctx, &template)
                            }
                            None => render_accrete(&rctx, child.as_ref()),
                        };
                        if window.is_none() {
                            return inner;
                        }
                        // On stage: the tile needs a handle that can be switched. Which one is
                        // current is a class; what "current" looks like is CSS. Size 1 stacks
                        // them and fades (`.rot`); a wider window just hides the others.
                        let n = name.clone();
                        let cls = move || {
                            if in_stage(&n) {
                                "slide active"
                            } else {
                                "slide off"
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

        // The header sits above the rows and outside the keyed list: it is one row for the whole grid,
        // not one per item, so it must not be rebuilt when an item arrives.
        let head = header
            .as_ref()
            .zip(columns.as_ref())
            .map(|(h, c)| head_view(h, c));

        div()
            .id(id.as_str())
            .class(css.as_str())
            .style(style.clone().unwrap_or_default())
            .child(head)
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

    fn node(v: Value) -> Accrete {
        serde_json::from_value(v).unwrap()
    }

    /// A record is a list of named fields: the name is the child's `selector`, so a column projects
    /// by name and reordering the children cannot move a value into another column. A field whose
    /// children are ALL records holds a list of them (the nested rows); a field that mixes records
    /// and values stays a plain cell, because drawing half of it as hierarchy would hide the
    /// frame's mistake.
    #[test]
    fn a_field_holds_a_value_or_a_list_of_records() {
        let rec = node(json!({
            "type": "record",
            "children": [
                {"type": "text", "attrs": {"selector": "name"},
                 "source": {"default": "x"}},
                {"type": "frame", "attrs": {"selector": "options"},
                 "children": [
                    {"type": "record", "id": "a"},
                    {"type": "record", "id": "b"}
                 ]},
                {"type": "frame", "attrs": {"selector": "mixed"},
                 "children": [
                    {"type": "record", "id": "c"},
                    {"type": "text", "attrs": {"selector": "name"}}
                 ]}
            ]
        }));

        // A field with a value: the cell it lands in is that node, and it nests nothing.
        let name = field(&rec, "name").expect("name");
        assert!(child_records(name).is_empty());

        // All records = a list: those become the child rows under this row.
        let options = field(&rec, "options").expect("options");
        assert_eq!(child_records(options).len(), 2);

        // Records and values together = not a hierarchy.
        let mixed = field(&rec, "mixed").expect("mixed");
        assert!(
            child_records(mixed).is_empty(),
            "a mix is not a list of records"
        );

        // A key no field answers is a hole in the record, not an error.
        assert!(field(&rec, "price").is_none());

        // Two fields with one name are one field, and the first is the one the frame wrote first
        // (the same rule `positions` follows).
        let dup = node(json!({
            "type": "record",
            "children": [
                {"type": "text", "attrs": {"selector": "n"}, "source": {"default": "first"}},
                {"type": "text", "attrs": {"selector": "n"}, "source": {"default": "second"}}
            ]
        }));
        assert_eq!(
            field(&dup, "n")
                .and_then(|f| f.get_source())
                .and_then(|b| b.default.clone()),
            Some(json!("first"))
        );

        // An empty field is not a list either: `is_empty` would otherwise say "all records" of
        // nothing and the row would vanish.
        let empty = node(json!({"type": "frame", "attrs": {"selector": "e"}, "children": []}));
        assert!(child_records(&empty).is_empty());
    }

    /// A record is the item, so the key a grid addresses it by is the record's OWN id — never one
    /// of its fields' selectors, and never the position it happens to sit at.
    #[test]
    fn a_record_is_keyed_by_its_own_id() {
        let named = |id: &str| Arc::new(node(json!({"type": "record", "id": id, "children": []})));
        let anon = || Arc::new(node(json!({"type": "record", "children": []})));
        let (a, b) = (named("853736"), named("56459"));

        let mut born = HashMap::new();
        assert_eq!(
            tile_keys(&[a.clone(), b.clone()], &mut born),
            vec!["853736", "56459"]
        );

        // Removing the head renames nothing behind it.
        assert_eq!(tile_keys(&[b.clone()], &mut born), vec!["56459"]);

        // A record that names nothing borrows its arrival position, once.
        let x = anon();
        assert_eq!(tile_keys(&[x.clone()], &mut born), vec!["#0"]);
        assert_eq!(tile_keys(&[b], &mut born), vec!["56459"]);
    }

    /// A two-row header reads its groups off the column keys: split on `/`, everything before the
    /// last one is the group, the rest is the leaf. A key with no `/` is its own group. Runs are
    /// contiguous because the header has to read left to right in column order.
    #[test]
    fn a_two_row_header_groups_columns_by_key_prefix() {
        let col = |key: &str| Column {
            key: key.into(),
            ..Column::default()
        };
        let cols = [
            col("qty"),
            col("price/retail"),
            col("price/wholesale"),
            col("name"),
        ];
        assert_eq!(
            head_groups(&cols[..]),
            vec![
                ("qty".to_string(), 1),
                ("price".to_string(), 2),
                ("name".to_string(), 1),
            ]
        );

        // Only the last slash splits, so a deeper path groups under its parent path — and the leaf
        // is what the cell title falls back to.
        assert_eq!(key_parts("price"), ("price", "price"));
        assert_eq!(key_parts("price/retail"), ("price", "retail"));
        assert_eq!(key_parts("price/retail/list"), ("price/retail", "list"));
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

    /// A window is N consecutive positions from the cursor — one is a page, forty is a paged table,
    /// and there is no second rule. Past the end it is simply shorter.
    #[test]
    fn a_window_is_n_consecutive_positions() {
        // `size: 1`: only the cursor's item is on stage — the old `stage: one`, unchanged.
        assert!(in_window(3, 3, 1));
        assert!(!in_window(2, 3, 1));
        assert!(!in_window(4, 3, 1));

        // `size: 3`: the cursor and the two behind it, and nothing before the cursor.
        let on: Vec<usize> = (0..6).filter(|at| in_window(*at, 2, 3)).collect();
        assert_eq!(on, vec![2, 3, 4]);

        // A cursor near the tail gives a shorter window rather than wrapping to the head.
        assert!(in_window(5, 4, 3));
        assert!(!in_window(1, 4, 3));
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
}