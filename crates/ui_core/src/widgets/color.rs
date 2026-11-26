//! A colour field (ADR 0052): a swatch, the `oklch` literal beside it, and a panel of three sliders behind
//! them. It replaces ADR 0048's disc — same value, same field rules, a gesture that costs one line instead of
//! 128 pixels.
//!
//! The value is the written-out triple `"<light> <chroma> <hue>"`, exactly what colour members already store,
//! so the picker and the value stay one thing and nothing downstream knows a picker was involved. Nothing here
//! computes colour either: the swatch is `oklch()` handed to the browser, the same reason `select` renders its
//! options as nodes instead of computing a menu.
//!
//! Opening: hover on a pointer device, click, or focus — ADR 0026's lesson is that state must not live only in
//! `:hover`. What opens is the WHOLE block (header included), not a menu hung under a static header; the header
//! inside the panel is the same swatch and literal, so it reads as the field lifting off the page.

use crate::Ctx;
use crate::hooks::{field_changed, locked, node_id, use_common_css};
use crate::rough;
use accrete::Color;
use leptos::ev;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

/// Lightness a field starts at when it has no value yet: mid grey, one term already in the panel.
pub const DEFAULT_LIGHT: f64 = 0.70;
/// The most chroma the panel offers — the same cap a colour member is validated against, so the slider and the
/// validator agree about how far "saturated" goes.
pub const MAX_CHROMA: f64 = 0.4;

/// The three rows of the panel: name, range, and step. Lightness and chroma are fractions, hue is degrees.
const ROWS: [(&str, f64, f64, f64); 3] = [
    ("L", 0.0, 1.0, 0.01),
    ("C", 0.0, MAX_CHROMA, 0.01),
    ("H", 0.0, 360.0, 1.0),
];

fn num(index: usize, value: f64) -> String {
    match index {
        2 => format!("{}", value.round() as i64),
        _ => format!("{value:.2}"),
    }
}

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

/// What the field packs: the written-out triple, rounded to what the panel shows.
pub fn triple(light: f64, chroma: f64, hue: f64) -> String {
    format!("{} {} {}", num(0, light), num(1, chroma), num(2, hue))
}

/// The literal the page can paste into CSS: the same three numbers, wrapped once.
pub fn literal(light: f64, chroma: f64, hue: f64) -> String {
    format!(
        "oklch({} {} {})",
        num(0, light),
        num(1, chroma),
        num(2, hue)
    )
}

fn chip_style(has_value: bool, light: f64, chroma: f64, hue: f64) -> String {
    let base = "width: 16px; height: 16px; border-radius: 4px; flex: 0 0 auto;";
    if has_value {
        format!("{base} background: {};", literal(light, chroma, hue))
    } else {
        format!("{base} box-shadow: inset 0 0 0 1px var(--border);")
    }
}

/// The header: the swatch and the literal. It appears twice — once in the page, once inside the open panel —
/// and both read the same signals, so the panel really is the whole block lifting rather than a menu.
fn header_view(terms: RwSignal<(f64, f64, f64)>, has_value: RwSignal<bool>) -> AnyView {
    div()
        .class("color-head flex row")
        .style("align-items: center; gap: 6px; min-height: 20px;")
        .child(move || {
            let (l, c, h) = terms.get();
            div().style(chip_style(has_value.get(), l, c, h)).into_any()
        })
        .child(
            span()
                .class("color-literal")
                .style(
                    "font-family: var(--mono, ui-monospace, monospace); font-size: 12px; \
                     user-select: text; cursor: text; white-space: nowrap;",
                )
                .child(move || {
                    let (l, c, h) = terms.get();
                    if has_value.get() {
                        literal(l, c, h)
                    } else {
                        "—".to_string()
                    }
                }),
        )
        .into_any()
}

