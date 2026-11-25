//! A disc in the `oklch` plane: the third field shape, beside `input` and `select` (ADR 0048).
//!
//! A field has one value and one timing; what differs between `input`, `select` and `disc` is the gesture.
//! A disc shows two of `oklch`'s three terms as a face — the **angle is the hue**, the **radius is the
//! chroma** (the centre is grey) — and pins the third, because a flat face has no room for a third axis.
//! What it packs is therefore the whole `L C H` triple a colour member reads: the picker and the value stay
//! one thing, and nothing downstream has to know a disc was involved.
//!
//! Nothing here computes colour: the face is a `conic-gradient` of `oklch()` stops with a grey radial fade
//! over it, so the browser does the mixing — the same reason `select` renders its options as nodes instead
//! of computing a menu.

use crate::Ctx;
use crate::hooks::{field_changed, locked, node_id, use_common_css};
use accrete::Disc;
use leptos::ev;
use leptos::html::div;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

/// The most chroma the rim carries: the same cap a colour member is validated against, so the face and the
/// validator agree about how far "saturated" goes.
pub const RIM_CHROMA: f64 = 0.4;

/// The hue a point names, measured from the disc's centre: `0` at the top, clockwise, `0..360`.
///
/// Screen coordinates grow downwards, so `atan2(dy, dx)` measures from the positive x axis with positive
/// angles pointing *down*; rotating by a quarter turn puts zero at twelve o'clock and keeps the sweep
/// clockwise, which is the direction `conic-gradient` sweeps.
pub fn hue_at(dx: f64, dy: f64) -> f64 {
    (dy.atan2(dx).to_degrees() + 90.0).rem_euclid(360.0)
}

/// The chroma a point names: `0` at the centre, [`RIM_CHROMA`] at the rim, clamped outside the disc.
///
/// A small dead zone at the centre means **grey**: a disc's centre is one meaningful point, and asking a hand to
/// hit it to the pixel would make "no chroma" the hardest value on the face to reach.
pub fn chroma_at(dx: f64, dy: f64, radius: f64) -> f64 {
    if radius <= 0.0 {
        return 0.0;
    }
    let reach = (dx.hypot(dy) / radius).clamp(0.0, 1.0);
    if reach < 0.03 { 0.0 } else { reach * RIM_CHROMA }
}

/// Where a value stands on the face: the point in the box (a fraction of its width from the centre).
pub fn place(hue: f64, chroma: f64) -> (f64, f64) {
    let reach = (chroma / RIM_CHROMA).clamp(0.0, 1.0);
    let rad = (hue - 90.0).to_radians();
    (reach * rad.cos(), reach * rad.sin())
}

/// The triple this disc packs: its pinned lightness, and the two terms the point named.
fn triple(light: f64, chroma: f64, hue: f64) -> String {
    format!("{light} {chroma:.2} {}", hue.round() as i64)
}

/// The terms inside a value, if the value is one of ours. `""` (nothing chosen yet) is `None`.
fn terms_of(value: &str) -> Option<(f64, f64, f64)> {
    let parts: Vec<f64> = value
        .split_whitespace()
        .filter_map(|p| p.parse::<f64>().ok())
        .collect();
    match parts.as_slice() {
        [light, chroma, hue] if (0.0..=1.0).contains(light) && (0.0..=1.0).contains(chroma) => {
            Some((*light, *chroma, hue.rem_euclid(360.0)))
        }
        _ => None,
    }
}

/// The face at a given lightness: the hue wheel at the rim, fading to grey at the centre.
fn face(light: f64) -> String {
    let stops: Vec<String> = (0..=12)
        .map(|step| format!("oklch({light} {RIM_CHROMA} {}deg)", step * 30))
        .collect();
    format!(
        "radial-gradient(circle at 50% 50%, oklch({light} 0 0) 0%, oklch({light} 0 0 / 0) 72%), \
         conic-gradient(from 0deg, {})",
        stops.join(", ")
    )
}

#[cfg(test)]
mod disc_tests {
    use super::{chroma_at, hue_at, place, terms_of, triple, RIM_CHROMA};

    #[test]
    fn zero_is_up_and_the_sweep_is_clockwise() {
        assert_eq!(hue_at(0.0, -10.0), 0.0, "straight up is hue 0");
        assert_eq!(hue_at(10.0, 0.0), 90.0, "right is a quarter turn clockwise");
        assert_eq!(hue_at(0.0, 10.0), 180.0, "down is half");
        assert_eq!(hue_at(-10.0, 0.0), 270.0, "left is three quarters");
    }

