//! Frame: the structural container, and the only shape of a data area (ADR 0027).
//!
//! There used to be two components here — `case` (draws children) and `placeholder` (when bound to a `Source` it
//! draws the tree in the slot, otherwise draws children). They are not two things: the implementation of `placeholder` is
//! a superset of `case`, and the only difference is "whether a bound source reads the slot". Splitting them into two components amounts to asking
//! the author to predict, while writing the layout, whether this block will later be filled with data — and static today, dynamic tomorrow, the type would have to
//! change with it. A binding can be added later; a type cannot.
//!
//! So only `frame` is left:
//!
//!   bind.value.slot   slot has a value → draw the tree in the slot (with one fade in and out),
//!                     slot empty → draw children (the default content)
//!   no source         draw children
//!
//! ADR 0034 pulls both ends of dragging in here too: `attrs.draggable` is the source, `attrs.accept` + `sink`
//! is the drop target. There is only one reason — the cargo is a subtree, and an attribute cannot hold a subtree; the emit must happen on the
//! box the cargo lands in, and that box is here. The three types `group`/`grab`/`dock` disappeared with it.
//!
//! Gestures and flight state come from `Ctx.drag`: a page-level session, only one copy (source and drop target sit apart with other nodes between them, and a local
//! signal cannot reach across). Each frame keeping its own copy is the opposite of this design.
//!
//! Both routes, `attrs.grid` and `attrs.class`, are kept: with grid only `grid` goes in, otherwise only `flex`,
//! because within one layer `.flex` and `.grid` let whichever comes later win, and having both at once suppresses `display:grid`
//! (`attrs.grid` has never taken effect since birth, and it was only left unhit because nobody used grid).

use crate::Ctx;
use crate::rough;
use crate::ctx::{render_accrete, render_children};
use crate::hooks::use_common_css;
use accrete::{Accept, Accrete, AccreteOps, Bind, Frame};
use leptos::attr::any_attribute::AnyAttribute;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::{Map, Value};

/// The slot name (`bind.value.slot`), `None` when there is no binding.
fn source_of(accrete: &Frame) -> Option<String> {
    accrete.get_source().and_then(|b| b.slot.clone())
}

/// The cargo's label: the `draggable.tag` the source frame declares itself. What the session holds is that frame node, so the label
/// exists only once and cannot be parted from the cargo (ADR 0034).
fn cargo_tag(cargo: &Accrete) -> Option<&str> {
    match cargo {
        Accrete::frame(f) => f.attrs.as_ref()?.draggable.as_ref()?.tag.as_deref(),
        _ => None,
    }
}

/// Whether this drop target takes this cargo: no tag in `accept` = take all; a tag makes it a closed-set comparison, and cargo
/// with no tag matches no target that declares one (a mismatch is a mismatch: no emit, no highlight).
fn accepts(accept: &Accept, cargo: &Accrete) -> bool {
    match &accept.tag {
        None => true,
        Some(want) => cargo_tag(cargo) == Some(want.as_str()),
    }
}