/// One panel row: the term's name, its slider, and a box that follows it.
#[allow(clippy::too_many_arguments)]
fn row_view(
    ctx: &Ctx,
    accrete: &Color,
    tag: &Option<String>,
    field: Option<RwSignal<Value>>,
    terms: RwSignal<(f64, f64, f64)>,
    has_value: RwSignal<bool>,
    texts: [RwSignal<String>; 3],
    index: usize,
) -> AnyView {
    let (name, min, max, step) = ROWS[index];
    let text_sig = texts[index];
    let write = {
        let ctx = ctx.clone();
        let tag = tag.clone();
        let window = accrete.emission();
        move |l: f64, c: f64, h: f64| {
            if let Some(sig) = field {
                sig.set(Value::String(triple(l, c, h)));
                // A field has no exit of its own (ADR 0032 §2): it writes its loom's copy, and the loom sends.
                // Dragging is continuous, so the window is the field's own clock, not one frame per event.
                field_changed(
                    &ctx,
                    ctx.loom.as_ref(),
                    tag.as_deref().unwrap_or_default(),
                    Some(window),
                );
            }
        }
    };

    // Slider and number box share this: `rewrite_text` is false while someone is typing, because a half-typed
    // `0.` must not be replaced by the rounded value under their cursor.
    let put = {
        let write = write.clone();
        move |value: f64, rewrite_text: bool| {
            let (mut l, mut c, mut h) = terms.get_untracked();
            match index {
                0 => l = value,
                1 => c = value,
                _ => h = value.rem_euclid(360.0),
            }
            terms.set((l, c, h));
            has_value.set(true);
            if rewrite_text {
                text_sig.set(num(index, value));
            }
            write(l, c, h);
        }
    };

    div()
        .class("color-row flex row")
        .style("align-items: center; gap: 6px;")
        .child(
            span()
                .class("muted")
                .style("width: 12px; font-size: 11px;")
                .child(name),
        )
        .child(
            input()
                .attr("type", "range")
                .attr("min", min.to_string())
                .attr("max", max.to_string())
                .attr("step", step.to_string())
                .style("flex: 1 1 auto; min-width: 90px;")
                .prop("value", move || {
                    let (l, c, h) = terms.get();
                    num(index, [l, c, h][index])
                })
                .on(ev::input, {
                    let put = put.clone();
                    move |e| {
                        if let Ok(value) = input_value(&e).parse::<f64>() {
                            put(value, true);
                        }
                    }
                }),
        )
        .child(
            input()
                .attr("type", "number")
                .attr("min", min.to_string())
                .attr("max", max.to_string())
                .attr("step", step.to_string())
                .style("width: 62px; font-family: var(--mono, ui-monospace, monospace); font-size: 12px;")
                .prop("value", move || text_sig.get())
                .on(ev::input, {
                    let put = put.clone();
                    move |e| {
                        // The text is the browser's while someone types; only a parseable value moves the
                        // sliders and the swatch.
                        let raw = input_value(&e);
                        text_sig.set(raw.clone());
                        if let Ok(value) = raw.parse::<f64>() {
                            put(value, false);
                        }
                    }
                }),
        )
        .into_any()
}

