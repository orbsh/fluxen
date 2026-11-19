//! `entry` — named fields (ADR 0031 §4).
//!
//! On its own an entry is a block of its fields: that is what an item of a `tiles` without
//! `fields` draws when the item happens to be an entry. Under a `tiles` with `fields` the row is
//! built by `tiles_` instead — there the fields are projected into cells, so a plain block of fields
//! would be the wrong shape twice over (one column of values, and the field names lost).

use crate::Ctx;
use crate::ctx::render_children;
use crate::hooks::use_common_css;
use accrete::Entry;
use leptos::html::*;
use leptos::prelude::*;

pub fn entry_(accrete: Entry, ctx: &Ctx) -> AnyView {
    let mut css = vec!["entry", "f"];
    use_common_css(&mut css, &accrete);
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();
    div().class(css.join(" ")).child(children).into_any()
}
