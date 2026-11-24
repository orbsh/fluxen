//! Hand-drawn borders (ADR 0038 §4): `roughr` gives the geometry, `currentColor`
//! paints it, and the host box's `ResizeObserver` gives the size.
//!
//! One thing this module refuses to do is exist in `normal` mode. The layer is
//! mounted by a reactive branch on `config.skin`, so a page without the skin pays
//! neither an element nor an observer — "no extra node, no branch taken" is a
//! promise the skin's default makes, and this is where it is kept.

use crate::Ctx;
use leptos::prelude::*;
use leptos::svg;
use roughr::core::{OpSet, OpSetType, OpType, OptionsBuilder};
use roughr::generator::Generator;
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// The pen: one rectangle, drawn the way a hand would. Each returned string is one
/// stroked pass — `roughr` emits two, and the second pass is the tell that makes a
/// sketch look drawn rather than traced.
#[must_use]
pub fn border_paths(w: f32, h: f32, seed: u64) -> Vec<String> {
    let Ok(opts) = OptionsBuilder::default()
        .seed(seed)
        .roughness(1.2)
        .bowing(1.0)
        .build()
    else {
        return Vec::new();
    };
    let drawable = Generator::default().rectangle(0.0f32, 0.0, w, h, &Some(opts));
    drawable
        .sets
        .iter()
        .filter(|s| s.op_set_type == OpSetType::Path)
        .map(ops_to_d)
        .filter(|d| !d.is_empty())
        .collect()
}

/// One op set as SVG path data. Three ops are all a stroked outline uses; an op
/// whose arity is anything else is skipped rather than trusted — a border that
/// misses a segment still looks hand-drawn, a panic does not.
fn ops_to_d(set: &OpSet<f32>) -> String {
    use std::fmt::Write as _;
    let mut d = String::new();
    for op in &set.ops {
        let v = &op.data;
        match op.op {
            OpType::Move => {
                if let [x, y] = v[..] {
                    let _ = write!(d, "M {x} {y} ");
                }
            }
            OpType::LineTo => {
                if let [x, y] = v[..] {
                    let _ = write!(d, "L {x} {y} ");
                }
            }
            OpType::BCurveTo => {
                if let [x1, y1, x2, y2, x, y] = v[..] {
                    let _ = write!(d, "C {x1} {y1} {x2} {y2} {x} {y} ");
                }
            }
        }
    }
    d
}

/// A stable seed per node: its id when it has one, else the id it did not have.
/// Never the size — a resize must redraw the same hand, not a new one.
#[must_use]
pub fn seed_for(id: Option<&str>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.unwrap_or("").hash(&mut hasher);
    // 0 is roughr's "pick one for me"; keep it out of the seeded path.
    hasher.finish() | 1
}

/// One call per border-capable widget: the layer only exists for a node that asked
/// for a border (`is-border` in its own class list) and only while the skin is
/// `sketch`. Every other case is an empty view — no element, no observer.
pub fn border_layer(ctx: &Ctx, class: Option<&Vec<String>>, id: Option<&str>) -> AnyView {
    let sketchy = class.is_some_and(|c| c.iter().any(|x| x == "is-border"));
    if !sketchy {
        return ().into_any();
    }
    sketch_layer(ctx, seed_for(id))
}

/// The layer, mounted only while the skin asks for it.
pub fn sketch_layer(ctx: &Ctx, seed: u64) -> AnyView {
    let ctx = ctx.clone();
    view! {
        {move || {
            let sketch = matches!(ctx.config_get("skin"), Some(Value::String(s)) if s == "sketch");
            sketch.then(|| view! { <RoughBorder seed=seed/> })
        }}
    }
    .into_any()
}

