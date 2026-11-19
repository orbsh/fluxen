//! Stamps (ADR 0037): a definition is a node tree, parsed and checked once at
//! registration; a value that is exactly `"$name"` is a variable and is replaced
//! whole. Nothing is interpolated into a JSON string, so a value with a quote in it
//! is just a value.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use accrete::Accrete;
use accrete::AccreteOps;
use accrete::template::Templates;
use serde_json::{Value, json};

fn accrete(js: Value) -> Accrete {
    serde_json::from_value(js).expect("valid accrete json")
}

fn default_of(b: &Accrete) -> Option<Value> {
    b.get_source().and_then(|x| x.default.clone())
}

fn stamp(name: &str, data: Value) -> Accrete {
    accrete(json!({ "type": "stamp", "name": name, "data": data }))
}

#[test]
fn whole_value_becomes_the_supplied_value() {
    let mut env = Templates::new();
    env.register("label", &["q"], &json!({
        "type": "text",
        "source": { "default": "$q" }
    }))
    .expect("registers");

    let mut b = stamp("label", json!({ "q": "42" }));
    assert!(b.expand(&env).is_empty(), "no warnings");

    assert_eq!(b.get_type(), "Accrete :: text");
    assert_eq!(default_of(&b), Some(json!("42")));
}

#[test]
fn a_value_with_a_quote_in_it_is_just_a_value() {
    // The old text form spliced this into `"..."` and broke the JSON.
    let mut env = Templates::new();
    env.register("label", &["q"], &json!({
        "type": "text",
        "source": { "default": "$q" }
    }))
    .expect("registers");

    let mut b = stamp("label", json!({ "q": "she said \"hi\"" }));
    assert!(b.expand(&env).is_empty());
    assert_eq!(default_of(&b), Some(json!("she said \"hi\"")));
}

#[test]
fn a_variable_inside_a_sentence_is_a_literal() {
    // A variable is the WHOLE value: text around it means the string is literal.
    let mut env = Templates::new();
    env.register("count", &[], &json!({
        "type": "text",
        "source": { "default": "共 $q 件" }
    }))
    .expect("`$q` inside a sentence is not a hole, so nothing is declared");

    // …and declaring it would be a lie: nothing in the tree uses it.
    env.register("count2", &["q"], &json!({
        "type": "text",
        "source": { "default": "共 $q 件" }
    }))
    .expect_err("declaring `q` when nothing uses it is refused");
}

#[test]
fn doubled_dollar_escapes_a_literal_that_starts_with_dollar() {
    let mut env = Templates::new();
    env.register("money", &[], &json!({
        "type": "text",
        "source": { "default": "$$amount" }
    }))
    .expect("`$$amount` is a literal, not a variable");

    let mut b = stamp("money", json!({}));
    assert!(b.expand(&env).is_empty());
    assert_eq!(default_of(&b), Some(json!("$amount")));
}

#[test]
fn a_variable_where_children_go_takes_a_list_of_nodes() {
    let mut env = Templates::new();
    env.register("box", &["kids"], &json!({
        "type": "frame",
        "children": "$kids"
    }))
    .expect("registers");

    let mut b = stamp(
        "box",
        json!({ "kids": [{ "type": "text" }, { "type": "text" }] }),
    );
    assert!(b.expand(&env).is_empty());
    assert_eq!(b.get_type(), "Accrete :: frame");
    assert_eq!(b.borrow_children().unwrap().len(), 2);
}

#[test]
fn a_node_valued_field_takes_a_node() {
    // `source.default` is a bare JSON value, so the hole may be a whole node.
    let mut env = Templates::new();
    env.register("row", &["cell"], &json!({
        "type": "frame",
        "source": { "default": "$cell" }
    }))
    .expect("registers");

    let mut b = stamp("row", json!({ "cell": { "type": "text", "id": "c1" } }));
    assert!(b.expand(&env).is_empty());
    assert_eq!(
        default_of(&b),
        Some(json!({ "type": "text", "id": "c1" }))
    );
}

