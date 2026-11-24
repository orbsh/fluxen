//! Panel: a "content area that appears/disappears" (`accrete::Panel`).
//!
//! The frame has one name only, `panel`, and two axes say what it is (ADR 0030):
//!
//! ```text
//!   place: inline    takes layout, pushes the content below down when opened    state: toggle   (formerly fold)
//!   place: anchor    floats beside the trigger, the page does not move    state: toggle   (formerly popup)
//!   place: viewport  covers the viewport (mask + flush to an edge or centered)    state: bound    (formerly layer)
//!   place: viewport  as above, shown/hidden by a local switch    state: toggle   (drawer)
//! ```
//!
//! This used to be three files (`fold.rs` / `popup.rs` / `layer.rs`), each shape with its own geometry and state machine,
//! and a rewrite measured no gain. So the three implementations moved into this file as they were, as private functions beside `drawer_`:
//!
//! ```text
//!   inline_   place: inline   state: toggle   —— one grid row: icon + header + body
//!   anchor_   place: anchor   state: toggle   —— relatively positioned trigger + absolutely positioned float
//!   overlay_  place: viewport state: bound    —— inline geometry + "a non-empty bound slot means shown"
//!   drawer_   place: viewport state: toggle   —— edge-flush sidebar + mask click / Esc
//! ```
//!
//! Function names take the words of the axes (inline / anchor / overlay), not the retired node names (fold / popup / layer):
//! in this file one word means one thing only, and `place` says precisely where it sits. CSS class names (`.popup`, `.layer`,
//! `.panel-trigger` …) are unchanged: those are words of the page stylesheet, not words of the model.
//!
//! The child convention splits by `place`: an `inline` panel's `children` are all body (the whole row is the switch); in the other three cells
//! `children[0]` is the trigger and `children[1]` is the body (`state: bound` may omit the trigger, since shown/hidden
//! is decided by the slot). A missing body is a frame error, not "half a panel can be rendered".

use crate::Ctx;
use crate::components::canvas::{HIDDEN_STYLE, payload_is_empty};
use crate::ctx::{render_accrete, render_children};
use crate::hooks::{use_common_css, use_default};
use crate::rough;
use accrete::{AccreteOps, Direction, Panel, PanelAttr, PanelPlace, PanelState, Source};
use leptos::ev::{click, keydown};
use leptos::html::*;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

pub fn panel_(accrete: Panel, ctx: &Ctx, id: String) -> AnyView {
    let attrs = accrete.attrs.clone().unwrap_or_default();
    match (attrs.place, attrs.state) {
        (PanelPlace::Inline, state) => {
            if state == PanelState::Bound {
                tracing::warn!(
                    "panel {id}: inline + bound 没有意义 —— 撑开布局的容器没有\"空即无\"这条规则，按 toggle 画"
                );
            }
            inline_(accrete, ctx, id, attrs)
        }
        // A float hugging the trigger: shown/hidden is local state to begin with, so the bound cell has no corresponding implementation.
        (PanelPlace::Anchor, state) => {
            if state == PanelState::Bound {
                tracing::warn!("panel {id}: anchor + bound 按 toggle 画（浮层只认本地开关）");
            }
            anchor_(accrete, ctx, id, attrs)
        }
        (PanelPlace::Viewport, PanelState::Bound) => overlay_(accrete, ctx, id, attrs),
        (PanelPlace::Viewport, PanelState::Toggle) => drawer_(accrete, ctx, id, attrs),
    }
}

/// `place: inline` + `state: toggle`: a collapsible area that takes layout.
///
/// One grid row (icon / header / body in columns 3/4), the whole row clickable. The `children` are all body — this cell has no trigger
/// and body split, so opening is just putting the body into the grid; which is why `attrs.replace_header` has no header to swap here (it governed
/// the header of `item`, retired along with `fold`). The initial open state reads the boolean of `source.default`.
fn inline_(accrete: Panel, ctx: &Ctx, id: String, _attrs: PanelAttr) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["grid"];
    css.push(id.as_str());
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let show = RwSignal::new(
        use_default(&accrete)
            .and_then(|x| x.as_bool())
            .unwrap_or_default(),
    );
    let children = accrete.children.clone().unwrap_or_default();

    move || -> AnyView {
        let s = show.get();
        let onclick = move |_: web_sys::MouseEvent| {
            show.set(!show.get());
        };

        let b: AnyView = if s {
            div().child(render_children(&ctx, &children)).into_any()
        } else {
            div().into_any()
        };

        let icon_class = if s { "icon open" } else { "icon close " };
        div()
            .id(id.as_str())
            .class(css.as_str())
            .style("grid-template-columns: auto 1fr;".to_string())
            .on(click, onclick)
            .child(
                div()
                    .class(icon_class)
                    .style("height: 100%; aspect-ratio: 1 / 1;".to_string()),
            )
            .child(div())
            .child(div())
            .child(b)
            .into_any()
    }
    .into_any()
}

