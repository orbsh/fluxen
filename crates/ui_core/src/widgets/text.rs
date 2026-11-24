use crate::Ctx;
use crate::hooks::{use_common_css, use_source};
use accrete::{Text, TextAttr};
use leptos::html::*;
use leptos::prelude::*;
use markdown::{Options, to_html_with_options};
use std::sync::LazyLock;

static MDFMT: LazyLock<Vec<String>> = LazyLock::new(|| {
    ["markdown", "md"]
        .iter()
        .map(|fmt| fmt.to_string())
        .collect()
});

/// Text: takes `source` (slot value or literal); renders HTML when `TextAttr.format` is markdown/md.
pub fn text_(accrete: Text, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["text"];
    if let Some(id) = &accrete.id {
        css.push(id);
    }
    use_common_css(&mut css, &accrete);
    // markdown goes into the same join: two .class() calls on one element land on the same class attribute,
    // and the latter squeezes the former out entirely (measured: a markdown node's class list kept only "markdown", text/user classes/id all lost).
    if let Some(TextAttr {
        format: Some(fmt), ..
    }) = &accrete.attrs
        && MDFMT.contains(fmt)
    {
        css.push("markdown");
    }
    let base_css = css.join(" ");

    move || -> AnyView {
        let text_content = match use_source(&ctx, &accrete) {
            Some(j) => match j.as_str() {
                Some(s) => s.to_owned(),
                None => j.to_string(),
            },
            None => "".to_string(),
        };

        if let Some(TextAttr {
            format: Some(fmt), ..
        }) = &accrete.attrs
            && MDFMT.contains(fmt)
            && let Ok(md_html) = to_html_with_options(&text_content, &Options::gfm())
        {
            div()
                .class(base_css.as_str())
                .inner_html(md_html)
                .into_any()
        } else {
            div()
                .class(base_css.as_str())
                .child(text_content)
                .into_any()
        }
    }
    .into_any()
}
