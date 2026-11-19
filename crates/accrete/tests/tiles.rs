//! ADR 0031 §4: `rack` and `pages` are one node, `tiles`, with two axes — `window` (how many items
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

use accrete::{Accrete, AccreteOps, Step, Window};

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

/// A frame that says nothing about the two axes gets neither: every item laid out, nothing
/// stepping — which is the plain list a `rack` that omitted `view` always was.
#[test]
fn a_frame_that_says_nothing_lays_everything_out() {
    let bare = tiles(r#"{"type":"tiles","source":{"slot":"chat"}}"#);
    let attrs = bare.attrs.unwrap_or_default();
    assert!(attrs.window.is_none(), "no window: everything is laid out");
    assert!(attrs.step.is_none(), "no clock, no external stepper");
}

/// A page is a window of one, driven from outside: the slot names the item on stage, and no clock
/// is involved.
#[test]
fn a_page_is_a_window_of_one_stepped_by_a_slot() {
    let pager = tiles(
        r#"{"type":"tiles","attrs":{"window":{"size":1},"step":{"slot":"page"}},
            "source":{"slot":"pages"}}"#,
    );
    let attrs = pager.attrs.expect("attrs");
    assert_eq!(attrs.window, Some(Window { size: 1 }));
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
fn a_marquee_is_a_window_of_one_stepped_by_a_clock() {
    let notice = tiles(
        r#"{"type":"tiles","attrs":{"window":{"size":1},"step":{"timer":6000}},
            "source":{"slot":"notice"}}"#,
    );
    let attrs = notice.attrs.expect("attrs");
    assert_eq!(attrs.window, Some(Window { size: 1 }));
    assert_eq!(attrs.step, Some(Step::Timer { timer: 6000 }));
}

/// A window of N is the same thing with a bigger number — the paged table, the board, the strip: one
/// node, one field, and N consecutive items drawn from the cursor.
#[test]
fn a_paged_table_is_a_window_of_n() {
    let page = tiles(
        r#"{"type":"tiles","attrs":{"window":{"size":20},"step":{"slot":"page"}},
            "source":{"slot":"rows"}}"#,
    );
    let attrs = page.attrs.expect("attrs");
    assert_eq!(attrs.window, Some(Window { size: 20 }));
}

/// A window narrower than one item can never show what the frame asked for, so it is refused where
/// every other impossible shape is: at decode.
#[test]
fn a_zero_width_window_is_refused() {
    assert!(
        serde_json::from_str::<Accrete>(r#"{"type":"tiles","attrs":{"window":{"size":0}}}"#)
            .is_err(),
        "size 0 must not decode"
    );
}

/// The defaults are not written out: a tiles that names no window encodes without `attrs`, and the
/// one that does carries that number and nothing else.
#[test]
fn the_defaults_do_not_appear_on_the_wire() {
    let n: Accrete =
        serde_json::from_str(r#"{"type":"tiles","source":{"slot":"chat"}}"#).expect("decode");
    assert_eq!(
        serde_json::to_value(&n).expect("encode"),
        serde_json::json!({"type":"tiles","source":{"slot":"chat"}})
    );

    let one: Accrete = serde_json::from_str(
        r#"{"type":"tiles","attrs":{"window":{"size":1}},"source":{"slot":"pages"}}"#,
    )
    .expect("decode");
    assert_eq!(
        serde_json::to_value(&one).expect("encode"),
        serde_json::json!({"type":"tiles","attrs":{"window":{"size":1}},
                           "source":{"slot":"pages"}})
    );
}

/// `stage` was the count before it was a window. Attribute bags drop keys they do not know (the
/// arms cannot deny them without inlining every `$defs` entry), so the load check in stage is what
/// makes the old spelling loud; here the point is only that it is not a synonym for anything.
#[test]
fn the_old_stage_key_is_not_a_window() {
    let n: Accrete = serde_json::from_str(
        r#"{"type":"tiles","attrs":{"stage":"one"},"source":{"slot":"chat"}}"#,
    )
    .expect("unknown attribute keys are dropped, not refused");
    match n {
        Accrete::tiles(t) => assert!(t.attrs.unwrap_or_default().window.is_none()),
        other => panic!("not tiles: {other:?}"),
    }
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