/// `place: anchor` + `state: toggle`: a float hugging the trigger (ADR 0026).
///
/// Showing/hiding used to be CSS `:hover` (`.popup:hover > .body` in `core.css`) — a state only pointer
/// devices have: a touch screen has no hover, so the float simply would not open. Now the state is the local signal `show`: a trigger click
/// toggles it, Esc and a click outside close it. `sub[0]` is the trigger, `sub[1]` is the float content; missing one draws nothing.
fn anchor_(accrete: Panel, ctx: &Ctx, id: String, attrs: PanelAttr) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["popup", "flex"];
    use_common_css(&mut css, &accrete);
    let base = css.join(" ");
    let style = attrs.direction_style();

    let subs = accrete.children.clone().unwrap_or_default();
    let trigger = subs.first().cloned();
    let body = subs.get(1).cloned();

    let show = RwSignal::new(false);
    let root = NodeRef::<Div>::new();

    // The two legs of closing: Esc, and "a click outside the float". Both hang on window — the float is an overlay, so a click may
    // land anywhere outside it, and listening inside the container alone would miss it. The listeners die with the component's scope (the handle is
    // removed in cleanup), otherwise every rebuild would add one.
    {
        let on_key = window_event_listener(keydown, move |ev| {
            if ev.key() == "Escape" {
                show.set(false);
            }
        });
        let on_click = window_event_listener(click, move |ev| {
            if !show.get_untracked() {
                return;
            }
            // A click inside its own subtree (trigger included) does not count as outside; the trigger itself flips the state.
            let inside = root
                .get_untracked()
                .zip(ev.target().and_then(|t| t.dyn_into::<web_sys::Node>().ok()))
                .map(|(el, target)| el.contains(Some(&target)))
                .unwrap_or(false);
            if !inside {
                show.set(false);
            }
        });
        on_cleanup(move || {
            on_key.remove();
            on_click.remove();
        });
    }

    let trigger_view = {
        let ctx = ctx.clone();
        move || match &trigger {
            Some(t) => render_accrete(&ctx, t),
            None => ().into_any(),
        }
    };
    let body_view = {
        let ctx = ctx.clone();
        move || -> AnyView {
            if !show.get() {
                return ().into_any();
            }
            match &body {
                Some(b) => {
                    let ctx = ctx.clone();
                    div()
                        .class("flex body")
                        .child(render_accrete(&ctx, b))
                        .into_any()
                }
                None => ().into_any(),
            }
        }
    };

    div()
        // Identity: a panel must be findable (`portal` points into it, other selectors point at it, debugging can see it).
        .id(id)
        .class(move || {
            if show.get() {
                format!("{base} open")
            } else {
                base.clone()
            }
        })
        .style(style)
        .node_ref(root)
        .child(
            div()
                .class("flex trigger")
                .on(click, move |_| show.update(|s| *s = !*s))
                .child(trigger_view),
        )
        .child(body_view)
        .into_any()
}

