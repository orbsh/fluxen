//! KDL parsing tests: wire-shape correctness + serde round trip.

use brick::{BindVariant, Brick, BrickOps};

const SAMPLE: &str = r#"
fold id="app" class="panel wide" {
    bind "click" {
        event "toggle"
    }
    text id="title" format="markdown"
    form instant=true {
        input id="q"
        button id="go" oneshot=true {
            bind "submit" {
                submit
            }
        }
    }
    rack scroll=true {
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

fn as_fold(b: &Brick) -> Option<&brick::Fold> {
    match b {
        Brick::fold(f) => Some(f),
        _ => None,
    }
}

fn as_form(b: &Brick) -> Option<&brick::Form> {
    match b {
        Brick::form(f) => Some(f),
        _ => None,
    }
}

fn as_button(b: &Brick) -> Option<&brick::Button> {
    match b {
        Brick::button(b) => Some(b),
        _ => None,
    }
}

fn as_rack(b: &Brick) -> Option<&brick::Rack> {
    match b {
        Brick::rack(r) => Some(r),
        _ => None,
    }
}

fn as_group(b: &Brick) -> Option<&brick::Group> {
    match b {
        Brick::group(g) => Some(g),
        _ => None,
    }
}

#[test]
fn kdl_well_formed_tree() {
    let bricks = stage::kdl_parse::parse(SAMPLE).expect("kdl parse");
    assert_eq!(bricks.len(), 1);
    assert_eq!(bricks[0].get_id().as_deref(), Some("app"));

    let fold = as_fold(&bricks[0]).expect("fold");
    // bind kind survives parsing
    let bind = fold.bind.as_ref().unwrap().get("click").unwrap();
    assert_eq!(bind.variant, BindVariant::Event { event: "toggle".into() });

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
    // KDL → Brick → wire Value → Brick must equal KDL → Brick.
    // i.e. the hand-written translation layer and serde agree on the shape.
    let via_brick = stage::kdl_parse::parse(SAMPLE).unwrap();
    for b in &via_brick {
        let wire = serde_json::to_value(b).unwrap();
        let back: Brick = serde_json::from_value(wire).unwrap();
        assert_eq!(back, *b);
    }
}

#[test]
fn kdl_errors() {
    assert!(stage::kdl_parse::parse("widget {}").is_err()); // unknown brick
    assert!(stage::kdl_parse::parse("fold { bind { event \"x\" } }").is_err()); // missing key
}

#[test]
fn content_frame_shape() {
    // parse_kdl_to_frame wraps bricks in the Content::Create envelope the UI speaks
    let frame = stage::proto::parse_kdl_to_frame(SAMPLE).unwrap();
    let obj = frame.as_object().unwrap();
    assert_eq!(obj.get("sender").and_then(|v| v.as_str()), Some("stage"));
    // content is OneOrMany; a single Content serializes as one object
    let create = obj.get("content").unwrap();
    assert_eq!(
        create.get("action").and_then(|v| v.as_str()),
        Some("create")
    );
    assert_eq!(
        create.get("event").and_then(|v| v.as_str()),
        Some("stage")
    );
    // data carries the single brick bare
    assert_eq!(
        create
            .get("data")
            .and_then(|d| d.get("type"))
            .and_then(|t| t.as_str()),
        Some("fold")
    );
}
