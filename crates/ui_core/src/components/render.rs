use crate::Ctx;
use accrete::Template;
use leptos::html::*;
use leptos::prelude::*;

/// 占位渲染：无内容。
pub fn template_(_accrete: Template, _ctx: &Ctx) -> AnyView {
    div().into_any()
}
