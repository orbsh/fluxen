use crate::Ctx;
use crate::hooks::{nearest, submit, use_common_css, use_default};
use crate::rough;
use accrete::{AccreteOps, Button, SubmitMode};
use leptos::ev::click;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;

/// A button: the label comes from `source`, and a press submits the nearest
/// enclosing bundle whose `attrs.submit` names `button`. Submitting is not a
/// third channel (ADR 0031 §2) and the button is not a party to it: the bundle
/// holds the exit, the payload, the id — and the timing, which is why a button
/// carries no `sink` of its own (ADR 0039 §2, amended). Outside such a bundle it
/// draws nothing, because a press there would send nothing.
///
/// A press is one submit (ADR 0032). `attrs.oneshot` therefore no longer changes
/// anything; it stays on the wire because frames still carry it.
pub fn button_(accrete: Button, ctx: &Ctx) -> AnyView {
    let t = use_default(&accrete)
        .unwrap_or(Value::String("Ok".into()))
        .as_str()
        .unwrap_or("Ok")
        .to_owned();

    // The nearest enclosing bundle that takes a button press owns the send; it also owns the payload
    // and the id, so the button adds nothing — not even the decision to send.
    let Some(bundle) = nearest(ctx.bundle.as_ref(), SubmitMode::Button) else {
        return div().into_any();
    };

    // The class list is the page's (ADR 0038 §4 recorded `button` ignoring `attrs.class` as the reason a
    // button could not be skinned; this is that one-line fix), with the pressing look as its default.
    let mut css = vec!["button", "is-shadow", "accent"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");
    let id = accrete.id.clone();

    let ctx = ctx.clone();
    move || -> AnyView {
        let ctx = ctx.clone();
        let bundle = bundle.clone();
        button()
            .class(css.clone())
            .child(t.clone())
            .child(rough::drawn_border(&ctx, id.as_deref()))
            .on(click, move |_| submit(&ctx, &bundle))
            .into_any()
    }
    .into_any()
}
