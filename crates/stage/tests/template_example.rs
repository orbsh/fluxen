//! The repo's own template example, end to end at the model level (ADR 0037):
//! `01.segment.tmpl.yaml` registers a definition whose body is a tree, and
//! `01.segment.render.yaml` appends a `stamp` that fills its three holes.
//!
//! No wasm here — this is the contract the renderer runs: the carrier parses the
//! definition once, the registry checks it, and the instance is the tree with the
//! values spliced in whole.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use accrete::AccreteOps;
use accrete::template::Templates;
use content::Content;

fn example(name: &str) -> String {
    let path = format!("{}/../../examples/yaml/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

#[test]
fn the_example_definition_registers_and_instantiates() {
    let mut env = Templates::new();

    for c in stage::proto::parse_yaml_to_frame(&example("01.segment.tmpl.yaml")).unwrap() {
        let Content::Tmpl(t) = c else {
            panic!("01.segment.tmpl.yaml is one `tmpl` item")
        };
        let params: Vec<&str> = t.params.iter().map(String::as_str).collect();
        env.register(&t.name, &params, &t.data)
            .expect("the example definition is valid");
    }
    assert_eq!(env.len(), 1);

    let frame = stage::proto::parse_yaml_to_frame(&example("01.segment.render.yaml")).unwrap();
    let Content::Append(op) = frame.into_iter().next().expect("one item") else {
        panic!("01.segment.render.yaml is one `append`")
    };
    let mut node: accrete::Accrete = op.data;
    assert_eq!(node.get_type(), "Accrete :: stamp");

    let warns = node.expand(&env);
    assert!(warns.is_empty(), "{warns:?}");

    // `$id` / `$attrs` / `$kids` were spliced in whole: the frame is the tree the
    // definition described, with the caller's values in it.
    assert_eq!(node.get_type(), "Accrete :: frame");
    assert_eq!(node.get_id().as_deref(), Some("seg1"));
    let kids = node.borrow_children().expect("children");
    assert_eq!(kids.len(), 6, "five frames + one text, as the caller sent");
    assert_eq!(kids[5].get_type(), "Accrete :: text");
}