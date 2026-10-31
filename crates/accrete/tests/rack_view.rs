//! ADR 0024: `rack.attrs.view` is wire contract, so the shapes are checked through the codec rather
//! than by constructing the Rust enum.
//!
//! Two properties matter. First, a frame that says nothing about `view` must decode exactly as it
//! always did — the view is optional and its defaults are the old rack. Second, the notice recipe is
//! a set of VALUES, not a type: what makes an area a notice area is this one block, and the test
//! pins it so a later tidy-up of the enums cannot quietly change what a notice means.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn
)]

use accrete::{Accrete, RackDirection, RackEnter, RackLayout, RackPause, RackPlay, RackScroll};

fn rack(src: &str) -> accrete::Rack {
    let node: Accrete = serde_json::from_str(src).expect("decode");
    let back = serde_json::to_string(&node).expect("encode");
    let again: Accrete = serde_json::from_str(&back).expect("re-decode");
    assert_eq!(node, again, "round-trip changed the node: {back}");
    match again {
        Accrete::rack(r) => r,
        other => panic!("not a rack: {other:?}"),
    }
}

#[test]
fn a_rack_that_says_nothing_about_view_is_the_old_rack() {
    let bare = rack(r#"{"type":"rack","bind":{"value":{"kind":"source","source":"msgs"}}}"#);
    assert!(
        bare.attrs.map(|a| a.view).unwrap_or_default().is_none(),
        "an old frame carries no view at all"
    );
}

#[test]
fn the_old_scroll_switch_still_decodes() {
    let scrolling = rack(
        r#"{"type":"rack","attrs":{"scroll":true},
            "bind":{"value":{"kind":"source","source":"msgs"}}}"#,
    );
    assert!(scrolling.attrs.unwrap().scroll, "the boolean survives");
}

#[test]
fn the_notice_recipe_is_a_set_of_values() {
    let notice = rack(
        r#"{"type":"rack","attrs":{"view":{"layout":"exclusive","enter":"fade","scroll":"none",
                                "play":"rotate","every":6000,"pause":"click"}},
            "bind":{"value":{"kind":"source","source":"notice"}}}"#,
    );
    let view = notice.attrs.and_then(|a| a.view).expect("view decodes");
    assert_eq!(view.layout, RackLayout::Exclusive);
    assert_eq!(view.enter, RackEnter::Fade);
    assert_eq!(view.scroll, RackScroll::None);
    assert_eq!(view.play, RackPlay::Rotate);
    assert_eq!(view.every, Some(6000));
    assert_eq!(view.pause, RackPause::Click);
    assert_eq!(
        view.direction,
        RackDirection::Vertical,
        "what the frame did not say comes from the default, which is the old behaviour"
    );
}

#[test]
fn a_fixed_window_says_how_many() {
    let fixed = rack(
        r#"{"type":"rack","attrs":{"view":{"layout":"fixed","count":3}},
            "bind":{"value":{"kind":"source","source":"rows"}}}"#,
    );
    let view = fixed.attrs.and_then(|a| a.view).expect("view decodes");
    assert_eq!(view.layout, RackLayout::Fixed);
    assert_eq!(view.count, Some(3));
    assert_eq!(view.play, RackPlay::Newest, "paging defaults to the newest");
}