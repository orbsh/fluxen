use crate::Ctx;
use accrete::Stamp;
use leptos::html::*;
use leptos::prelude::*;

/// 占位渲染：无内容。`stamp` 是展开期的引用，展开后不该留在这里（ADR 0034 改名，
/// 定义侧不变，ADR 0006）。
pub fn stamp_(_accrete: Stamp, _ctx: &Ctx) -> AnyView {
    div().into_any()
}