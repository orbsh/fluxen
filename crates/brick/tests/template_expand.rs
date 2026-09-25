//! Template expansion: `Brick::expand` renders minijinja templates stored
//! by `Content::Tmpl` into concrete Brick trees (the AI-side abstraction
//! for reusable layout fragments).

use brick::Brick;
use brick::BrickOps;

use minijinja::Environment;
use serde_json::{Value, json};

fn brick(js: Value) -> Brick {
    serde_json::from_value(js).expect("valid brick json")
}

fn default_of(b: &Brick) -> Option<Value> {
    b.get_bind()
        .and_then(|m| m.get("value"))
        .and_then(|x| x.default.clone())
}

#[test]
fn expand_replaces_template_with_rendered_brick() {
    let mut env = Environment::new();
    env.add_template_owned(
        "greet".to_string(),
        json!({
            "type": "text",
            "bind": { "value": { "kind": "default", "default": "hi {{ name }}" } }
        })
        .to_string(),
    )
    .expect("add template");

    let mut b = brick(json!({
        "type": "template",
        "name": "greet",
        "data": { "name": "ada" }
    }));
    b.expand(&env);

    assert_eq!(b.get_type(), "Brick :: text");
    assert_eq!(default_of(&b), Some(json!("hi ada")));
}

#[test]
fn expand_leaves_unresolvable_template_untouched() {
    // Missing template: expand fails silently, brick keeps its shape.
    let env = Environment::new();
    let mut b = brick(json!({
        "type": "template", "name": "ghost", "data": {}
    }));
    b.expand(&env);
    assert_eq!(b.get_type(), "Brick :: template");
}

#[test]
fn expand_recurses_into_children() {
    let mut env = Environment::new();
    env.add_template_owned(
        "lbl".to_string(),
        json!({
            "type": "text",
            "bind": { "value": { "kind": "default", "default": "{{ t }}" } }
        })
        .to_string(),
    )
    .unwrap();

    let mut b = brick(json!({
        "type": "case",
        "children": [
            { "type": "template", "name": "lbl", "data": { "t": "one" } },
            { "type": "template", "name": "lbl", "data": { "t": "two" } }
        ]
    }));
    b.expand(&env);

    let kids = b.borrow_children().unwrap();
    assert_eq!(default_of(&kids[0]), Some(json!("one")));
    assert_eq!(default_of(&kids[1]), Some(json!("two")));
}
