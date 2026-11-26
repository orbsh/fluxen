use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{field_changed, locked, use_common_css, use_source_list, use_source_untracked};
use crate::rough;
use accrete::{AccreteOps, Dir, Picker, classify::Classify};
use leptos::ev;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::{Value, to_value};

/// A picker's menu (ADR 0031 §2): the rows come from a list plane, the current choice from `source`, and the choice
/// goes out through the enclosing `loom` (ADR 0032) when there is a `tag`.
///
/// ADR 0049 gives it three shapes, all from `attrs` and all off by default:
///
/// - `expand` — the rows fold behind the current choice and open on a click. Open and closed is this file's
///   own state: it is a gesture, not a value, so it never travels.
/// - `dir` — the rows run `row` (a row of buttons) or `col` (the default list).
/// - `multiple` — several keys may be chosen; the field then packs the list of them instead of one.
pub fn picker_(accrete: Picker, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let attrs = accrete.attrs.clone().unwrap_or_default();
    let folded = attrs.expand.unwrap_or(false);
    let sideways = matches!(attrs.dir, Some(Dir::Row) | Some(Dir::RowReverse));
    let multiple = attrs.multiple.unwrap_or(false);

    let mut css = vec!["picker", "flex"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let tag = accrete.tag.clone().filter(|t| !t.is_empty());
    let field = tag.as_ref().and_then(|t| ctx.loom.as_ref().and_then(|b| b.field(t)));

    // Initial value read untracked (same reason as `input_`: a component that emits must not subscribe to the
    // slot its own send writes).
    let initial = match field {
        Some(sig) => sig.get_untracked(),
        None => use_source_untracked(&ctx, &accrete).unwrap_or_else(|| Value::String(String::new())),
    };
    let chosen = RwSignal::new(keys_of(&initial));
    // The shown choice follows the loom's copy, so a reset shows up here too.
    if let Some(sig) = field {
        Effect::new(move |_| chosen.set(keys_of(&sig.get())));
    }
    let open = RwSignal::new(false);

    // A loom that used up its `sends` disables the select with its fields.
    let locked_ctx = ctx.clone();
    let is_locked = move || locked(locked_ctx.loom.as_ref()).then_some("");

    let layer = rough::drawn_border(&ctx, accrete.id.as_deref());
    let direction = if sideways {
        "flex-direction: row; align-items: center; gap: 4px;"
    } else {
        "flex-direction: column;"
    };

    let body = if folded {
        let toggle_ctx = ctx.clone();
        let toggle = move |_ev: web_sys::MouseEvent| {
            if !locked(toggle_ctx.loom.as_ref()) {
                open.set(!open.get_untracked());
            }
        };
        let close = move |_ev: web_sys::MouseEvent| open.set(false);

        let summary = {
            let ctx = ctx.clone();
            let accrete = accrete.clone();
            let tag = tag.clone();
            move || row_views(&ctx, &accrete, &tag, chosen, field, folded, multiple, open, true)
        };
        let menu = {
            let ctx = ctx.clone();
            let accrete = accrete.clone();
            let tag = tag.clone();
            // The fold is a box like any other: a surface with an edge and a shadow in the plain skin, and the
            // same box drawn by the pen under the skin (`is-border` + `is-shadow` are the stylesheet's handles
            // for exactly that swap). The positioning sits on a wrapper, because the skin gives `.is-border`
            // `position: relative` and an absolutely positioned menu has to stay where it was put.
            let place = "position: absolute; top: 100%; left: 0; z-index: 30; min-width: 100%; margin-top: 4px;";
            let box_style = format!(
                "padding: 4px; display: flex; {direction} background: var(--surface); \
                 border-radius: var(--round);"
            );
            move || {
                open.get().then(|| {
                    let ctx_box = ctx.clone();
                    div()
                        .style(place)
                        .child(
                            div()
                                .class("picker-menu is-border is-shadow")
                                .style(box_style.clone())
                                .child(row_views(
                                    &ctx, &accrete, &tag, chosen, field, folded, multiple, open, false,
                                ))
                                .child(rough::drawn_border(&ctx_box, None)),
                        )
                        .into_any()
                })
            }
        };

        div()
            .style("position: relative;")
            .child(
                div()
                    .class("picker-trigger flex row as-stretch")
                    .style("align-items: center; gap: 6px; cursor: pointer;")
                    .on(ev::click, toggle)
                    .child(summary)
                    .child(span().class("muted").child("▾")),
            )
            .child(
                move || {
                    open.get().then(|| {
                        div()
                            .class("picker-scrim")
                            .style("position: fixed; inset: 0; z-index: 20;")
                            .on(ev::click, close)
                            .into_any()
                    })
                },
            )
            .child(menu)
            .into_any()
    } else {
        // The plain form hangs the rows straight on the box: a wrapper would sit between the select and its
        // rows, and readers (a gate, a stylesheet) would have to reach around it.
        let ctx = ctx.clone();
        let accrete = accrete.clone();
        let tag = tag.clone();
        div()
            .child(move || row_views(&ctx, &accrete, &tag, chosen, field, folded, multiple, open, false))
            .into_any()
    };

    if folded {
        div()
            .class(css.clone())
            .style(direction.to_string())
            .attr("disabled", is_locked.clone())
            .child(body)
            .child(layer)
            .into_any()
    } else {
        // Unfolded: the rows are the box's children, as they were before ADR 0049 — no wrapper between the
        // select and its rows.
        let ctx = ctx.clone();
        let accrete = accrete.clone();
        let tag = tag.clone();
        div()
            .class(css.clone())
            .style(direction.to_string())
            .attr("disabled", is_locked.clone())
            .child(move || row_views(&ctx, &accrete, &tag, chosen, field, folded, multiple, open, false))
            .child(layer)
            .into_any()
    }
}

/// The option rows as view nodes: all of them, or (for a folded select's face) only the chosen ones.
#[allow(clippy::too_many_arguments)]
fn row_views(
    ctx: &Ctx,
    accrete: &Picker,
    tag: &Option<String>,
    chosen: RwSignal<Vec<String>>,
    field: Option<RwSignal<Value>>,
    folded: bool,
    multiple: bool,
    open: RwSignal<bool>,
    only_chosen: bool,
) -> Vec<AnyView> {
    let option = use_source_list(ctx, accrete).unwrap_or_default();
    let picked = chosen.get();
    let mut out: Vec<AnyView> = Vec::new();
    for (idx, child) in option.iter().enumerate() {
        let key = child.get_id().clone().unwrap_or_else(|| idx.to_string());
        let selected = picked.iter().any(|k| k == &key);
        if only_chosen && !selected {
            continue;
        }
        // Rows are shared, and `add_class` mutates: copy this row, not the whole list.
        let mut node = child.as_ref().clone();
        node.add_class("flex as-stretch");
        if selected {
            node.add_class("selected");
        }
        if only_chosen {
            // The face of a folded select is a readout, not a menu: it does not choose anything.
            out.push(div().child(render_accrete(ctx, &node)).into_any());
            continue;
        }
        let ctx_row = ctx.clone();
        let tag_row = tag.clone();
        let value = to_value(&key).unwrap_or_else(|_| Value::String(String::new()));
        out.push(
            div()
                .on(ev::click, move |ev| {
                    ev.stop_propagation();
                    let next = clicked(&chosen.get_untracked(), &key, multiple);
                    chosen.set(next.clone());
                    // A select with no tag keeps the choice to itself: a field has no exit (ADR 0032 §2), so a
                    // loom is what sends a choice anywhere.
                    if let Some(sig) = field {
                        sig.set(pack(&next, multiple));
                        // A click is already discreet: no window to wait for.
                        field_changed(
                            &ctx_row,
                            ctx_row.loom.as_ref(),
                            tag_row.as_deref().unwrap_or_default(),
                            Some(accrete::Emit::IMMEDIATE),
                        );
                    }
                    if folded && !multiple {
                        open.set(false);
                    }
                })
                .child(render_accrete(ctx, &node))
                .into_any(),
        );
    }
    if out.is_empty() && only_chosen {
        out.push(span().class("muted").child("—").into_any());
    }
    out
}

/// The keys inside a value: one key, or a list of them. Anything else is "nothing chosen".
pub fn keys_of(value: &Value) -> Vec<String> {
    match value {
        Value::String(one) if !one.is_empty() => vec![one.clone()],
        Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// What a set of keys packs: one key as a string, several as a list — the shape the field then carries.
pub fn pack(keys: &[String], multiple: bool) -> Value {
    if multiple {
        Value::Array(keys.iter().map(|k| Value::String(k.clone())).collect())
    } else {
        Value::String(keys.first().cloned().unwrap_or_default())
    }
}

/// The set after a click on `key`: one choice replaces, several toggle.
pub fn clicked(keys: &[String], key: &str, multiple: bool) -> Vec<String> {
    if !multiple {
        return vec![key.to_string()];
    }
    let mut next: Vec<String> = keys.to_vec();
    match next.iter().position(|k| k == key) {
        Some(at) => {
            next.remove(at);
        }
        None => next.push(key.to_string()),
    }
    next
}

#[cfg(test)]
mod picker_tests {
    use super::{clicked, keys_of, pack};
    use serde_json::{Value, json};

    #[test]
    fn a_click_replaces_or_toggles() {
        assert_eq!(clicked(&["a".into()], "b", false), vec!["b".to_string()]);
        assert_eq!(clicked(&["a".into()], "b", true), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(clicked(&["a".into(), "b".into()], "a", true), vec!["b".to_string()]);
    }

    #[test]
    fn several_choices_pack_as_a_list_and_one_as_a_key() {
        assert_eq!(pack(&["a".into(), "b".into()], true), json!(["a", "b"]));
        assert_eq!(pack(&["a".into(), "b".into()], false), json!("a"));
        assert_eq!(pack(&[], false), json!(""));
    }

    #[test]
    fn a_value_reads_back_into_keys() {
        assert_eq!(keys_of(&json!("a")), vec!["a"]);
        assert_eq!(keys_of(&json!(["a", "b"])), vec!["a", "b"]);
        assert_eq!(keys_of(&json!("")), Vec::<String>::new(), "nothing chosen");
        assert_eq!(keys_of(&Value::Null), Vec::<String>::new());
        assert_eq!(keys_of(&json!([1, "a"])), vec!["a"], "only keys count");
    }
}