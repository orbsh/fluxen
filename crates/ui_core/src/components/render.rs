use crate::Ctx;
use accrete::Stamp;
use leptos::html::*;
use leptos::prelude::*;

/// Placeholder render: no content. `stamp` is a reference from expansion time and should not be left here after expansion (ADR 0034 renamed it,
/// the definition side is unchanged, ADR 0006).
pub fn stamp_(_accrete: Stamp, _ctx: &Ctx) -> AnyView {
    div().into_any()
}