/// `place: viewport` + `state: bound`: the modal layer (ADR 0014).
///
/// Shown/hidden has one rule only: **a non-empty bound slot means shown** (ADR 0012's "empty means absent"). When `dismiss` clicks the layer itself
/// (the blurred area, not the modal content) it **clears the bound slot** — clearing is closing, and no second masked state is invented; with the slot empty,
/// the next write is necessarily a new value, and the same-value short circuit cannot wrongly block a reopen. So dismiss needs a slot binding: a layer with an inline
/// default has no slot to clear and only warns.
///
/// The geometry must be inline, two measured lessons: (1) the ordering defect of the custom layer loses to the
/// element layer until core.css's layer order declaration is put back (PLAN §19), while inline has no cascade ambiguity at all, the same as ADR 0012's inline display; (2) on this machine's
/// headless chromium the height of `position: fixed` + `inset: 0` does not fill (a clean div measured 20px),
/// so `height` must be given explicitly. The mask (blur/tint) comes from `.layer::before` in custom.css.
fn overlay_(accrete: Panel, ctx: &Ctx, id: String, attrs: PanelAttr) -> AnyView {
    let dismiss = attrs.dismiss.unwrap_or(false);
    let mut css = vec!["layer", "flex"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let (local_slot, data_source, inline) = bind_slot(accrete.source.as_ref());
    // Position: given `right`/`top` or the like it hugs that corner of the page (the former `float` was this), with nothing given it fills
    // the viewport and centers — both are "layers", only the geometry differs.
    let shown_style = attrs
        .pos
        .as_ref()
        .map(|p| {
            format!(
                "position: fixed; {} z-index: 100; display: flex;",
                p.into_style()
            )
        })
        .unwrap_or_else(|| LAYER_STYLE.to_string());
    // The value at construction time (before the first Effect): same as the canvas host, left to the Effect below to sync
    let style_now = if payload_is_empty(&read_payload(ctx, &local_slot, &data_source, &inline)) {
        HIDDEN_STYLE.to_string()
    } else {
        shown_style.clone()
    };

    let nr = NodeRef::<Div>::new();
    {
        let ctx = ctx.clone();
        let (l, s, i) = (local_slot.clone(), data_source.clone(), inline.clone());
        let shown = shown_style.clone();
        Effect::new(move |_| {
            let want = if payload_is_empty(&read_payload(&ctx, &l, &s, &i)) {
                HIDDEN_STYLE.to_string()
            } else {
                shown.clone()
            };
            if let Some(el) = nr.get() {
                el.set_attribute("style", &want).ok();
            }
        });
    }

    // The closure needs 'static: both Ctx and id must be cloned into scope first (ctx is an Rc, clone just bumps the count)
    let onclick = {
        let ctx = ctx.clone();
        let id = id.clone();
        move |ev: web_sys::MouseEvent| {
            if !dismiss {
                return;
            }
            // e.target === e.currentTarget: the click is on the layer's own blank area (the blurred surface), not on the modal content —
            // the event also arrives here along the bubbling path, and the equality test rules the content out.
            let same = match (ev.target(), ev.current_target()) {
                (Some(t), Some(c)) => {
                    // Ask explicitly for the JsValue view: EventTarget's AsRef has three impls, and without an annotation it hits E0283
                    let tv: &wasm_bindgen::JsValue = t.as_ref();
                    let cv: &wasm_bindgen::JsValue = c.as_ref();
                    tv == cv
                }
                _ => false,
            };
            if !same {
                return;
            }
            if let Some(slot) = &local_slot {
                ctx.slot_for_value(slot).set(None);
            } else if let Some(src) = &data_source {
                ctx.slot_for_data(src).set(None);
            } else {
                tracing::warn!(
                    "panel {id}: dismiss 需要槽绑定（清空槽 = 关），inline default 无槽可清"
                );
            }
        }
    };

    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();

    // The sketch skin (ADR 0038 §4): the layer exists only for a node that asked for a border.
    let layer = rough::border_layer(
        ctx,
        accrete.attrs.as_ref().and_then(|a| a.class.as_ref()),
        Some(&id),
    );
    div()
        .id(id)
        .class(css.as_str())
        .style(style_now)
        .on(click, onclick)
        .child(children)
        .child(layer)
        .node_ref(nr)
        .into_any()
}

/// Flush to the viewport + local toggle: the drawer.
///
/// `children[0]` is the trigger, `children[1]` is the sidebar body (missing one draws nothing). The body flushes to the edge
/// `direction` names (default L), and the width comes from `attrs.width` or the class. There are two ways to close it, the same set of
/// terms as the other two cells: clicking the mask (`dismiss`, on by default) and Esc.
///
/// Animation is not the framework's to give: once the body is mounted it carries the class, and sliding in is CSS's job (the same division as rack's enter).
fn drawer_(accrete: Panel, ctx: &Ctx, id: String, attrs: PanelAttr) -> AnyView {
    let subs = accrete.children.clone().unwrap_or_default();
    let Some(trigger) = subs.first().cloned() else {
        tracing::warn!("panel {id}: 抽屉缺触发器（children[0]），不画");
        return ().into_any();
    };
    let Some(body) = subs.get(1).cloned() else {
        tracing::warn!("panel {id}: 抽屉缺内容（children[1]），不画");
        return ().into_any();
    };

    let show = RwSignal::new(false);
    let dismiss = attrs.dismiss.unwrap_or(true);
    let edge = attrs.direction.unwrap_or(Direction::L);
    let vertical = matches!(edge, Direction::U | Direction::D);
    let size = attrs.width.clone().unwrap_or_else(|| {
        if vertical {
            "min(60vh, 100%)".to_string()
        } else {
            "min(34rem, 92vw)".to_string()
        }
    });

    // Esc closes. The same policy as anchor: a panel's shown/hidden state is local, and the keyboard is just another hand.
    let esc = window_event_listener(keydown, move |ev| {
        if ev.key() == "Escape" {
            show.set(false);
        }
    });
    on_cleanup(move || esc.remove());

    let align = match edge {
        Direction::L => "justify-content: flex-start;",
        Direction::R => "justify-content: flex-end;",
        Direction::U => "align-items: flex-start;",
        Direction::D => "align-items: flex-end;",
    };
    let body_style = if vertical {
        format!("height: {size}; width: 100%;")
    } else {
        format!("width: {size}; height: 100%;")
    };

    let trigger_ctx = ctx.clone();
    let body_ctx = ctx.clone();
    view! {
        <div id=id class="panel flex col">
            <div
                class="panel-trigger flex"
                on:click=move |ev: web_sys::MouseEvent| {
                    ev.stop_propagation();
                    show.update(|v| *v = !*v);
                }
            >
                {render_accrete(&trigger_ctx, &trigger)}
            </div>
            {move || {
                if !show.get() {
                    return ().into_any();
                }
                let mask = if dismiss {
                    Some(view! {
                        <div
                            class="panel-mask"
                            style="position: absolute; inset: 0; background: rgba(0, 0, 0, 0.28);"
                            on:click=move |_| show.set(false)
                        ></div>
                    })
                } else {
                    None
                };
                view! {
                    <div
                        class="panel-viewport"
                        style=format!(
                            "position: fixed; inset: 0; height: 100vh; z-index: 100; display: flex; {align} align-items: stretch;"
                        )
                    >
                        {mask}
                        <div
                            class="panel-body flex col"
                            style=format!(
                                "position: relative; {body_style} background: var(--bg, Canvas); box-shadow: 0 0 2rem rgba(0,0,0,.25); overflow: auto;"
                            )
                            on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()
                        >
                            {render_accrete(&body_ctx, &body)}
                        </div>
                    </div>
                }
                .into_any()
            }}
        </div>
    }
    .into_any()
}

/// Layer geometry: fill the viewport + center the modal content. z-index above ordinary floats.
const LAYER_STYLE: &str = "position: fixed; inset: 0; height: 100vh; z-index: 100; \
                           display: flex; align-items: center; justify-content: center;";

/// `source` → (local slot name, data slot name, inline default), decided by `scope` (ADR 0031 §1).
/// `both` gives both addresses: the local assignment wins while it exists, the far side is
/// the fallback.
fn bind_slot(bind: Option<&Source>) -> (Option<String>, Option<String>, Option<Value>) {
    match bind {
        Some(b) => match &b.slot {
            Some(slot) if b.scope.local() => (
                Some(slot.clone()),
                b.scope.remote().then(|| slot.clone()),
                None,
            ),
            Some(slot) => (None, Some(slot.clone()), None),
            None => (None, None, b.default.clone()),
        },
        None => (None, None, None),
    }
}

/// Current payload: the bare value of the local slot / the node in the data slot (its `source.default` first, otherwise the whole-node JSON) / the literal
/// default. A slot never written = Null = empty (for the "empty means absent" criterion see payload_is_empty).
fn read_payload(
    ctx: &Ctx,
    local: &Option<String>,
    source: &Option<String>,
    inline: &Option<Value>,
) -> Value {
    // `scope: both`: the local assignment is the draft, the remote slot the fallback — an
    // empty local plane must not hide a value that did arrive.
    if let Some(slot) = local
        && let Some(v) = ctx.slot_for_value(slot).get()
    {
        return v;
    }
    if let Some(src) = source {
        return match ctx.slot_for_data(src).get().map(|a| (*a).clone()) {
            Some(node) => node
                .get_source()
                .and_then(|v| v.default.clone())
                .unwrap_or_else(|| serde_json::to_value(&node).unwrap_or(Value::Null)),
            None => Value::Null,
        };
    }
    inline.clone().unwrap_or(Value::Null)
}
