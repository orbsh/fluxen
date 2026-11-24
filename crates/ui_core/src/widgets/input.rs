use crate::Ctx;
use crate::hooks::{
    enter_pressed, field_changed, node_id, use_common_css, value_of,
};
use accrete::{AccreteOps, Input, JsType};
use leptos::ev;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

fn default_option_jskind(v: &Option<JsType>) -> Value {
    v.as_ref()
        .map(|x| x.default_value())
        .unwrap_or_else(|| Value::String(String::new()))
}

/// The text of an `<input>` or a `<textarea>` target: multiline is a layout of
/// this field, not another node (ADR 0032), so both elements land here.
///
/// A `type: bool` field is a checkbox, and a checkbox keeps its state in `checked()`, never in `value()`
/// (a box with no `value` attribute reports the string `"on"`, which parsed as a bool is always false —
/// a skin toggle written that way could only ever turn things off).
fn target_value(kind: &Option<JsType>, target: Option<web_sys::EventTarget>) -> String {
    let Some(t) = target else {
        return String::new();
    };
    if let Some(i) = t.dyn_ref::<web_sys::HtmlInputElement>() {
        if matches!(kind, Some(JsType::bool)) {
            return if i.checked() { "true".into() } else { "false".into() };
        }
        return i.value();
    }
    if let Some(a) = t.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        return a.value();
    }
    String::new()
}

/// Clear the element imperatively (browser value-attribute dirty semantics):
/// typing dirties the default value, so attribute updates no longer reset what
/// is displayed, and this node lives for the whole session.
fn clear_target(target: Option<web_sys::EventTarget>) {
    let Some(t) = target else { return };
    if let Some(i) = t.dyn_ref::<web_sys::HtmlInputElement>() {
        i.set_value("");
    } else if let Some(a) = t.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        a.set_value("");
    }
}

fn parse(kind: &Option<JsType>, raw: &str) -> Value {
    match kind {
        Some(JsType::bool) => Value::Bool(raw == "true"),
        Some(JsType::number) => Value::from(raw.parse::<f64>().unwrap_or(0.0)),
        _ => Value::String(raw.to_string()),
    }
}

/// One keystroke. A tagged field writes the bundle's copy on every change, so a
/// later submit packs what is on screen; whether a change is also a send is the
/// bundle's decision (`attrs.submit`), never the field's.
fn on_input(
    kind: Option<JsType>,
    slot: RwSignal<Value>,
    field: Option<RwSignal<Value>>,
    sink: Option<accrete::Bind>,
    id: Option<String>,
    ctx: Ctx,
) -> impl Fn(web_sys::Event) + 'static {
    let is_switch = matches!(kind, Some(JsType::bool));
    move |event: web_sys::Event| {
        let parsed = parse(&kind, &target_value(&kind, event.target()));
        slot.set(parsed.clone());
        if let Some(sig) = field {
            sig.set(parsed);
            field_changed(&ctx, ctx.bundle.as_ref());
            return;
        }
        // A switch has no Enter to press, so its change IS its exit (a text field keeps Enter: typing must
        // not send per keystroke).
        if is_switch
            && let Some(sink) = &sink
        {
            ctx.emit(sink, id.clone(), parsed);
        }
    }
}

/// Enter. The nearest enclosing bundle that declares `enter` sends it; a field
/// with no bundle keeps its own exit (the sink, as before). In a multiline field
/// Enter is the submit and Shift+Enter is the newline — otherwise `submit:
/// [enter]` would be dead on the very field a chat box uses.
fn on_key(
    multiline: bool,
    field: Option<RwSignal<Value>>,
    sink: Option<accrete::Bind>,
    id: Option<String>,
    kind: Option<JsType>,
    slot: RwSignal<Value>,
    ctx: Ctx,
) -> impl Fn(web_sys::KeyboardEvent) + 'static {
    move |ev: web_sys::KeyboardEvent| {
        if ev.key() != "Enter" || ev.shift_key() {
            return;
        }
        if multiline {
            ev.prevent_default();
        }
        if field.is_some() {
            enter_pressed(&ctx, ctx.bundle.as_ref());
            return;
        }
        let Some(sink) = sink.clone() else {
            return;
        };
        let val = slot.get_untracked();
        // An empty Enter does not fire (same rule the old input/textarea had).
        if val.as_str().is_some_and(|s| s.trim().is_empty()) {
            return;
        }
        slot.set(default_option_jskind(&kind));
        clear_target(ev.target());
        // One exit (ADR 0031): the scope decides page-local, uplink, or both;
        // `id` = this field's own id (ADR 0010).
        ctx.emit(&sink, id.clone(), val);
    }
}

