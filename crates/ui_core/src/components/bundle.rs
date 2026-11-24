use crate::Ctx;
use crate::ctx::render_children;
use crate::hooks::{BundleState, Field, bundle_key, node_id, refresh, use_common_css};
use crate::rough;
use accrete::{Accrete, AccreteOps, Bundle, JsType};
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;
use std::sync::Arc;

/// A field's initial value: `source.default` (what it shows) then
/// `sink.default`, typed by whichever channel names a type.
fn field_default(node: &Accrete) -> Value {
    let kind = node
        .get_sink()
        .and_then(|b| b.r#type.clone())
        .or_else(|| node.get_source().and_then(|b| b.r#type.clone()));
    let default = node
        .get_source()
        .and_then(|b| b.default.clone())
        .or_else(|| node.get_sink().and_then(|b| b.default.clone()));
    // serde_json has an infallible constructor for every JSON scalar, so the
    // default never needs a `to_value(..).unwrap()` round trip.
    match kind {
        Some(JsType::number) => {
            Value::from(default.as_ref().and_then(|x| x.as_f64()).unwrap_or(0.0))
        }
        Some(JsType::bool) => {
            Value::Bool(default.as_ref().and_then(|x| x.as_bool()).unwrap_or(false))
        }
        _ => Value::String(
            default
                .as_ref()
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        ),
    }
}

/// What this bundle packs: one key per tagged field, and one key per nested
/// bundle. The walk STOPS at a nested bundle (ADR 0032 §4) — its fields belong
/// to it, and the outer object sees only its packed value. That is what makes
/// `outer(submit: [enter]) > inner(submit: [change], tag: mention) > input(tag: q)`
/// a single `mention` key instead of a flattened `q`.
fn collect(node: &Accrete, out: &mut Vec<Field>) {
    match node {
        Accrete::bundle(b) => {
            if let Some(name) = bundle_key(&b.tag, &b.id)
                && !out.iter().any(|f| f.name == name)
            {
                // The value arrives from the child's own pack, through the
                // shared `mirror` signal — not from a field here.
                out.push(Field {
                    name,
                    sig: RwSignal::new(Value::Object(serde_json::Map::new())),
                    initial: Value::Object(serde_json::Map::new()),
                });
            }
        }
        Accrete::input(i) => {
            if let Some(tag) = i.tag.as_ref().filter(|t| !t.is_empty())
                && !out.iter().any(|f| &f.name == tag)
            {
                let initial = field_default(node);
                out.push(Field {
                    name: tag.clone(),
                    sig: RwSignal::new(initial.clone()),
                    initial,
                });
            }
        }
        Accrete::select(s) => {
            if let Some(tag) = s.tag.as_ref().filter(|t| !t.is_empty())
                && !out.iter().any(|f| &f.name == tag)
            {
                let initial = field_default(node);
                out.push(Field {
                    name: tag.clone(),
                    sig: RwSignal::new(initial.clone()),
                    initial,
                });
            }
        }
        _ => {
            if let Some(subs) = node.borrow_children() {
                for c in subs {
                    collect(c, out);
                }
            }
        }
    }
}

/// A bundle packs a group of fields into one object and sends it (ADR 0032).
/// It owns two things: where the packed object goes (`sink`) and when it leaves
/// (`attrs.submit`). Fields are named by their own `tag`; the bundle's `id` is
/// this submission's instance, not a field name.
pub fn bundle_(accrete: Bundle, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let node_id = node_id(&accrete);
    let mut css = vec!["case", "flex"];
    if let Some(id) = &accrete.id {
        css.push(id.as_str());
    }
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let mut fields = Vec::new();
    if let Some(subs) = accrete.children.as_deref() {
        for c in subs {
            collect(c, &mut fields);
        }
    }

    // A nested bundle's packed object lands in the parent's slot for its key.
    let parent = ctx.bundle.clone();
    let mirror = bundle_key(&accrete.tag, &accrete.id)
        .and_then(|key| parent.as_ref().and_then(|p| p.field(&key)));

    let attrs = accrete.attrs.clone().unwrap_or_default();
    let sends = attrs.sends;
    let state = Arc::new(BundleState {
        fields,
        modes: attrs.submit.clone(),
        reset: attrs.reset,
        sends,
        // `-1` and "nothing said" are the same thing: no count to take from.
        remaining: RwSignal::new(sends.filter(|n| *n >= 0).unwrap_or(-1)),
        children: StoredValue::new(Vec::new()),
        sink: accrete.get_sink().cloned(),
        id: node_id,
        parent: parent.clone(),
        mirror,
    });
    // A nested bundle joins its parent's list, so a reset can travel down the tree a pack travels up.
    if let Some(p) = &parent {
        p.children
            .update_value(|kids| kids.push(std::sync::Arc::clone(&state)));
    }
    // Publish the initial object: an outer bundle must be able to pack this one
    // before anything was typed in it.
    refresh(&state);

    // Inject and clone down: fields and nested bundles read their bundle from
    // the ctx they were rendered with, so ownership does not depend on timing.
    let ctx = Ctx {
        bundle: Some(state),
        ..ctx
    };
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(&ctx, s))
        .unwrap_or_default();

    // The sketch skin (ADR 0038 §4): only a container that asked for a border has this layer.
    let layer = rough::border_layer(
        &ctx,
        accrete.attrs.as_ref().and_then(|a| a.class.as_ref()),
        accrete.id.as_deref(),
    );
    div()
        .class(css.as_str())
        .child(children)
        .child(layer)
        .into_any()
}
