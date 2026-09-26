//! KDL parsing tests: wire-shape correctness + serde round trip.

use accrete::{Accrete, AccreteOps, BindVariant};

const SAMPLE: &str = r#"
fold id="app" class="panel wide" {
    bind "click" {
        event "toggle"
    }
    text id="title" format="markdown"
    form instant=#true {
        input id="q"
        button id="go" oneshot=#true {
            bind "submit" {
                submit
            }
        }
    }
    rack scroll=#true {
        item {
            text
        }
    }
    group {
        style {
            color "red"
            margin "4px 8px"
        }
    }
}
"#;

fn as_fold(b: &Accrete) -> Option<&accrete::Fold> {
    match b {
        Accrete::fold(f) => Some(f),
        _ => None,
    }
}

fn as_form(b: &Accrete) -> Option<&accrete::Form> {
    match b {
        Accrete::form(f) => Some(f),
        _ => None,
    }
}

fn as_button(b: &Accrete) -> Option<&accrete::Button> {
    match b {
        Accrete::button(b) => Some(b),
        _ => None,
    }
}

fn as_rack(b: &Accrete) -> Option<&accrete::Rack> {
    match b {
        Accrete::rack(r) => Some(r),
        _ => None,
    }
}

fn as_group(b: &Accrete) -> Option<&accrete::Group> {
    match b {
        Accrete::group(g) => Some(g),
        _ => None,
    }
}

#[test]
fn kdl_well_formed_tree() {
    let accretes = stage::kdl_parse::parse(SAMPLE).expect("kdl parse");
    assert_eq!(accretes.len(), 1);
    assert_eq!(accretes[0].get_id().as_deref(), Some("app"));

    let fold = as_fold(&accretes[0]).expect("fold");
    // bind kind survives parsing
    let bind = fold.bind.as_ref().unwrap().get("click").unwrap();
    assert_eq!(
        bind.variant,
        BindVariant::Event {
            event: "toggle".into()
        }
    );

    // children count: text, form, rack, group
    let children = fold.borrow_children().unwrap();
    assert_eq!(children.len(), 4);

    // nested form → button → submit bind
    let form = children.iter().find_map(as_form).unwrap();
    assert!(form.attrs.as_ref().unwrap().instant.unwrap());
    let go = form
        .borrow_children()
        .unwrap()
        .iter()
        .find_map(as_button)
        .unwrap();
    assert!(matches!(
        go.bind.as_ref().unwrap().get("submit").unwrap().variant,
        BindVariant::Submit { .. }
    ));

    // rack item group
    let rack = children.iter().find_map(as_rack).unwrap();
    assert!(rack.attrs.as_ref().unwrap().scroll);
    assert_eq!(rack.item.as_ref().unwrap().len(), 1);

    // style map from child block
    let group = children.iter().find_map(as_group).unwrap();
    let style = group.attrs.as_ref().unwrap().style.as_ref().unwrap();
    assert_eq!(style.get("color").map(String::as_str), Some("red"));
    assert_eq!(style.get("margin").map(String::as_str), Some("4px 8px"));
}

#[test]
fn kdl_serde_roundtrip() {
    // KDL → Accrete → wire Value → Accrete must equal KDL → Accrete.
    // i.e. the hand-written translation layer and serde agree on the shape.
    let via_accrete = stage::kdl_parse::parse(SAMPLE).unwrap();
    for b in &via_accrete {
        let wire = serde_json::to_value(b).unwrap();
        let back: Accrete = serde_json::from_value(wire).unwrap();
        assert_eq!(back, *b);
    }
}

#[test]
fn kdl_errors() {
    assert!(stage::kdl_parse::parse("widget {}").is_err()); // unknown accrete
    assert!(stage::kdl_parse::parse("fold { bind { event \"x\" } }").is_err()); // missing key
}

#[test]
fn v2_literals_and_reject_of_v1_booleans() {
    // #true parses (real v2 grammar)…
    let accretes = stage::kdl_parse::parse("rack scroll=#true").unwrap();
    assert!(matches!(accretes[0], Accrete::rack(_)));
    // …and bare v1-style `true` must now fail (regression lock for the crate swap)
    assert!(stage::kdl_parse::parse("rack scroll=true").is_err());
}

#[test]
fn template_node_maps_name_and_data() {
    // template "<name>" { data { k "v" } } → accrete::Template { name, data }
    let src = "template \"greet\" {\n    data {\n        who \"world\"\n        times 2\n    }\n}";
    let accretes = stage::kdl_parse::parse(src).unwrap();
    let wire = serde_json::to_value(&accretes[0]).unwrap();
    assert_eq!(wire["type"], "template");
    assert_eq!(wire["name"], "greet");
    assert_eq!(wire["data"]["who"], "world");
    assert_eq!(wire["data"]["times"], 2);
}

#[test]
fn bind_field_payload_subblock() {
    // field kind accepts a `payload { ... }` child → flattened payload map
    let src = "button {\n    bind \"click\" {\n        field \"send\" {\n            payload {\n                channel \"general\"\n            }\n        }\n    }\n}";
    let accretes = stage::kdl_parse::parse(src).unwrap();
    let wire = serde_json::to_value(&accretes[0]).unwrap();
    let bind = &wire["bind"]["click"];
    assert_eq!(bind["kind"], "field");
    assert_eq!(bind["field"], "send");
    assert_eq!(bind["payload"]["channel"], "general");
    // must also survive serde round trip into the typed variant
    let back: Accrete = serde_json::from_value(wire).unwrap();
    match back {
        Accrete::button(b) => {
            let bind_map = b.bind.unwrap();
            match &bind_map["click"].variant {
                BindVariant::Field { field, payload } => {
                    assert_eq!(field.as_str(), "send");
                    assert_eq!(payload.as_ref().unwrap()["channel"], "general");
                }
                other => panic!("expected field variant, got {other:?}"),
            }
        }
        _ => panic!("expected button"),
    }
}

#[test]
fn content_frame_shape() {
    // parse_kdl_to_frame wraps accretes in the Content::Create envelope the UI speaks
    let frame = stage::proto::parse_kdl_to_frame(SAMPLE).unwrap();
    let obj = frame.as_object().unwrap();
    assert_eq!(obj.get("ev").and_then(|v| v.as_str()), Some("draw"));
    assert_eq!(obj.get("sender").and_then(|v| v.as_str()), Some("stage"));
    // content is OneOrMany; a single Content serializes as one object
    let create = obj.get("content").unwrap();
    assert_eq!(
        create.get("action").and_then(|v| v.as_str()),
        Some("create")
    );
    assert_eq!(create.get("event").and_then(|v| v.as_str()), Some("stage"));
    // data carries the single accrete bare
    assert_eq!(
        create
            .get("data")
            .and_then(|d| d.get("type"))
            .and_then(|t| t.as_str()),
        Some("fold")
    );
}