/// What should come comes, what should not takes a placeholder (`()` produces no DOM attribute at all). Source and drop target are independent: a frame
/// may only drag, only take, or be both source and target.
fn drag_attrs(
    ctx: &Ctx,
    cargo: &Accrete,
    draggable: &Option<accrete::Draggable>,
    accept: &Option<Accept>,
    sink: &Option<Bind>,
    id: &str,
    over: RwSignal<bool>,
) -> (
    AnyAttribute,
    AnyAttribute,
    AnyAttribute,
    AnyAttribute,
    AnyAttribute,
    AnyAttribute,
) {
    // Source: `dragstart` only writes the cargo into the page-level session and reports nothing — the uplink happens at the drop target.
    let label = draggable
        .as_ref()
        .and_then(|d| d.tag.clone())
        .unwrap_or_else(|| "item".into());
    let on_start = {
        let (ctx, cargo) = (ctx.clone(), cargo.clone());
        leptos::ev::on(leptos::ev::dragstart, move |ev: web_sys::DragEvent| {
            // Firefox does not accept the drag unless something is written into dataTransfer; the cargo itself travels in the session,
            // and is not stuffed into dataTransfer (only a readable label goes there).
            if let Some(dt) = ev.data_transfer() {
                dt.set_effect_allowed("move");
                let _ = dt.set_data("text/plain", &label);
            }
            ctx.drag.set(Some(cargo.clone()));
        })
    };
    let on_end = {
        let ctx = ctx.clone();
        leptos::ev::on(leptos::ev::dragend, move |_ev: web_sys::DragEvent| {
            ctx.drag.set(None)
        })
    };

    // Drop target: both dragenter and dragover must preventDefault — block only one and the browser still treats
    // this release as "cannot drop", and drop never fires at all. The two must hang on different event descriptors (the wrapper type
    // follows the event), while the test itself is one block, so the two copies below differ only in the event name.
    let on_enter = {
        let (ctx, accept) = (ctx.clone(), accept.clone());
        leptos::ev::on(leptos::ev::dragenter, move |ev: web_sys::DragEvent| {
            let ok = ctx
                .drag
                .get_untracked()
                .is_some_and(|c| accept.as_ref().is_some_and(|a| accepts(a, &c)));
            if ok {
                ev.prevent_default();
                over.set(true);
            }
        })
    };
    let on_over = {
        let (ctx, accept) = (ctx.clone(), accept.clone());
        leptos::ev::on(leptos::ev::dragover, move |ev: web_sys::DragEvent| {
            let ok = ctx
                .drag
                .get_untracked()
                .is_some_and(|c| accept.as_ref().is_some_and(|a| accepts(a, &c)));
            if ok {
                ev.prevent_default();
                over.set(true);
            }
        })
    };
    let on_leave = leptos::ev::on(leptos::ev::dragleave, move |_ev: web_sys::DragEvent| {
        over.set(false)
    });
    let on_drop = {
        let (ctx, accept, sink, id) = (ctx.clone(), accept.clone(), sink.clone(), id.to_string());
        leptos::ev::on(leptos::ev::drop, move |ev: web_sys::DragEvent| {
            ev.prevent_default();
            over.set(false);
            let Some(cargo) = ctx.drag.get_untracked() else {
                return;
            };
            if !accept.as_ref().is_some_and(|a| accepts(a, &cargo)) {
                return;
            }
            let mut data = Map::new();
            // An empty id counts as no identity (the same convention as `node_id`), so it is normalised here, at the exit point.
            if let Some(id) = cargo
                .get_id()
                .as_deref()
                .filter(|s| !s.is_empty())
            {
                data.insert("id".into(), Value::String(id.to_string()));
            }
            if let Some(tag) = cargo_tag(&cargo) {
                data.insert("tag".into(), Value::String(tag.to_string()));
            }
            if let Some(sink) = &sink {
                // Frame: `sink` = the name the drop target declares, `id` = the target itself, `data` = {id?, tag?}
                // (empty means absent, ADR 0012).
                ctx.emit(sink, Some(id.clone()), Value::Object(data));
            }
            // On the normal path the source's dragend clears it; clear once more here, so that nobody leaves the session
            // in "in flight" state after a drop.
            ctx.drag.set(None);
        })
    };

    // What should come comes, what should not takes a placeholder (`()` produces no DOM attribute at all).
    let attr = |on: bool, a: AnyAttribute| if on { a } else { ().into_any_attr() };
    (
        attr(draggable.is_some(), on_start.into_any_attr()),
        attr(draggable.is_some(), on_end.into_any_attr()),
        attr(accept.is_some(), on_enter.into_any_attr()),
        attr(accept.is_some(), on_over.into_any_attr()),
        attr(accept.is_some(), on_leave.into_any_attr()),
        attr(
            accept.is_some() && sink.is_some(),
            on_drop.into_any_attr(),
        ),
    )
}

