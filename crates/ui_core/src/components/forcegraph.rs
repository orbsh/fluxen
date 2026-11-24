//! Relation graph component (ADR 0018): a native leptos + wasm renderer, not a canvas module.
//!
//! The code here does three things only — build the host element, feed the data slot's payload to the renderer, and hand the events
//! the renderer emits to Ctx. Rendering, layout, camera and picking all live in the `forcegraph` crate,
//! which knows neither Accrete nor leptos.

use std::cell::RefCell;
use std::rc::Rc;

use accrete::classify::Classify;
use accrete::{ForceGraph, ForceGraphStyle};
use leptos::html::Div;
use leptos::html::div;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

use crate::{Ctx, hooks};

pub fn forcegraph_(accrete: ForceGraph, ctx: &Ctx, id: String) -> AnyView {
    let style = style_of(&accrete.style);
    let css = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.get_class().clone().map(|c| c.join(" ")))
        .unwrap_or_default();
    let size = accrete
        .attrs
        .as_ref()
        .map(|a| a.size_style())
        .unwrap_or_default();
    // A canvas with a shader must have a real size: even with no attrs a drawable height must be given,
    // otherwise the canvas is 0×0 and the picture "shows nothing", though that is not a render failure.
    let size = if size.is_empty() {
        "height: 360px;".to_string()
    } else {
        size
    };

    // Beyond size there are three structural properties: the label layer and the hover card are absolutely positioned children of the host, so the positioning
    // base must be the host itself, and labels and cards beyond the panel must be clipped. The `style` attribute belongs to the
    // Effect below and gets rewritten, so these three must be written together with the size — writing the size alone would wipe out the
    // `position: relative` / `overflow: hidden` written at `mount` (symptom: the hover card appears
    // below the canvas, outside the panel, instead of beside the node).
    let host_style = format!("{size}position: relative; margin: 0; padding: 0; overflow: hidden;");
    let nr = NodeRef::<Div>::new();
    let view: Rc<RefCell<Option<forcegraph::View>>> = Rc::new(RefCell::new(None));

    {
        // `on_cleanup` wants a `Send` closure; the view is thread-local by
        // nature (GL context, DOM), so it rides a `SendWrapper` — cleanup runs
        // on the main thread in CSR, where the wrapper is transparent.
        let view = send_wrapper::SendWrapper::new(Rc::clone(&view));
        on_cleanup(move || {
            if let Some(mut v) = view.borrow_mut().take() {
                v.dispose();
            }
        });
    }

    let node_id = id.clone();
    let ctx_load = ctx.clone();
    let accrete_load = accrete.clone();
    let view_load = Rc::clone(&view);
    let nr_load = nr;
    nr_load.on_load(move |el| {
        let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() else {
            tracing::warn!("forcegraph {node_id}: host element is not an HtmlElement");
            return;
        };
        let emit_id = node_id.clone();
        let emit_ctx = ctx_load.clone();
        let mounted = forcegraph::View::mount(&el, style, move |event, data| {
            let ctx = emit_ctx.clone();
            let id = emit_id.clone();
            let event = event.to_string();
            // Same uplink leg as a bundle's submit: a name plus a payload, the
            // host does not interpret it.
            leptos::task::spawn_local(async move {
                ctx.send(event, Some(id), data).await;
            });
        });
        match mounted {
            Some(v) => *view_load.borrow_mut() = Some(v),
            None => {
                tracing::warn!("forcegraph {node_id}: WebGL2 context unavailable");
                el.set_attribute("data-forcegraph-failed", "webgl2").ok();
                return;
            }
        }

        // Theme channel (optional): `theme` binds a local slot or a source, and the payload is directly
        // `true/false` or `"dark"/"light"` — an event's data is a bare Value to begin with,
        // and a boolean needs no wrapping at all. This only translates into an enum and swaps the appearance: scene, camera and layout
        // stay untouched, so switching themes does not drag the picture into a relayout.
        let ctx_theme = ctx_load.clone();
        let accrete_theme = accrete_load.clone();
        let view_theme = Rc::clone(&view_load);
        Effect::new(move |_| {
            let Some(value) = hooks::value_of(&ctx_theme, accrete_theme.theme.as_ref(), true)
            else {
                return;
            };
            let theme = match &value {
                Value::Bool(true) => Some(forcegraph::style::Theme::Dark),
                Value::Bool(false) => Some(forcegraph::style::Theme::Light),
                Value::String(s) if s.eq_ignore_ascii_case("dark") => {
                    Some(forcegraph::style::Theme::Dark)
                }
                Value::String(s) if s.eq_ignore_ascii_case("light") => {
                    Some(forcegraph::style::Theme::Light)
                }
                _ => None,
            };
            let Some(theme) = theme else { return };
            if let Some(view) = view_theme.borrow_mut().as_mut() {
                view.set_theme(theme);
            }
        });

        // Data area: the payload is `{nodes, links, focus?}`, the whole snapshot pushed to the renderer.
        // An empty payload = takes no place (ADR 0012), the same rule as canvas/chart.
        let ctx2 = ctx_load.clone();
        let view2 = Rc::clone(&view_load);
        let accrete2 = accrete_load.clone();
        let host_style = host_style.clone();
        let host_el = el.clone();
        Effect::new(move |_| {
            let payload = hooks::use_source(&ctx2, &accrete2);
            let empty = payload
                .as_ref()
                .map(super::canvas::payload_is_empty)
                .unwrap_or(true);
            // Placement follows the payload (ADR 0012): an empty data area = collapse the container, a non-empty one writes back
            // the node's own size. Change the attribute rather than add a class: `.f` and `.hide` suppress each other.
            let want = if empty {
                super::canvas::HIDDEN_STYLE.to_string()
            } else {
                host_style.clone()
            };
            host_el.set_attribute("style", &want).ok();
            let Some(value) = payload else { return };
            if empty {
                return;
            }
            let mut holder = view2.borrow_mut();
            let Some(view) = holder.as_mut() else {
                return;
            };
            match serde_json::from_value::<forcegraph::Snapshot>(value) {
                Ok(snapshot) => view.set_payload(&snapshot),
                Err(e) => tracing::warn!("forcegraph payload rejected: {e}"),
            }
        });
    });

    // `fg-host` drops the padding `.box` carries: the canvas measures itself
    // against this element, so padding becomes inset canvas rather than margin.
    let mut classes = vec!["box", "fg-host"];
    if !css.is_empty() {
        classes.push(css.as_str());
    }
    let class = classes.join(" ");
    div()
        .class(class)
        .attr("style", size)
        .node_ref(nr)
        .into_any()
}

