use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{field_changed, locked, use_common_css, use_source_list, use_source_untracked};
use crate::rough;
use accrete::{AccreteOps, Select, classify::Classify};
use leptos::html::*;
use leptos::prelude::*;
use serde_json::{Value, to_value};

/// A select (ADR 0031 §2): options come from `options` (the list plane), the
/// current value from `source`, the choice goes out through `sink`.
///
/// A `tag` makes it a field of the enclosing bundle (ADR 0032): the choice lands
/// in the bundle's signal and the bundle owns the exit and the timing. No tag =
/// no uplink (ADR 0032 §2 revised): a `local` sink still lands the choice on the page, and that is all.
pub fn select_(accrete: Select, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["select", "flex"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let field = accrete
        .tag
        .as_ref()
        .filter(|t| !t.is_empty())
        .and_then(|tag| ctx.bundle.as_ref().and_then(|b| b.field(tag)));

    // Initial value read untracked (same reason as `input_`: a component that
    // emits must not subscribe to the slot its own send writes).
    let initial = match field {
        Some(sig) => sig.get_untracked(),
        None => {
            use_source_untracked(&ctx, &accrete).unwrap_or_else(|| Value::String(String::new()))
        }
    };
    let current = RwSignal::new(initial.as_str().map(String::from).unwrap_or_default());

    // The shown choice follows the bundle's copy, so a reset shows up here too (same rule as `input_`).
    if let Some(sig) = field {
        Effect::new(move |_| {
            current.set(sig.get().as_str().map(String::from).unwrap_or_default());
        });
    }

    // A bundle that used up its `sends` disables the select with its fields.
    let locked_ctx = ctx.clone();
    let is_locked = move || locked(locked_ctx.bundle.as_ref()).then_some("");

    // The rows are rebuilt on every choice, so they hang inside the box rather than being the box: the layer
    // is a child of it and stays put while a choice re-picks the rows (ADR 0038 §4).
    let layer = rough::drawn_border(&ctx, accrete.id.as_deref());
    let rows_ctx = ctx.clone();
    let rows = move || {
        let ctx = rows_ctx.clone();
        let option = use_source_list(&ctx, &accrete).unwrap_or_default();
        let children = option.iter().enumerate().map(|(idx, child)| {
            let key = child.get_id().clone().unwrap_or(idx.to_string());
            let selected = current.get() == key;
            // Rows are shared, and `add_class` mutates: copy this row, not the
            // whole list.
            let mut option = child.as_ref().clone();
            option.add_class("flex as-stretch");
            if selected {
                option.add_class("selected");
                div().child(render_accrete(&ctx, &option)).into_any()
            } else {
                let v = to_value(&key).unwrap_or_else(|_| Value::String(String::new()));
                let c = current;
                let click_ctx = ctx.clone();
                div()
                    .on(leptos::ev::click, move |_| {
                        // A select with no tag keeps the choice to itself: a field has no exit
                        // (ADR 0032 §2), so a bundle is what sends a choice anywhere.
                        if let Some(sig) = field {
                            sig.set(v.clone());
                            field_changed(&click_ctx, click_ctx.bundle.as_ref());
                        }
                        if let Some(s) = v.as_str() {
                            c.set(s.to_string());
                        }
                    })
                    .child(render_accrete(&ctx, &option))
                    .into_any()
            }
        });
        Vec::from_iter(children)
    };

    div()
        .class(css.clone())
        .attr("disabled", is_locked.clone())
        .child(rows)
        .child(layer)
        .into_any()
}
