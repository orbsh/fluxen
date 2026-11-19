//! ADR 0031 §4: `rack` and `pages` are one node, `tiles`, with two axes — `stage` (how many items
//! are on it) and `step` (what advances it).
//!
//! These are wire-contract checks, so the shapes are decoded through the codec rather than by
//! building the Rust enums: what a frame may say is the thing being pinned, and the retired
//! spellings must not decode at all.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn
)]

use accrete::{Accrete, AccreteOps, Stage, Step};

fn tiles(src: &str) -> accrete::Tiles {
    let node: Accrete = serde_json::from_str(src).expect("decode");
    let back = serde_json::to_string(&node).expect("encode");
    let again: Accrete = serde_json::from_str(&back).expect("re-decode");
    assert_eq!(node, again, "round-trip changed the node: {back}");
    match again {
        Accrete::tiles(t) => t,
        other => panic!("not tiles: {other:?}"),
    }
}

/// A frame that says nothing about the two axes gets the two defaults: everything laid out, and
/// nothing stepping — which is the plain list a `rack` that omitted `view` always was.
#[test]
fn a_frame_that_says_nothing_lays_everything_out() {
    let bare = tiles(r#"{"type":"tiles","source":{"slot":"chat"}}"#);
    let attrs = bare.attrs.unwrap_or_default();
    assert_eq!(attrs.stage, Stage::All);
    assert!(attrs.step.is_none(), "no clock, no external stepper");
}

/// One at a time, driven from outside: this is what `pages` was, and the slot names the node that
/// does the driving. No clock, no second channel.
#[test]
fn a_page_is_one_on_stage_stepped_by_a_slot() {
    let pager = tiles(
        r#"{"type":"tiles","attrs":{"stage":"one","step":{"slot":"page"}},
            "source":{"slot":"pages"}}"#,
    );
    let attrs = pager.attrs.expect("attrs");
    assert_eq!(attrs.stage, Stage::One);
    assert_eq!(
        attrs.step,
        Some(Step::Slot {
            slot: "page".into()
        })
    );
}

/// The other shape of `step`: a clock the node owns. Both shapes live on the same key, so a frame
/// says which one by what it puts there, not by a mode.
#[test]
fn a_marquee_is_one_on_stage_stepped_by_a_clock() {
    let notice = tiles(
        r#"{"type":"tiles","attrs":{"stage":"one","step":{"timer":6000}},
            "source":{"slot":"notice"}}"#,
    );
    let attrs = notice.attrs.expect("attrs");
    assert_eq!(attrs.stage, Stage::One);
    assert_eq!(attrs.step, Some(Step::Timer { timer: 6000 }));

    // Many at once, all served by one clock: the dwell a log uses.
    let log = tiles(
        r#"{"type":"tiles","attrs":{"step":{"timer":4000}},"source":{"slot":"lines"}}"#,
    );
    assert_eq!(log.attrs.expect("attrs").stage, Stage::All);
}

/// The defaults are not written out: a frame that says `stage: all` and no step encodes to the
/// shape it would have had saying nothing.
#[test]
fn the_defaults_do_not_appear_on_the_wire() {
    let n: Accrete = serde_json::from_str(
        r#"{"type":"tiles","attrs":{"stage":"all"},"source":{"slot":"chat"}}"#,
    )
    .expect("decode");
    assert_eq!(
        serde_json::to_value(&n).expect("encode"),
        serde_json::json!({"type":"tiles","attrs":{},"source":{"slot":"chat"}})
    );
}

/// Two containers became one, and the old spellings are gone rather than aliased: a frame that
/// still says `rack` or `pages` is a frame from before ADR 0031, not a synonym.
#[test]
fn the_retired_names_do_not_decode() {
    for old in ["rack", "pages"] {
        let wire = format!(r#"{{"type":"{old}","source":{{"slot":"chat"}}}}"#);
        assert!(
            serde_json::from_str::<Accrete>(&wire).is_err(),
            "`{old}` must not decode any more"
        );
    }
}

/// The template a tile is drawn with is `children` — the node's one "inside" field, like every
/// other container. The `item` a `rack` carried is not a second place to put it.
#[test]
fn a_tile_layout_arrives_as_children() {
    let t = tiles(
        r#"{"type":"tiles","source":{"slot":"chat"},
            "children":[{"type":"text","source":{"default":"x"}}]}"#,
    );
    assert_eq!(t.children.as_ref().map(|c| c.len()), Some(1));
    assert!(t.get_id().is_none());
}