/// A field: `source` is what it shows, `sink` is where its value goes when it
/// stands alone, `tag` is what a bundle calls it (ADR 0032). `multiline` is the
/// layout — same node, same channels.
pub fn input_(accrete: Input, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let node_id = node_id(&accrete);
    let mut css = vec!["input", "flex", "is-shadow"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let sink = accrete.get_sink().cloned();
    // The type can sit on either channel: the read side gives the display, the
    // write side gives the parse of the uplink payload.
    let kind = sink
        .as_ref()
        .and_then(|s| s.r#type.clone())
        .or_else(|| accrete.get_source().and_then(|b| b.r#type.clone()));

    // A tagged field belongs to its bundle: its value lives in the bundle's
    // signal and the bundle's sink is the only exit. No tag = no member, and the
    // field keeps its own exit.
    let field = accrete
        .tag
        .as_ref()
        .filter(|t| !t.is_empty())
        .and_then(|tag| ctx.bundle.as_ref().and_then(|b| b.field(tag)));

    // Initial value: the bundle's copy, else the source literal. A field does
    // not subscribe to the slot it writes (a self read/write loop restores what
    // was just sent).
    let is_switch = matches!(kind, Some(JsType::bool));
    let initial = match field {
        Some(sig) => sig.get_untracked(),
        // A switch shows the state it is SET to, including the one the far side set: its value is its display,
        // which a text field's is not (that is why a field otherwise takes its literal and never reads its own
        // slot — typing would be undone by the echo of what was just sent).
        None if is_switch => {
            value_of(&ctx, accrete.get_source(), false).unwrap_or(Value::Bool(false))
        }
        None => accrete
            .get_source()
            .and_then(|b| b.default.clone())
            .unwrap_or_else(|| default_option_jskind(&kind)),
    };
    let slot = RwSignal::new(initial);

    // Writing a switch is idempotent (checked = value), so following the slot costs no loop: the write lands in
    // the same place the read comes from and an equal value notifies nobody.
    if is_switch {
        let src = accrete.get_source().cloned();
        let ctx2 = ctx.clone();
        Effect::new(move |_| {
            if let Some(v) = value_of(&ctx2, src.as_ref(), true) {
                slot.set(Value::Bool(v.as_bool().unwrap_or(false)));
            }
        });
    }

    let placeholder = value_of(&ctx, accrete.placeholder.as_ref(), false)
        .and_then(|d| d.as_str().map(String::from));

    let ty = match &kind {
        Some(JsType::number) => "number",
        Some(JsType::bool) => "checkbox",
        Some(x) => x.input_type(),
        None => "text",
    };

    if accrete.multiline {
        // A textarea shows its value as content; the display is a snapshot and
        // a send clears the element imperatively (clear_target).
        let text = match slot.get_untracked() {
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            v => v.as_str().unwrap_or("").to_string(),
        };
        let base = textarea()
            .class(css.as_str())
            .child(text)
            .on(
                ev::input,
                on_input(
                    kind.clone(),
                    slot,
                    field,
                    sink.clone(),
                    node_id.clone(),
                    ctx.clone(),
                ),
            )
            .on(
                ev::keydown,
                on_key(true, field, sink, node_id, kind, slot, ctx.clone()),
            );
        return match placeholder {
            Some(ph) => base.placeholder(ph).into_any(),
            None => base.into_any(),
        };
    }

    // Reactivity sinks into the attribute closure: `value` is a DynProperty, so a
    // signal write lands on the DOM value property directly. Wrapping the whole
    // element tree in a top-level closure instead makes the rebuild a no-op for
    // dirty string attributes ("signal cleared, text still there").
    let value = move || {
        let v = slot.get();
        match &v {
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => v.as_str().unwrap_or("").to_string(),
        }
    };
    let checked = move || slot.get().as_bool().unwrap_or(false);

    let base = input()
        .class(css.as_str())
        .r#type(ty)
        .on(
            ev::input,
            on_input(
                kind.clone(),
                slot,
                field,
                sink.clone(),
                node_id.clone(),
                ctx.clone(),
            ),
        )
        .on(
            ev::keydown,
            on_key(false, field, sink, node_id, kind.clone(), slot, ctx.clone()),
        );
    match (&kind, placeholder) {
        (Some(JsType::bool), Some(ph)) => base.checked(checked).placeholder(ph).into_any(),
        (Some(JsType::bool), None) => base.checked(checked).into_any(),
        (_, Some(ph)) => base.value(value).placeholder(ph).into_any(),
        (_, None) => base.value(value).into_any(),
    }
}