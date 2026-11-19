use crate::Ctx;
use crate::hooks::{nearest, submit, use_default};
use accrete::{AccreteOps, Button, SubmitMode};
use leptos::ev::click;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;

/// A button: the label comes from `source`. A `sink` makes it the bundle's
/// confirm key — submitting is not a third channel (ADR 0031 §2), the bundle
/// holds the exit and names the timing (`submit: [.., button]`), the button only
/// says "send this group".
///
/// A press is one submit (ADR 0032). `attrs.oneshot` therefore no longer changes
/// anything; it stays on the wire because frames still carry it.
pub fn button_(accrete: Button, ctx: &Ctx) -> AnyView {
    let t = use_default(&accrete)
        .unwrap_or(Value::String("Ok".into()))
        .as_str()
        .unwrap_or("Ok")
        .to_owned();

    if accrete.get_sink().is_none() {
        // No sink = not a confirm key: nothing to draw (same as before).
        return div().into_any();
    }

    // The nearest enclosing bundle that takes a button press owns the send; it
    // also owns the payload and the id, so the button adds nothing.
    let Some(bundle) = nearest(ctx.bundle.as_ref(), SubmitMode::Button) else {
        return div().into_any();
    };

    let ctx = ctx.clone();
    move || -> AnyView {
        let ctx = ctx.clone();
        let bundle = bundle.clone();
        button()
            .class("button shadow accent")
            .child(t.clone())
            .on(click, move |_| submit(&ctx, &bundle))
            .into_any()
    }
    .into_any()
}