use crate::Ctx;
use crate::hooks::use_default;
use accrete::{Image, ImageAttr};
use leptos::html::*;
use leptos::prelude::*;

/// Image: `src` comes from `source`, size/description from `ImageAttr`.
pub fn image_(accrete: Image, _ctx: &Ctx) -> AnyView {
    if let Some(src) = use_default(&accrete)
        && let Some(src) = src.as_str()
        && let Some(x) = accrete.attrs
    {
        let ImageAttr { desc, .. } = &x;
        let style = x.size_style();
        let desc = desc.clone().unwrap_or_default();
        img().src(src).alt(desc.as_str()).style(style).into_any()
    } else {
        div().into_any()
    }
}
