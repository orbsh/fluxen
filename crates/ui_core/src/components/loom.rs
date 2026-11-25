use crate::Ctx;
use crate::ctx::render_children;
use crate::hooks::{LoomState, Field, loom_key, node_id, refresh, use_common_css};
use crate::rough;
use accrete::{Accrete, AccreteOps, Loom, JsType};
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;
use std::sync::Arc;

/// A field's initial value: `source.default` (what it shows), in the shape the field's own `kind` names.
fn field_default(node: &Accrete) -> Value {
    let kind = match node {
        Accrete::input(i) => i.kind.clone(),
        _ => None,
    };
    let default = node.get_source().and_then(|b| b.default.clone());
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

/// What this loom packs: one key per tagged field, and one key per nested
/// loom. The walk STOPS at a nested loom (ADR 0032 §4) — its fields belong
/// to it, and the outer object sees only its packed value. That is what makes
/// `outer(submit: [enter]) > inner(submit: [change], tag: mention) > input(tag: q)`
/// a single `mention` key instead of a flattened `q`.
fn collect(node: &Accrete, out: &mut Vec<Field>) {
    match node {
        Accrete::loom(b) => {
            if let Some(name) = loom_key(&b.tag)
                && !out.iter().any(|f| f.name == name)
            {
                // The value arrives from the child's own pack, through the
                // shared `mirror` signal — not from a field here.
                out.push(Field::new(
                    name,
                    RwSignal::new(Value::Object(serde_json::Map::new())),
                    Value::Object(serde_json::Map::new()),
                ));
            }
        }
        Accrete::input(i) => {
            if let Some(tag) = i.tag.as_ref().filter(|t| !t.is_empty())
                && !out.iter().any(|f| &f.name == tag)
            {
                let initial = field_default(node);
                out.push(Field::new(tag.clone(), RwSignal::new(initial.clone()), initial));
            }
        }
        Accrete::select(s) => {
            if let Some(tag) = s.tag.as_ref().filter(|t| !t.is_empty())
                && !out.iter().any(|f| &f.name == tag)
            {
                let initial = field_default(node);
                out.push(Field::new(tag.clone(), RwSignal::new(initial.clone()), initial));
            }
        }
        Accrete::disc(d) => {
            if let Some(tag) = d.tag.as_ref().filter(|t| !t.is_empty())
                && !out.iter().any(|f| &f.name == tag)
            {
                // A disc packs a triple (text), never a pair of numbers: its value is the colour.
                let initial = field_default(node);
                out.push(Field::new(tag.clone(), RwSignal::new(initial.clone()), initial));
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

/// A loom packs a group of fields into one object and sends it (ADR 0032).
/// It owns two things: where the packed object goes (`sink`) and when it leaves
/// (`attrs.submit`). Fields are named by their own `tag`; the loom's `id` is
/// this submission's instance, not a field name.
pub fn loom_(accrete: Loom, ctx: &Ctx) -> AnyView {
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

    // A nested loom's packed object lands in the parent's slot for its key.
    let parent = ctx.loom.clone();
    let mirror =
        loom_key(&accrete.tag).and_then(|key| parent.as_ref().and_then(|p| p.field(&key)));

    let attrs = accrete.attrs.clone().unwrap_or_default();
    let sends = attrs.sends;
    let state = Arc::new(LoomState {
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
    // A nested loom joins its parent's list, so a reset can travel down the tree a pack travels up.
    if let Some(p) = &parent {
        p.children
            .update_value(|kids| kids.push(std::sync::Arc::clone(&state)));
    }
    // Publish the initial object: an outer loom must be able to pack this one
    // before anything was typed in it.
    refresh(&state);

    // Inject and clone down: fields and nested looms read their loom from
    // the ctx they were rendered with, so ownership does not depend on timing.
    let ctx = Ctx {
        loom: Some(state),
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

#[cfg(test)]
mod tests {
    use super::collect;
    use accrete::Accrete;
    use serde_json::json;

    /// The names an outer loom would pack: `collect` walks the outer's children, exactly as `loom_` does.
    fn names(v: serde_json::Value) -> Vec<String> {
        let a: Accrete = serde_json::from_value(v).expect("valid accrete");
        let Accrete::loom(outer) = &a else {
            panic!("expected loom")
        };
        let mut out = Vec::new();
        for c in outer.children.as_deref().unwrap_or_default() {
            collect(c, &mut out);
        }
        out.into_iter().map(|f| f.name).collect()
    }

    #[test]
    fn a_tagged_nested_loom_is_one_key_and_the_walk_stops() {
        // The outer object keeps the group as one object (ADR 0032 §4), it does not flatten the inner fields.
        assert_eq!(
            names(json!({
                "type": "loom",
                "children": [{
                    "type": "loom",
                    "tag": "mention",
                    "children": [{"type": "input", "tag": "q"}]
                }]
            })),
            vec!["mention".to_string()]
        );
    }

    #[test]
    fn an_untagged_nested_loom_packs_nothing_into_its_parent() {
        // `id` is identity and `dest` is where the inner sends, so neither names it here: without a `tag` the
        // group is its own, and the parent's object does not move because someone gave it an id.
        assert!(
            names(json!({
                "type": "loom",
                "children": [{
                    "type": "loom",
                    "id": "login-42",
                    "sink": {"dest": "notes", "scope": "local"},
                    "children": [{"type": "input", "tag": "q"}]
                }]
            }))
            .is_empty()
        );
    }
}