/// An absolutely positioned `<svg>` over its host box, redrawn when the box
/// resizes. `data-rough="ready"` on the host is what lets the stylesheet drop the
/// CSS border's paint; until it is there, the box keeps its plain border.
#[component]
pub fn RoughBorder(seed: u64) -> impl IntoView {
    let svg_ref = NodeRef::<svg::Svg>::new();
    let paths = RwSignal::new(Vec::<String>::new());
    let view_box = RwSignal::new(String::new());

    Effect::new(move |_| {
        // The host is the element this layer is a child of: what gets drawn is its
        // border box, which is why the layer is inset by one border width.
        let Some(host) = svg_ref.get().and_then(|el| el.parent_element()) else {
            return;
        };
        let last = std::cell::Cell::new((0.0f32, 0.0f32));
        let host_measure = host.clone();
        let host_mark = host.clone();
        let cb = Closure::<dyn FnMut(js_sys::Array, web_sys::ResizeObserver)>::new(move |_, _| {
            // offsetWidth/Height are the border box in whole pixels — the same box
            // the CSS border occupies, and the one the layer is inset to.
            let Some(box_el) = host_measure.dyn_ref::<web_sys::HtmlElement>() else {
                return;
            };
            let w = box_el.offset_width() as f32;
            let h = box_el.offset_height() as f32;
            let prev = last.get();
            if (prev.0 - w).abs() < 0.5 && (prev.1 - h).abs() < 0.5 {
                return;
            }
            last.set((w, h));
            if w < 1.0 || h < 1.0 {
                paths.set(Vec::new());
                return;
            }
            let d = border_paths(w, h, seed);
            if d.is_empty() {
                return;
            }
            view_box.set(format!("0 0 {w} {h}"));
            host_mark.set_attribute("data-rough", "ready").ok();
            paths.set(d);
        });
        match web_sys::ResizeObserver::new(cb.as_ref().unchecked_ref()) {
            Ok(ro) => {
                ro.observe(&host);
                // The observer owns the callback for as long as it observes; the
                // cleanup below is what ends that (same shape as the module hosts).
                cb.forget();
                let ro_off = ro.clone();
                let host_off = host.clone();
                on_cleanup(move || {
                    ro_off.disconnect();
                    host_off.remove_attribute("data-rough").ok();
                });
            }
            // No observer (old browser): draw nothing. The CSS border stays painted,
            // which is the same page the skin promised before this layer existed.
            Err(_) => tracing::warn!("rough border: no ResizeObserver, CSS border stays"),
        }
    });

    view! {
        <svg
            node_ref=svg_ref
            class="rough"
            aria-hidden="true"
            viewBox=move || view_box.get()
        >
            {move || {
                paths
                    .get()
                    .into_iter()
                    .map(|d| {
                        // Paint lives in the stylesheet (`svg.rough path`): one place
                        // for the pen, and `currentColor` is what makes the stroke
                        // follow `light-dark()` without this file knowing dark mode
                        // exists.
                        view! { <path d=d /> }
                    })
                    .collect_view()
            }}
        </svg>
    }
}

#[cfg(test)]
mod tests {
    use super::{border_paths, ops_to_d, seed_for};
    use roughr::core::{Op, OpSet, OpSetType, OpType};

    #[test]
    fn a_rectangle_comes_back_as_stroked_passes() {
        let d = border_paths(120.0, 60.0, 7);
        assert!(!d.is_empty(), "no passes for a real box");
        for one in &d {
            assert!(
                one.starts_with('M'),
                "a pass must start with a move: {one:?}"
            );
        }
    }

    #[test]
    fn the_same_seed_draws_the_same_line() {
        assert_eq!(border_paths(120.0, 60.0, 7), border_paths(120.0, 60.0, 7));
    }

    #[test]
    fn a_different_seed_draws_a_different_line() {
        assert_ne!(border_paths(120.0, 60.0, 7), border_paths(120.0, 60.0, 8));
    }

    #[test]
    fn the_seed_is_never_zero_and_never_empty() {
        assert_ne!(seed_for(None) & 1, 1u64 ^ 1);
        assert_ne!(seed_for(Some("card")), 0);
        assert_eq!(seed_for(Some("card")), seed_for(Some("card")));
        assert_ne!(seed_for(Some("card")), seed_for(Some("panel")));
    }

    #[test]
    fn an_op_with_the_wrong_arity_is_skipped_not_trusted() {
        let set = OpSet {
            op_set_type: OpSetType::Path,
            ops: vec![
                Op {
                    op: OpType::Move,
                    data: vec![1.0],
                },
                Op {
                    op: OpType::LineTo,
                    data: vec![2.0, 3.0],
                },
            ],
            size: None,
            path: None,
        };
        assert_eq!(ops_to_d(&set), "L 2 3 ");
    }
}
