//! Template expansion (ADR 0006): `Accrete::expand` substitutes `{{key}}`
//! slots in registered JSON bodies with the data values' JSON
//! serialization, then parses the result back into Accrete. No template
//! language: producers build loop/conditional fragments in `data`.

use accrete::Accrete;
use accrete::AccreteOps;
use accrete::template::{Templates, subst};
use serde_json::{Value, json};

fn accrete(js: Value) -> Accrete {
    serde_json::from_value(js).expect("valid accrete json")
}

fn default_of(b: &Accrete) -> Option<Value> {
    b.get_bind()
        .and_then(|m| m.get("value"))
        .and_then(|x| x.default.clone())
}

#[test]
fn expand_replaces_template_with_substituted_accrete() {
    let mut env = Templates::new();
    env.insert(
        "greet".into(),
        json!({
            "type": "text",
            "bind": { "value": { "kind": "default", "default": "hi {{ name }}" } }
        })
        .to_string(),
    );

    let mut b = accrete(json!({
        "type": "template",
        "name": "greet",
        "data": { "name": "ada" }
    }));
    assert!(b.expand(&env).is_empty());

    assert_eq!(b.get_type(), "Accrete :: text");
    assert_eq!(default_of(&b), Some(json!("hi ada")));
}

#[test]
fn unregistered_name_warns_and_keeps_node() {
    let env = Templates::new();
    let mut b = accrete(json!({
        "type": "template", "name": "ghost", "data": {}
    }));
    let warns = b.expand(&env);
    assert_eq!(warns.len(), 1);
    assert!(warns[0].contains("not registered"));
    assert_eq!(b.get_type(), "Accrete :: template");
}

#[test]
fn invalid_result_warns_and_keeps_node() {
    // Producer forgot to quote a bare slot inside a JSON string context:
    // substituted body is not valid Accrete — node survives untouched.
    let mut env = Templates::new();
    env.insert("broken".into(), "{\"type\": \"text\", \"children\": {{ xs }}".into());

    let mut b = accrete(json!({
        "type": "template", "name": "broken", "data": { "xs": "not-json-here" }
    }));
    let warns = b.expand(&env);
    assert_eq!(warns.len(), 1);
    assert!(warns[0].contains("invalid"));
    assert_eq!(b.get_type(), "Accrete :: template");
}

#[test]
fn slot_carries_json_fragment_from_producer() {
    // The {% for %} replacement: producer builds the children array
    // fragment, the template just splices it in (no quotes around the
    // slot — the value serializes as JSON).
    let mut env = Templates::new();
    env.insert(
        "wrap".into(),
        r#"{"type": "case", "children": {{kids}}, "id": "{{id}}"}"#.into(),
    );

    let mut b = accrete(json!({
        "type": "template",
        "name": "wrap",
        "data": {
            "kids": [{"type": "text"}, {"type": "text"}],
            "id": "r7"
        }
    }));
    assert!(b.expand(&env).is_empty());

    assert_eq!(b.get_type(), "Accrete :: case");
    assert_eq!(b.get_id().as_deref(), Some("r7"));
    assert_eq!(b.borrow_children().unwrap().len(), 2);
}

#[test]
fn expand_recurses_into_children() {
    let mut env = Templates::new();
    env.insert(
        "lbl".into(),
        json!({
            "type": "text",
            "bind": { "value": { "kind": "default", "default": "{{t}}" } }
        })
        .to_string(),
    );

    let mut b = accrete(json!({
        "type": "case",
        "children": [
            { "type": "template", "name": "lbl", "data": { "t": "one" } },
            { "type": "template", "name": "lbl", "data": { "t": "two" } }
        ]
    }));
    assert!(b.expand(&env).is_empty());

    let kids = b.borrow_children().unwrap();
    assert_eq!(default_of(&kids[0]), Some(json!("one")));
    assert_eq!(default_of(&kids[1]), Some(json!("two")));
}

#[test]
fn subst_handles_spaced_and_tight_slots() {
    // `{{ k }}` and `{{k}}` are the same slot.
    let data: serde_json::Map<String, Value> =
        serde_json::from_value(json!({ "k": "v" })).unwrap();
    let a = subst(r#"{"type": "text", "id": "{{ k }}"}"#, &data).unwrap();
    assert_eq!(a.get_id().as_deref(), Some("v"));
    let b = subst(r#"{"type": "text", "id": "{{k}}"}"#, &data).unwrap();
    assert_eq!(b.get_id().as_deref(), Some("v"));
}
