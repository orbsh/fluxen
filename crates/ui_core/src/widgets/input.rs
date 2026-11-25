use crate::Ctx;
use crate::hooks::{enter_pressed, field_changed, locked, use_common_css, value_of};
use crate::rough;
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
            return if i.checked() {
                "true".into()
            } else {
                "false".into()
            };
        }
        return i.value();
    }
    if let Some(a) = t.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        return a.value();
    }
    String::new()
}

/// The text an element shows for a value — the same spelling the `value` binding uses.
fn as_text(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        v => v.as_str().unwrap_or("").to_string(),
    }
}

#[cfg(test)]
mod parse_tests {
    use super::parse;
    use accrete::JsType;
    use serde_json::json;

    /// The declared type decides the JSON the loom packs — a field that says `number` sends a number,
    /// and one that says nothing sends what was typed (a "42" in a text field stays text).
    #[test]
    fn the_declared_type_is_what_lands_in_the_object() {
        assert_eq!(parse(&Some(JsType::number), "42"), json!(42.0));
        assert_eq!(parse(&Some(JsType::number), "4.5"), json!(4.5));
        assert_eq!(parse(&Some(JsType::number), ""), json!(0.0));
        assert_eq!(parse(&Some(JsType::bool), "true"), json!(true));
        assert_eq!(parse(&Some(JsType::bool), "false"), json!(false));
        assert_eq!(parse(&None, "42"), json!("42"));
        assert_eq!(parse(&Some(JsType::text), "42"), json!("42"));
    }
}

/// The name the hand draws by: the field's `tag` (what its form calls it), else its `id` (its identity when it
/// stands alone). Either way it is stable across re-renders, so the same field keeps the same hand.
fn seed_of(accrete: &accrete::Input) -> Option<&str> {
    accrete.tag.as_deref().or(accrete.id.as_deref())
}

fn parse(kind: &Option<JsType>, raw: &str) -> Value {
    match kind {
        Some(JsType::bool) => Value::Bool(raw == "true"),
        Some(JsType::number) => Value::from(raw.parse::<f64>().unwrap_or(0.0)),
        _ => Value::String(raw.to_string()),
    }
}

/// One keystroke. A tagged field writes the loom's copy on every change, so a
/// later submit packs what is on screen; whether a change is also a send is the
/// loom's decision (`attrs.submit`), never the field's.
fn on_input(
    kind: Option<JsType>,
    slot: RwSignal<Value>,
    field: Option<RwSignal<Value>>,
    ctx: Ctx,
) -> impl Fn(web_sys::Event) + 'static {
    move |event: web_sys::Event| {
        let parsed = parse(&kind, &target_value(&kind, event.target()));
        slot.set(parsed.clone());
        // The value goes where the loom keeps it, and nowhere else: a field has no exit of its own
        // (ADR 0032 §2), so an untagged field is a display and nothing more.
        if let Some(sig) = field {
            sig.set(parsed);
            field_changed(&ctx, ctx.loom.as_ref());
        }
    }
}

/// Enter. The nearest enclosing loom that declares `enter` sends it; a field with no tag has no exit
/// here at all (ADR 0032 §2 revised — an uplink is a loom's business, and clearing is `reset`'s). In a
/// multiline field Enter is the submit and Shift+Enter is the newline — otherwise `submit: [enter]` would
/// be dead on the very field a chat box uses.
fn on_key(
    multiline: bool,
    field: Option<RwSignal<Value>>,
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
            enter_pressed(&ctx, ctx.loom.as_ref());
        }
    }
}

/// A field: `source` is what it shows, `kind` is what kind of field it is (element type and parse, one
/// declaration), `tag` is what a loom calls it (ADR 0032). `multiline` is the layout — same node.
pub fn input_(accrete: Input, ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    let mut css = vec!["input", "flex", "is-shadow"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    // One declaration for both halves: the same `kind` picks the element and parses what is typed into it.
    let kind = accrete.kind.clone();

    // A tagged field belongs to its loom: its value lives in the loom's
    // signal and the loom's sink is the only exit. No tag = no member, and the
    // field keeps its own exit.
    let field = accrete
        .tag
        .as_ref()
        .filter(|t| !t.is_empty())
        .and_then(|tag| ctx.loom.as_ref().and_then(|b| b.field(tag)));

    // Initial value: the loom's copy, else the source literal. A field does
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

    // A field's display follows the loom's copy, so `reset: clear`/`default` shows up in the box. Writing an
    // equal value notifies nobody, so following the very signal this input writes costs no loop.
    if let Some(sig) = field {
        Effect::new(move |_| slot.set(sig.get()));
    }

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

    // A loom that has used up its `sends` disables every field under it (ADR 0032 §5): the count says
    // "no more", and a box nobody can use should not take a click. `disabled` is presence, not value, so an
    // absent attribute is how "not locked" is spelled.
    let locked_ctx = ctx.clone();
    let is_locked = move || locked(locked_ctx.loom.as_ref()).then_some("");

    // The element keeps its own text once someone types in it, and a binding that writes the same string it
    // wrote last time is skipped — so a reset (`clear`/`default`) would change the signal and leave the box
    // full. This pushes the signal onto the element whenever they disagree, which is idempotent for the
    // ordinary case (`slot` == element) and the only way a reset becomes visible.
    let node_el = NodeRef::<leptos::html::Input>::new();
    let node_area = NodeRef::<leptos::html::Textarea>::new();
    {
        let slot = slot;
        let switch = is_switch;
        Effect::new(move |_| {
            let want = as_text(&slot.get());
            if let Some(el) = node_el.get_untracked() {
                // A checkbox shows `checked`, not `value`, and it has the same memory problem: a reset that
                // writes `false` onto a box the reader ticked would otherwise only reach the signal.
                if switch {
                    let on = slot.get().as_bool().unwrap_or(false);
                    if el.checked() != on {
                        el.set_checked(on);
                    }
                } else if el.value() != want {
                    el.set_value(&want);
                }
            }
            if let Some(el) = node_area.get_untracked() {
                if el.value() != want {
                    el.set_value(&want);
                }
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
        // A textarea shows its value as content; the display is a snapshot of the signal, which now
        // follows the loom, so a reset shows up without touching the element by hand.
        let text = match slot.get_untracked() {
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            v => v.as_str().unwrap_or("").to_string(),
        };
        let base = textarea()
            .class(css.as_str())
            .attr("disabled", is_locked)
            .node_ref(node_area)
            .child(text)
            .on(ev::input, on_input(kind.clone(), slot, field, ctx.clone()))
            .on(ev::keydown, on_key(true, field, ctx.clone()));
        let el = match placeholder {
            Some(ph) => base.placeholder(ph).into_any(),
            None => base.into_any(),
        };
        return rough::field_host(&ctx, seed_of(&accrete), el);
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
        .attr("disabled", is_locked)
        .node_ref(node_el)
        .on(ev::input, on_input(kind.clone(), slot, field, ctx.clone()))
        .on(ev::keydown, on_key(false, field, ctx.clone()));
    let el = match (&kind, placeholder) {
        (Some(JsType::bool), Some(ph)) => base.checked(checked).placeholder(ph).into_any(),
        (Some(JsType::bool), None) => base.checked(checked).into_any(),
        (_, Some(ph)) => base.value(value).placeholder(ph).into_any(),
        (_, None) => base.value(value).into_any(),
    };
    rough::field_host(&ctx, seed_of(&accrete), el)
}