    #[test]
    fn the_centre_is_grey_and_the_rim_is_saturated() {
        assert_eq!(chroma_at(0.0, 0.0, 50.0), 0.0, "the middle has no chroma");
        assert_eq!(chroma_at(50.0, 0.0, 50.0), RIM_CHROMA, "the rim is the cap");
        assert_eq!(chroma_at(90.0, 90.0, 50.0), RIM_CHROMA, "outside the disc clamps");
        let half = chroma_at(25.0, 0.0, 50.0);
        assert!((half - RIM_CHROMA / 2.0).abs() < 1e-9, "half way is half the chroma");
        assert_eq!(chroma_at(1.0, 0.0, 50.0), 0.0, "the dead zone keeps the centre reachable");
    }

    #[test]
    fn a_hue_and_a_chroma_land_where_they_came_from() {
        for (hue, chroma) in [(0.0, RIM_CHROMA), (90.0, RIM_CHROMA / 2.0), (270.0, 0.0)] {
            let (x, y) = place(hue, chroma);
            let back = chromium_round(hue_at(x, y));
            assert!((back - hue).abs() < 0.5, "hue {hue} came back as {back}");
        }
    }

    fn chromium_round(value: f64) -> f64 {
        if value > 359.0 { 0.0 } else { value }
    }

    #[test]
    fn the_value_it_packs_is_a_triple() {
        assert_eq!(triple(0.7, 0.12, 200.4), "0.7 0.12 200");
        assert_eq!(terms_of("0.7 0.12 200"), Some((0.7, 0.12, 200.0)));
        assert_eq!(terms_of(""), None, "nothing chosen yet is not hue zero");
        assert_eq!(terms_of("nonsense"), None);
    }
}

pub fn disc_(accrete: Disc, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let node_id = node_id(&accrete);
    let mut css = vec!["disc", "flex", "col"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let tag = accrete.tag.clone().filter(|t| !t.is_empty());
    let field = tag.as_ref().and_then(|t| ctx.loom.as_ref().and_then(|b| b.field(t)));
    let light = accrete.attrs.clone().unwrap_or_default().light;

    // What it shows now: the field's copy if there is one (a reset writes it too), else the node's own value.
    let initial = accrete
        .source
        .as_ref()
        .and_then(|s| s.default.as_ref())
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_default();
    let at = terms_of(&initial).map(|(_, chroma, hue)| (hue, chroma)).unwrap_or((0.0, 0.0));
    let spot = RwSignal::new(at);
    if let Some(signal) = field {
        Effect::new(move |_| {
            if let Some((_, chroma, hue)) = terms_of(signal.get().as_str().unwrap_or("")) {
                spot.set((hue, chroma));
            }
        });
    }

    let is_locked = {
        let locked_ctx = ctx.clone();
        move || locked(locked_ctx.loom.as_ref()).then_some("")
    };

    let pick = {
        let ctx = ctx.clone();
        let tag = tag.clone();
        move |ev: web_sys::MouseEvent| {
            let Some(target) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) else {
                return;
            };
            let rect = target.get_bounding_client_rect();
            let (cx, cy) = (rect.left() + rect.width() / 2.0, rect.top() + rect.height() / 2.0);
            let dx = f64::from(ev.client_x()) - cx;
            let dy = f64::from(ev.client_y()) - cy;
            let picked = (hue_at(dx, dy), chroma_at(dx, dy, rect.width() / 2.0));
            spot.set(picked);
            // A field has no exit of its own (ADR 0032 §2): it writes its loom's copy, and the loom sends.
            if let Some(signal) = field {
                signal.set(Value::String(triple(light, picked.1, picked.0)));
                // A click is already discreet: no window to wait for, whatever a neighbouring field asks for.
                field_changed(
                    &ctx,
                    ctx.loom.as_ref(),
                    tag.as_deref().unwrap_or_default(),
                    Some(accrete::Emit::IMMEDIATE),
                );
            }
        }
    };

    let background = face(light);
    let face_css = format!(
        "width: 100%; height: 100%; border-radius: 50%; background: {background}; \
         box-shadow: inset 0 0 0 1px var(--border);"
    );
    let thumb = "position: absolute; width: 12px; height: 12px; margin: -6px 0 0 -6px; border-radius: 50%; \
                 background: var(--bg); box-shadow: 0 0 0 2px var(--fg); pointer-events: none;"
        .to_string();

    div()
        .class(css)
        .attr("disabled", is_locked)
        .attr("id", node_id)
        .style("position: relative; width: 128px; height: 128px; cursor: crosshair;")
        .on(ev::click, pick)
        .child(div().style(face_css))
        .child(div().style(move || {
            let (hue, chroma) = spot.get();
            let (x, y) = place(hue, chroma);
            format!("{thumb} left: {}%; top: {}%;", 50.0 + x * 50.0, 50.0 + y * 50.0)
        }))
        .into_any()
}