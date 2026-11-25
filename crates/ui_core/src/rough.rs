//! Hand-drawn borders (ADR 0038 §4): `roughr` gives the geometry, `currentColor`
//! paints it, and the host box's `ResizeObserver` gives the size.
//!
//! One thing this module refuses to do is exist in `normal` mode. The layer is
//! mounted by a reactive branch on `config.skin`, so a page without the skin pays
//! neither an element nor an observer — "no extra node, no branch taken" is a
//! promise the skin's default makes, and this is where it is kept.

use crate::Ctx;
use leptos::html::span;
use leptos::prelude::*;
use leptos::svg;
use roughr::core::{OpSet, OpSetType, OpType, OptionsBuilder};
use roughr::generator::Generator;
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// The pen: one rectangle, drawn the way a hand would. Each returned string is one
/// tracing, and there are two of them — the same rectangle by a slightly different
/// hand — which is what makes a hairline read as pencil gone over twice rather than
/// as a shape traced once. Two tracings is the whole budget: an edge drawn more than
/// twice stops looking sketched and starts looking bold.
#[must_use]
pub fn border_paths(w: f32, h: f32, seed: u64) -> Vec<String> {
    both_hands(seed)
        .into_iter()
        .flat_map(|s| rect_passes(w, h, s))
        .collect()
}