pub fn color_(accrete: Color, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let node_id = node_id(&accrete);
    let mut css = vec!["color", "flex", "row"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let tag = accrete.tag.clone().filter(|t| !t.is_empty());
    let field = tag
        .as_ref()
        .and_then(|t| ctx.loom.as_ref().and_then(|b| b.field(t)));

    // Initial value read untracked (same reason as `input_`): a component that emits must not subscribe to the
    // slot its own send writes, or every change would rebuild its own subtree.
    let initial = match field {
        Some(sig) => sig.get_untracked(),
        None => accrete
            .source
            .as_ref()
            .and_then(|s| s.default.clone())
            .unwrap_or(Value::Null),
    };
    let start = initial.as_str().and_then(terms_of);
    let seed = start.unwrap_or((DEFAULT_LIGHT, 0.0, 0.0));

    let terms = RwSignal::new(seed);
    let has_value = RwSignal::new(start.is_some());
    let texts = [
        RwSignal::new(num(0, seed.0)),
        RwSignal::new(num(1, seed.1)),
        RwSignal::new(num(2, seed.2)),
    ];
    let open = RwSignal::new(false);
    let pinned = RwSignal::new(false);

    // The shown value follows the loom's copy, so a reset lands here too.
    if let Some(sig) = field {
        Effect::new(move |_| {
            let value = sig.get();
            let text = value.as_str().unwrap_or("");
            match terms_of(text) {
                Some((l, c, h)) => {
                    terms.set((l, c, h));
                    texts[0].set(num(0, l));
                    texts[1].set(num(1, c));
                    texts[2].set(num(2, h));
                    has_value.set(true);
                }
                None if text.is_empty() => has_value.set(false),
                None => {}
            }
        });
    }

    let is_locked = {
        let ctx = ctx.clone();
        move || locked(ctx.loom.as_ref()).then_some("")
    };

    // Open on hover (pointer devices), on click, and on focus; a click pins it so the pointer can travel into
    // the panel without it closing.
    let hover_in = {
        let ctx = ctx.clone();
        move |_| {
            if !locked(ctx.loom.as_ref()) {
                open.set(true);
            }
        }
    };
    let hover_out = move |_| {
        if !pinned.get_untracked() {
            open.set(false);
        }
    };
    let toggle_pin = {
        let ctx = ctx.clone();
        move |_| {
            if locked(ctx.loom.as_ref()) {
                return;
            }
            let next = !pinned.get_untracked();
            pinned.set(next);
            open.set(next || open.get_untracked());
        }
    };
    let close = move || {
        pinned.set(false);
        open.set(false);
    };

    let panel = {
        let ctx = ctx.clone();
        let accrete = accrete.clone();
        let tag = tag.clone();
        move || {
            open.get().then(|| {
                let rows: Vec<AnyView> = (0..3)
                    .map(|i| row_view(&ctx, &accrete, &tag, field, terms, has_value, texts, i))
                    .collect();
                let close_esc = close;
                div()
                    .style("position: absolute; top: 0; left: 0; z-index: 30;")
                    .attr("tabindex", "0")
                    .on(ev::keydown, move |e: web_sys::KeyboardEvent| {
                        if e.key() == "Escape" {
                            close_esc();
                        }
                    })
                    .child(
                        div()
                            .class("color-panel is-border is-shadow")
                            .style(
                                "display: flex; flex-direction: column; gap: 6px; padding: 8px; \
                                 min-width: 240px; background: var(--surface); border-radius: var(--round);",
                            )
                            .child(header_view(terms, has_value))
                            .child(
                                div()
                                    .class("color-rows")
                                    .style("display: flex; flex-direction: column; gap: 4px;")
                                    .child(rows),
                            )
                            .child(rough::drawn_border(&ctx, None)),
                    )
                    .into_any()
            })
        }
    };

    let scrim = move || {
        open.get().then(|| {
            div()
                .class("color-scrim")
                .style("position: fixed; inset: 0; z-index: 20;")
                .on(ev::click, move |_| close())
                .into_any()
        })
    };

    let face_layer = rough::drawn_border(&ctx, None);
    div()
        .class(css)
        .attr("disabled", is_locked)
        .attr("id", node_id)
        .style("position: relative; align-items: center; gap: 6px; cursor: pointer;")
        .on(ev::mouseenter, hover_in)
        .on(ev::mouseleave, hover_out)
        .on(ev::click, toggle_pin)
        .child(
            div()
                .class("color-face")
                .style("position: relative;")
                .child(header_view(terms, has_value))
                .child(face_layer),
        )
        .child(scrim)
        .child(panel)
        .into_any()
}

/// The text an input event carries, whichever input it came from.
fn input_value(e: &web_sys::Event) -> String {
    e.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.value())
        .unwrap_or_default()
}

#[cfg(test)]
mod color_tests {
    use super::{MAX_CHROMA, literal, terms_of, triple};

    #[test]
    fn the_value_it_packs_is_a_triple() {
        assert_eq!(triple(0.7, 0.12, 200.4), "0.70 0.12 200");
        assert_eq!(terms_of("0.70 0.12 200"), Some((0.7, 0.12, 200.0)));
        assert_eq!(terms_of(""), None, "nothing chosen yet is not hue zero");
        assert_eq!(terms_of("nonsense"), None);
    }

    #[test]
    fn the_literal_is_the_value_wrapped_once() {
        assert_eq!(literal(0.7, 0.12, 200.4), "oklch(0.70 0.12 200)");
        assert_eq!(literal(1.0, MAX_CHROMA, 0.0), "oklch(1.00 0.40 0)");
    }

    #[test]
    fn a_hue_outside_the_wheel_comes_back_inside_it() {
        assert_eq!(terms_of("0.5 0.1 400"), Some((0.5, 0.1, 40.0)));
        assert_eq!(terms_of("0.5 0.1 -40"), Some((0.5, 0.1, 320.0)));
    }
}
