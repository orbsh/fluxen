use crate::Ctx;
use brick::Template;
use leptos::prelude::*;
use leptos::html::*;

/// 占位渲染：无内容。
pub fn template_(_brick: Template, _ctx: &Ctx) -> AnyView {
    div().into_any()
}