#[test]
fn unregistered_name_warns_and_keeps_node() {
    let env = Templates::new();
    let mut b = stamp("ghost", json!({}));
    let warns = b.expand(&env);
    assert_eq!(warns.len(), 1);
    assert!(warns[0].contains("not registered"), "{warns:?}");
    assert_eq!(b.get_type(), "Accrete :: stamp");
}

#[test]
fn a_missing_name_is_the_loud_error_and_it_names_the_key() {
    let mut env = Templates::new();
    env.register("two", &["a", "b"], &json!({
        "type": "text",
        "id": "$a",
        "source": { "default": "$b" }
    }))
    .expect("registers");

    let mut b = stamp("two", json!({ "a": "x" }));
    let warns = b.expand(&env);
    assert_eq!(warns.len(), 1);
    assert!(warns[0].contains("missing value for `$b`"), "{warns:?}");
    assert_eq!(b.get_type(), "Accrete :: stamp", "node survives untouched");
}

#[test]
fn registration_refuses_the_old_text_form() {
    let mut env = Templates::new();
    let err = env
        .register("old", &["q"], &json!("{\"type\": \"text\", \"id\": \"{{q}}\"}"))
        .expect_err("a `{{key}}` string is not a definition");
    assert!(err.contains("node tree"), "{err}");
    assert!(err.contains("$name"), "the message says what to do: {err}");
    assert!(env.is_empty(), "a refused definition is not registered");
}

#[test]
fn registration_refuses_an_undeclared_variable() {
    let mut env = Templates::new();
    let err = env
        .register("typo", &["qq"], &json!({
            "type": "text", "source": { "default": "$q" }
        }))
        .expect_err("`$q` is not declared");
    assert!(err.contains("`$q`"), "{err}");
    assert!(err.contains("not declared"), "{err}");
}

#[test]
fn registration_refuses_an_unused_param() {
    let mut env = Templates::new();
    let err = env
        .register("idle", &["q"], &json!({ "type": "text" }))
        .expect_err("`q` is declared and never used");
    assert!(err.contains("never used"), "{err}");
}

#[test]
fn registration_refuses_a_body_that_is_not_a_node() {
    let mut env = Templates::new();
    let err = env
        .register("bad", &[], &json!({ "type": "nosuchnode" }))
        .expect_err("not a node the model has");
    assert!(err.contains("not a node tree"), "{err}");
}

#[test]
fn expand_recurses_into_children() {
    let mut env = Templates::new();
    env.register("lbl", &["t"], &json!({
        "type": "text",
        "source": { "default": "$t" }
    }))
    .unwrap();

    let mut b = accrete(json!({
        "type": "frame",
        "children": [
            { "type": "stamp", "name": "lbl", "data": { "t": "one" } },
            { "type": "stamp", "name": "lbl", "data": { "t": "two" } }
        ]
    }));
    assert!(b.expand(&env).is_empty());

    let kids = b.borrow_children().unwrap();
    assert_eq!(default_of(&kids[0]), Some(json!("one")));
    assert_eq!(default_of(&kids[1]), Some(json!("two")));
}

#[test]
fn a_stamp_inside_a_definition_expands_too() {
    let mut env = Templates::new();
    env.register("outer", &["n"], &json!({
        "type": "frame",
        "children": [
            { "type": "stamp", "name": "inner", "data": { "n": "$n" } }
        ]
    }))
    .unwrap();
    env.register("inner", &["n"], &json!({
        "type": "text", "source": { "default": "$n" }
    }))
    .unwrap();

    let mut b = stamp("outer", json!({ "n": "deep" }));
    assert!(b.expand(&env).is_empty());
    let kids = b.borrow_children().unwrap();
    assert_eq!(default_of(&kids[0]), Some(json!("deep")));
}