/// DSL knob → renderer knob. The fields on both sides correspond one to one, and the only mapping point is here.
fn style_of(style: &ForceGraphStyle) -> forcegraph::Style {
    forcegraph::Style {
        node_size_px: style.node_size_px,
        node_size_step_px: style.node_size_step_px,
        node_tiers: style.node_tiers,
        link_width_px: style.link_width_px,
        link_shape: match style.link_shape {
            accrete::LinkShape::Straight => forcegraph::frame::LinkShape::Straight,
            accrete::LinkShape::Axes => forcegraph::frame::LinkShape::Axes,
            accrete::LinkShape::Tetra => forcegraph::frame::LinkShape::Tetra,
        },
        node_size_ratio: style.node_size_ratio,
        link_opacity: style.link_opacity,
        charge: style.charge,
        link_distance: style.link_distance,
        link_strength: style.link_strength,
        velocity_decay: style.velocity_decay,
        labels: style.labels,
        label_px: style.label_px,
        label_max_nodes: style.label_max_nodes,
        auto_fit: style.auto_fit,
        fit_padding_px: style.fit_padding_px,
        background: style.background.clone(),
        palette: style.palette.clone(),
        glow_alpha: style.glow_alpha,
        highlight_reach: style.highlight_reach,
        reach_hops: style.reach_hops,
        theme: match style.theme {
            accrete::Theme::Light => forcegraph::style::Theme::Light,
            accrete::Theme::Dark => forcegraph::style::Theme::Dark,
        },
    }
}

/// `Value` → snapshot, for callers without a direct serde path (tests).
pub fn snapshot_of(value: &Value) -> Option<forcegraph::Snapshot> {
    serde_json::from_value(value.clone()).ok()
}