pub fn frame_(accrete: Frame, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    let attrs = accrete.attrs.clone().unwrap_or_default();
    // HTML5 only starts a drag when the element itself carries `draggable="true"`; a dragstart handler alone is not enough
    // (the event would never come, and dragging on the page is the browser selecting text). This attribute is written only with `attrs.draggable`.
    let drag_flag = attrs.draggable.is_some().then_some("true");
    let mut css = vec!["frame"];
    if let Some(id) = &accrete.id {
        css.push(id.as_str());
    }

    let mut style = String::new();
    let mut use_grid = false;
    if let Some(g) = &attrs.grid {
        use_grid = true;
        css.push("grid");
        style = g
            .iter()
            .map(|(k, v)| format!("{}: {};", k, v.as_str().unwrap_or("")))
            .collect::<Vec<String>>()
            .join("\n");
    }
    if !use_grid {
        css.push("flex");
    }
    // The inline style of `group`: written into style as it stands, coexisting with the grid track declaration (the latter written first).
    if let Some(s) = &attrs.style {
        for (k, v) in s {
            style.push_str(&format!("{k}: {v};\n"));
        }
    }
    let is_target = attrs.accept.is_some() && accrete.sink.is_some();
    if is_target {
        css.push("drop");
    }
    use_common_css(&mut css, &accrete);
    let base = css.join(" ");

    let over = RwSignal::new(false);
    // Only a drop target subscribes to the session: the class closure of a non-target does not even evaluate `ctx.drag.get()`, so it does not subscribe
    // (short-circuit evaluation: when `watch` is false that signal is in nobody's dependencies).
    let class = {
        let (ctx, base) = (ctx.clone(), base.clone());
        move || {
            let mut cls = base.clone();
            if is_target && ctx.drag.get().is_some() {
                cls.push_str(" drop-armed");
            }
            if over.get() {
                cls.push_str(" drop-over");
            }
            cls
        }
    };

    let cargo = Accrete::frame(accrete.clone());
    // All three channel declarations move into the closure: attributes must be rebuilt on every rebuild (a closure cannot be cloned), and the frame itself
    // is gone after the return.
    let sink = accrete.sink.clone();
    let draggable = attrs.draggable.clone();
    let accept = attrs.accept.clone();
    let drag = move |ctx: &Ctx| drag_attrs(ctx, &cargo, &draggable, &accept, &sink, &id, over);

    let children = accrete.children.clone().unwrap_or_default();
    // The DOM id is the marker contract: a reserved-id node (`reflux:path` and
    // friends) is read back with `getElementById`, and `placeholder` carried the
    // id before the merge (ADR 0027 keeps its behaviour). The class token above
    // stays — it is what CSS addressed — but a class is not addressable by id.
    let dom_id = accrete.id.clone();
    // The sketch skin (ADR 0038 §4): only a frame that asked for a border has this layer,
    // and only while `config.skin = sketch` — in normal mode there is no element and no
    // observer.
    let sketch = {
        let class = attrs.class.clone();
        let id = accrete.id.clone();
        move |c: &Ctx| rough::border_layer(c, class.as_ref(), id.as_deref())
    };
    let Some(source) = source_of(&accrete) else {
        let view = div()
            .attr("class", class)
            .attr("draggable", drag_flag)
            .style(style)
            .add_any_attr(drag(&ctx))
            .child(render_children(&ctx, &children))
            .child(sketch(&ctx));
        return match dom_id {
            Some(id) => view.id(id).into_any(),
            None => view.into_any(),
        };
    };

    // Data area: the whole container is a mirror of the slot. `is-fade-in-and-out` is a one-shot
    // animation in core.css — the element is rebuilt with the slot value, the animation replays itself, and no JS adds or removes classes.
    let inner_ctx = ctx.clone();
    move || -> AnyView {
        match inner_ctx.slot_for_data(&source).get() {
            Some(data) => {
                let view = div()
                    .attr("class", format!("{} is-fade-in-and-out", base))
                    .attr("draggable", drag_flag)
                    .style(style.clone())
                    .add_any_attr(drag(&inner_ctx))
                    .child(render_accrete(&inner_ctx, &data))
                    .child(sketch(&inner_ctx));
                match dom_id.clone() {
                    Some(id) => view.id(id).into_any(),
                    None => view.into_any(),
                }
            }
            None => {
                let view = div()
                    .attr("class", class.clone())
                    .attr("draggable", drag_flag)
                    .style(style.clone())
                    .add_any_attr(drag(&inner_ctx))
                    .child(render_children(&inner_ctx, &children))
                    .child(sketch(&inner_ctx));
                match dom_id.clone() {
                    Some(id) => view.id(id).into_any(),
                    None => view.into_any(),
                }
            }
        }
    }
    .into_any()
}