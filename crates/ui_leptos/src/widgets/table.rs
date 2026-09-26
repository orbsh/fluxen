use crate::Ctx;
use crate::ctx::render_children;
use accrete::{Table, Tbody, Td, Th, Thead, Tr};
use leptos::html::*;
use leptos::prelude::*;

pub fn table_(accrete: Table, ctx: &Ctx) -> AnyView {
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();
    table().child(children).into_any()
}

pub fn thead_(accrete: Thead, ctx: &Ctx) -> AnyView {
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();
    thead().child(children).into_any()
}

pub fn tbody_(accrete: Tbody, ctx: &Ctx) -> AnyView {
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();
    tbody().child(children).into_any()
}

pub fn tr_(accrete: Tr, ctx: &Ctx) -> AnyView {
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();
    tr().child(children).into_any()
}

pub fn th_(accrete: Th, ctx: &Ctx) -> AnyView {
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();
    th().child(children).into_any()
}

pub fn td_(accrete: Td, ctx: &Ctx) -> AnyView {
    let children = accrete
        .children
        .as_deref()
        .map(|s| render_children(ctx, s))
        .unwrap_or_default();
    td().child(children).into_any()
}