fn rect_passes(w: f32, h: f32, seed: u64) -> Vec<String> {
    let Ok(opts) = OptionsBuilder::default()
        .seed(seed)
        .roughness(1.2)
        .bowing(1.0)
        // roughr traces every line twice of its own accord; two hands on top of that would be four passes
        // over one edge, which reads heavy instead of sketchy. One edge, two passes, and no more.
        .disable_multi_stroke(true)
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

/// Two hands for one line: the given seed, and the same seed stirred. The second is
/// never zero (that is roughr's "pick one for me") and never the first.
fn both_hands(seed: u64) -> [u64; 2] {
    [seed, seed.rotate_left(17) | 1]
}

/// The selected mark: an L in the corner — down the right edge and along the bottom,
/// a few pixels outside the box — drawn by the same two hands and traced the same way.
/// A heavy stroke would say "selected" in a voice the rest of the page does not use;
/// a right angle gone over twice says it in the one it does. The stylesheet decides
/// when it shows (the box carrying `selected`), so nothing here watches for a class.
#[must_use]
pub fn mark_paths(w: f32, h: f32, seed: u64) -> Vec<String> {
    let (out_x, out_y) = (w + 3.0, h + 3.0);
    let (in_x, in_y) = (6.0, 6.0);
    let mut paths = Vec::new();
    for s in both_hands(seed.rotate_right(9) | 1) {
        let Ok(opts) = OptionsBuilder::default()
            .seed(s)
            .roughness(0.9)
            .bowing(0.5)
            .disable_multi_stroke(true)
            .build()
        else {
            continue;
        };
        let pen = Generator::default();
        for (x1, y1, x2, y2) in [(out_x, in_y, out_x, out_y), (out_x, out_y, in_x, out_y)] {
            paths.extend(
                pen.line(x1, y1, x2, y2, &Some(opts.clone()))
                    .sets
                    .iter()
                    .filter(|set| set.op_set_type == OpSetType::Path)
                    .map(ops_to_d)
                    .filter(|d| !d.is_empty()),
            );
        }
    }
    paths
}

/// The tick inside a switch: two segments, down-right then up-right, drawn by the same two hands as
/// everything else. A switch is the one box whose *content* is a stroke — the CSS version paints a
/// `clip-path` shape, which is a filled glyph, and a filled glyph next to hand-drawn borders reads as
/// two different hands on one page. The path is always here and the stylesheet decides when it shows
/// (`input:checked` is a fact the stylesheet can see for itself), so nothing has to watch the state.
#[must_use]
pub fn check_paths(w: f32, h: f32, seed: u64) -> Vec<String> {
    let km = 1.6; // the tick sits a little inside the edge the pen drew
    let points = [
        (0.22 * w + km, 0.52 * h),
        (0.42 * w + km, 0.74 * h),
        (0.80 * w, 0.26 * h),
    ];
    let mut paths = Vec::new();
    for s in both_hands(seed.rotate_left(23) | 1) {
        let Ok(opts) = OptionsBuilder::default()
            .seed(s)
            .roughness(0.8)
            .bowing(0.4)
            .disable_multi_stroke(true)
            .build()
        else {
            continue;
        };
        let pen = Generator::default();
        for (a, b) in [(points[0], points[1]), (points[1], points[2])] {
            paths.extend(
                pen.line(a.0, a.1, b.0, b.1, &Some(opts.clone()))
                    .sets
                    .iter()
                    .filter(|set| set.op_set_type == OpSetType::Path)
                    .map(ops_to_d)
                    .filter(|d| !d.is_empty()),
            );
        }
    }
    paths
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

/// A stable seed per node. What identifies a node is its id when the page gave it one; when it did not,
/// the id it did not have hashed the same string for every node — so every box on a page without ids was
/// the same drawing, retraced stroke for stroke. A counter, advanced once per layer and mixed with
/// whatever does identify the node, makes the wobble a property of the box instead of a property of the
/// page happening to hold exactly one box.
///
/// Never the size — a resize must redraw the same hand, not a new one.
#[must_use]
pub fn seed_for(id: Option<&str>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.unwrap_or("").hash(&mut hasher);
    let nth = NODES.with(|c| {
        let n = c.get();
        c.set(n.wrapping_add(0x9E37_79B9_7F4A_7C15));
        n
    });
    // 0 is roughr's "pick one for me"; keep it out of the seeded path.
    mix(hasher.finish() ^ nth) | 1
}

/// SplitMix64's finaliser: the counter above is a straight addition, and a hand that walks in step with
/// the last one is exactly the sameness this is here to break.
fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

thread_local! {
    /// One tick per layer built, so two boxes with nothing to tell them apart still get different hands.
    static NODES: std::cell::Cell<u64> = const { std::cell::Cell::new(0x2545_F491_4F6C_DD1D) };
}

/// One call per border-capable widget: the layer only exists for a node that asked
/// for a border (`is-border` in its own class list) and only while the skin is
/// `sketch`. Every other case is an empty view — no element, no observer.
pub fn border_layer(ctx: &Ctx, class: Option<&Vec<String>>, id: Option<&str>) -> AnyView {
    let sketchy = class.is_some_and(|c| c.iter().any(|x| x == "is-border"));
    if !sketchy {
        return ().into_any();
    }
    sketch_layer(ctx, seed_for(id), false)
}

/// For a widget whose border is not something the page asked for but something its
/// element type always paints — a field, a select, a button. There is no `is-border`
/// to look for, so the only question left is the skin, which `sketch_layer` answers.
pub fn drawn_border(ctx: &Ctx, id: Option<&str>) -> AnyView {
    sketch_layer(ctx, seed_for(id), false)
}

/// A box for a widget that cannot hold its own drawing (a field is void, a textarea holds
/// text). The wrapper is `display: contents` until the skin is on, so normal mode has no
/// box of its own and the child lays out exactly as it did; sketching turns it into the
/// host the drawing needs and takes the child's painted edge away.
pub fn field_host(ctx: &Ctx, id: Option<&str>, check: bool, child: AnyView) -> AnyView {
    // `is-checkbox` is the stylesheet's handle on "this host holds a switch": a switch keeps its own
    // size in both skins (`flex: 0 0 auto` here, `width: 1.15em` on the child), while every other field
    // takes the width it is given. Without the marker the two cases cannot be told apart from CSS, since
    // the host has no attributes of its own to read.
    let class = if check {
        "rough-host is-checkbox"
    } else {
        "rough-host"
    };
    span()
        .class(class)
        .child(child)
        .child(sketch_layer(ctx, seed_for(id), check))
        .into_any()
}

/// Whether the page is in the drawing skin — the same question `sketch_layer` asks,
/// for a widget that has to decide its own shape (a field needs a host box for its
/// drawing; nothing else does).
#[must_use]
pub fn sketching(ctx: &Ctx) -> bool {
    matches!(ctx.config_get("sketch"), Some(Value::Bool(true)))
}

/// The layer, mounted only while the skin asks for it. `check` adds the tick a switch shows when it is
/// checked — a stroke of the same pen, not a filled glyph.
pub fn sketch_layer(ctx: &Ctx, seed: u64, check: bool) -> AnyView {
    let ctx = ctx.clone();
    (move || {
        // The page's options are ordinary values now (ADR 0038 §2 revised): `sketch` is a bool a control
        // can own, and the far side's `true` is what the producer sends when it builds the page.
        let sketch = matches!(ctx.config_get("sketch"), Some(Value::Bool(true)));
        sketch.then(|| border_svg(seed, check))
    })
    .into_any()
}

/// An absolutely positioned `<svg>` over its host box, redrawn when the box
/// resizes. `data-rough="ready"` on the host is what lets the stylesheet drop the
/// CSS border's paint; until it is there, the box keeps its plain border.
fn border_svg(seed: u64, check: bool) -> AnyView {
    let svg_ref = NodeRef::<svg::Svg>::new();
    let edges = RwSignal::new(Vec::<String>::new());
    let marks = RwSignal::new(Vec::<String>::new());
    let checks = RwSignal::new(Vec::<String>::new());
    // The shadow is geometry too: normal mode throws a hard 2px offset (`--shadow-width`), which is the same
    // box drawn twice — so under the pen it is the same box drawn twice, in a lighter ink, with its own hand.
    // Nothing in a sketch is blurred; a shadow that is a blur is the one thing the skin cannot draw.
    let shadows = RwSignal::new(Vec::<String>::new());
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
                edges.set(Vec::new());
                marks.set(Vec::new());
                return;
            }
            let d = border_paths(w, h, seed);
            if d.is_empty() {
                return;
            }
            view_box.set(format!("0 0 {w} {h}"));
            host_mark.set_attribute("data-rough", "ready").ok();
            edges.set(d);
            shadows.set(border_paths(w, h, seed.rotate_left(41) | 1));
            marks.set(mark_paths(w, h, seed));
            if check {
                checks.set(check_paths(w, h, seed));
            }
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

    leptos::svg::svg()
        .node_ref(svg_ref)
        .class("rough")
        .attr("aria-hidden", "true")
        .attr("viewBox", move || view_box.get())
        .child(move || {
            // Paint lives in the stylesheet (`svg.rough path`): one place for the
            // pen, and `currentColor` is what makes the stroke follow
            // `light-dark()` without this file knowing dark mode exists. The class
            // says which of the two marks a path belongs to — the edge, or the L
            // that only a selected box shows.
            let edge = edges
                .get()
                .into_iter()
                .map(|d| leptos::svg::path().class("edge").attr("d", d).into_any());
            // Drawn first, so the edge's ink sits on top of it: a shadow is behind the thing that casts it.
            let shadow = shadows
                .get()
                .into_iter()
                .map(|d| leptos::svg::path().class("edge").attr("d", d).into_any())
                .collect_view();
            let shade = leptos::svg::g().class("shadow").child(shadow);
            let mark = marks
                .get()
                .into_iter()
                .map(|d| leptos::svg::path().class("mark").attr("d", d).into_any());
            // The tick is a stroke like the rest: same pen, same `currentColor`, and the stylesheet
            // shows it when the switch says `:checked`.
            let tick = checks
                .get()
                .into_iter()
                .map(|d| leptos::svg::path().class("check").attr("d", d).into_any());
            std::iter::once(shade.into_any())
                .chain(edge)
                .chain(mark)
                .chain(tick)
                .collect_view()
        })
        .into_any()
}

#[cfg(test)]
mod tests {
    use super::{border_paths, check_paths, mark_paths, ops_to_d, seed_for};
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
    fn a_one_pixel_line_is_gone_over_more_than_once() {
        // One tracing reads as a traced shape; two hands over the same rectangle read as pencil. Each string
        // is one hand (four edges, one subpath each), so two strings and eight subpaths is the floor.
        let d = border_paths(120.0, 60.0, 7);
        assert!(d.len() >= 2, "a border is retraced: {d:?}");
        let subpaths: usize = d.iter().map(|one| one.matches("M ").count()).sum();
        assert!(subpaths >= 8, "four edges, twice over: {subpaths}");
    }

    #[test]
    fn the_selected_mark_is_an_l_outside_the_box() {
        let marks = mark_paths(120.0, 60.0, 7);
        assert!(!marks.is_empty(), "no mark for a real box");
        // The L lives a few pixels past the box's right edge and below its bottom.
        assert!(
            marks.iter().any(|d| d.contains(&format!("{}", 123.0))
                || d.split_whitespace()
                    .any(|n| n.parse::<f32>().is_ok_and(|v| v > 120.0))),
            "the mark is not outside the box: {marks:?}"
        );
        assert_eq!(marks, mark_paths(120.0, 60.0, 7));
        assert_ne!(marks, mark_paths(120.0, 60.0, 8));
    }

    #[test]
    fn the_tick_is_stroked_inside_the_box() {
        let tick = check_paths(20.0, 20.0, 7);
        assert!(!tick.is_empty(), "no tick for a real switch");
        // Two segments, two hands: four passes at the very least, and each one starts with a move.
        assert!(
            tick.len() >= 4,
            "a tick is retraced like every other line: {tick:?}"
        );
        for one in &tick {
            assert!(
                one.starts_with('M'),
                "a pass must start with a move: {one:?}"
            );
            for n in one.split_whitespace().filter_map(|s| s.parse::<f32>().ok()) {
                assert!(
                    (-4.0..=24.0).contains(&n),
                    "the tick left its box: {n} in {one:?}"
                );
            }
        }
        assert_eq!(tick, check_paths(20.0, 20.0, 7));
        assert_ne!(tick, check_paths(20.0, 20.0, 8));
    }

    #[test]
    fn a_seed_belongs_to_the_node_not_to_the_page() {
        // Two boxes with nothing to tell them apart still get different hands. Without the counter, every
        // id-less node hashed the same empty string and the page was one drawing repeated.
        assert_ne!(seed_for(None), seed_for(None));
        assert_ne!(seed_for(Some("card")), seed_for(Some("card")));
        // Never 0: that is roughr's "pick one for me", and it would leave the drawing out of our hands.
        assert_ne!(seed_for(None), 0);
        assert_eq!(seed_for(None) & 1, 1);
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
