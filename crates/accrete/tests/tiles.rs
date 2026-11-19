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

/// ADR 0035: the direction is the way the items flow, spelled as flex spells it — a frame author
/// reads `row-reverse` and already knows what it means.
#[test]
fn the_four_directions_are_wire_words() {
    for (word, dir) in [
        ("row", accrete::Dir::Row),
        ("row-reverse", accrete::Dir::RowReverse),
        ("col", accrete::Dir::Col),
        ("col-reverse", accrete::Dir::ColReverse),
    ] {
        let t = tiles(&format!(
            r#"{{"type":"tiles","attrs":{{"dir":"{word}"}},"source":{{"slot":"x"}}}}"#
        ));
        assert_eq!(t.attrs.expect("attrs").dir, Some(dir), "dir: {word}");
    }
}

/// Absent is `col`, not flex's `row`: `tiles` is a list of items, so an unlabelled `tiles` is a
/// vertical list — which is also the shape every frame written before the axis existed already had.
/// Nothing is added to the wire for it — a default that appears is a default that can disagree.
#[test]
fn an_absent_direction_is_col_and_stays_off_the_wire() {
    let n: Accrete =
        serde_json::from_str(r#"{"type":"tiles","source":{"slot":"chat"}}"#).expect("decode");
    match &n {
        Accrete::tiles(t) => {
            assert_eq!(t.attrs.as_ref().and_then(|a| a.dir), None);
            // The default the renderer reads when the field is absent.
            assert_eq!(
                t.attrs.as_ref().and_then(|a| a.dir).unwrap_or_default(),
                accrete::Dir::Col,
                "a tiles with no dir is a vertical list"
            );
        }
        other => panic!("not tiles: {other:?}"),
    }
    assert_eq!(
        serde_json::to_value(&n).expect("encode"),
        serde_json::json!({"type":"tiles","source":{"slot":"chat"}})
    );
}

/// The line axis: `wrap` is absent because a wall of cards is the shape that has no other
/// spelling; `nowrap` is one word, as CSS writes it.
#[test]
fn the_line_axis_has_three_words() {
    for (word, wrap) in [
        ("wrap", accrete::Wrap::Wrap),
        ("wrap-reverse", accrete::Wrap::WrapReverse),
        ("nowrap", accrete::Wrap::NoWrap),
    ] {
        let t = tiles(&format!(
            r#"{{"type":"tiles","attrs":{{"wrap":"{word}"}},"source":{{"slot":"x"}}}}"#
        ));
        assert_eq!(t.attrs.expect("attrs").wrap, Some(wrap), "wrap: {word}");
    }
    // `no-wrap` is not a spelling of anything: the word is CSS's.
    assert!(
        serde_json::from_str::<Accrete>(r#"{"type":"tiles","attrs":{"wrap":"no-wrap"}}"#).is_err(),
        "`no-wrap` must not decode"
    );
}

/// `layers` counts the lines of header cells along the FIELD axis: 1 or 2, because a third one has
/// no key to group by. `rows` described where the header sat, which `dir: row` stopped being true.
#[test]
fn a_header_has_one_or_two_layers() {
    let two = tiles(
        r#"{"type":"tiles","attrs":{"fields":[{"key":"q"}],
            "header":{"selector":"h","layers":2}},"source":{"slot":"x"}}"#,
    );
    assert_eq!(two.attrs.expect("attrs").header.expect("header").layers, 2);

    let one = tiles(
        r#"{"type":"tiles","attrs":{"fields":[{"key":"q"}],
            "header":{"selector":"h"}},"source":{"slot":"x"}}"#,
    );
    let h = one.attrs.expect("attrs").header.expect("header");
    assert_eq!(h.layers, 1, "absent layers is one line, not zero");
    // One layer is the default, so it is not written out.
    let n: Accrete = serde_json::from_str(
        r#"{"type":"tiles","attrs":{"fields":[{"key":"q"}],
            "header":{"selector":"h","layers":1}},"source":{"slot":"x"}}"#,
    )
    .expect("decode");
    assert!(
        !serde_json::to_string(&n).expect("encode").contains("layers"),
        "the default must not appear on the wire"
    );

    // A third layer would render as something nobody chose, so it does not decode.
    assert!(
        serde_json::from_str::<Accrete>(
            r#"{"type":"tiles","attrs":{"fields":[{"key":"q"}],
                "header":{"selector":"h","layers":3}}}"#
        )
        .is_err(),
        "layers: 3 must not decode"
    );
}
