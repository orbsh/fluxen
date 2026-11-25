//! ui-leptos — the REFERENCE BOOT: page shell + WS wiring + the JS uplink handle.
//!
//! Everything that is rendering lives in `ui-core`; what stays here is the part a host is free to
//! replace, and this crate is one such host (ours). A host that wraps the core in its own binary (prism)
//! writes its own boot and never depends on this crate — the ownership rule is
//! `docs/decisions/0021-external-integration.md`, the section on ownership (`## 归属`).
//!
//! Two ways in, one configuration model (ADR 0045):
//!
//!   * the automatic one — this module starts, finds `#fluxen` and mounts it with the defaults (`?token=` and
//!     `?codec=` still apply). That is our own id, not a page landmark: a host's `#main` mounts nothing, so a
//!     page that configures anything (a path, a policy, its own socket) always means to call `mount`;
//!   * the explicit one — `mount(element, options)`, with the element named by the caller and the settings
//!     passed as data. One resolver (`settings::Boot::resolve`) reads both, and the call outranks the address.

// Unit tests may use the unwrap/panic vocabulary the production targets are
// denied (a panicking test is a failing test). `cfg(test)` is on only for this
// crate's own test build; `tests/` crates set the same line at their own root.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unwrap_in_result,
        clippy::panic_in_result_fn
    )
)]
use leptos::prelude::Get;

mod hook;
mod settings;

use content::codec::{ActiveCodec, CodecType};
use serde_json::Value;
use settings::{Boot, Opts};
use ui_core::Ctx;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;

/// Start the module: install the diagnostics, then mount `#fluxen` if the page has one.
///
/// `#fluxen` is our id, and it means "mount me with the defaults" — nothing else can be said in HTML any more,
/// so the element carries no configuration and there is no second reading of anything.
#[wasm_bindgen(start)]
pub fn start() {
    // WASM panics go only to stderr by default (invisible); a console hook is required to see them in devtools
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default_with_config(
        tracing_wasm::WASMLayerConfigBuilder::new()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );

    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let Ok(Some(el)) = doc.query_selector("#fluxen") else {
        // No `#fluxen` is not an error at start-up: a page may be about to call `mount` itself, or may have no
        // renderer at all. Only a page that asks for something gets an error, and it gets a loud one.
        return;
    };
    let Some(el) = el.dyn_into::<web_sys::HtmlElement>().ok() else {
        report("fluxen: #fluxen is not an HTML element");
        return;
    };
    // The element's `data-codec` is the page's word (ADR-0046): a snippet that read the address can
    // set it, and a page that wants to say more calls `mount` itself. The boot still reads no URL.
    let codec = el
        .dataset()
        .get("codec")
        .filter(|said| !said.trim().is_empty());
    let opts = codec.map(|codec| Opts {
        codec: Some(codec),
        ..Opts::default()
    });
    if let Err(e) = boot(el, opts, None, Vec::new(), Vec::new()) {
        report(&e);
    }
}

/// Mount explicitly: `await mount(document.querySelector('#app'), { host, path, codec, query, config,
/// policy, transport })`.
///
/// Nothing is read from `document.location` (ADR-0046): the host knows its own address, its own session
/// and its own storage, so it passes `codec` and puts anything else the socket needs — a token included,
/// because a token is just one more query parameter — into `query`. The boot joins and escapes them.
// --- the host's hooks: JS functions live here, the context holds only ids ------------------------
//
// `Ctx` is captured by view closures, so it must be `Send + Sync`, and a `js_sys::Function` is
// neither. The way out is not a channel: wasm is single-threaded, so calling the function from here
// IS running it on the JS side (ADR 0047). The table lives on this side; the adapter a `Ctx` holds
// captures one `u32` and nothing else.

thread_local! {
    static SLOT_HOOKS: std::cell::RefCell<std::collections::HashMap<u32, js_sys::Function>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static DEST_HOOKS: std::cell::RefCell<std::collections::HashMap<u32, js_sys::Function>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static NEXT_HOOK: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
}

fn next_hook_id() -> u32 {
    NEXT_HOOK.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    })
}

/// One slot observer as `Ctx` sees it: an id, and a call into the table.
fn slot_observer(id: u32) -> ui_core::ctx::SlotObserver {
    std::sync::Arc::new(move |name: &str, value: Option<&accrete::Accrete>| {
        let Ok(js) = serde_wasm_bindgen::to_value(&value) else {
            return;
        };
        SLOT_HOOKS.with(|table| {
            if let Some(callback) = table.borrow().get(&id) {
                // A host callback that throws is the host's bug, not a reason to lose the render: the
                // answer is dropped and said once in the console (ADR 0047 §4).
                if let Err(e) = callback.call2(&JsValue::NULL, &JsValue::from_str(name), &js) {
                    web_sys::console::warn_1(
                        &JsValue::from_str(&format!("fluxen: hooks.slots[{name}] threw: {e:?}")),
                    );
                }
            }
        });
    })
}

/// One uplink rewriter: `None` leaves the payload, a value replaces it (ADR 0047 §5).
fn dest_rewriter(id: u32) -> ui_core::ctx::DestRewriter {
    std::sync::Arc::new(move |dest: &str, data: serde_json::Value| {
        let ok = serde_wasm_bindgen::to_value(&data).ok()?;
        let answered = DEST_HOOKS.with(|table| {
            table
                .borrow()
                .get(&id)
                .map(|callback| callback.call2(&JsValue::NULL, &JsValue::from_str(dest), &ok))
        })?;
        match answered {
            Ok(value) if !value.is_undefined() && !value.is_null() => {
                serde_wasm_bindgen::from_value(value).ok()
            }
            Err(e) => {
                web_sys::console::warn_1(&JsValue::from_str(&format!(
                    "fluxen: hooks.dests[{dest}] threw, the frame keeps its payload: {e:?}"
                )));
                None
            }
            _ => None,
        }
    })
}

#[wasm_bindgen]
pub fn mount(element: web_sys::Element, options: JsValue) -> Result<(), JsValue> {
    let el = element
        .dyn_into::<web_sys::HtmlElement>()
        .map_err(|_| JsValue::from_str("fluxen: mount() needs an HTML element"))?;

    let mut opts = None;
    let mut hook = None;
    let mut slots: Vec<(String, u32)> = Vec::new();
    let mut dests: Vec<(String, u32)> = Vec::new();
    if !options.is_undefined() && !options.is_null() {
        // A hook is a live object, not data: take it off first, then deserialize the rest, so an unknown key
        // or a mis-shaped value is still a loud refusal.
        if let Ok(found) = js_sys::Reflect::get(&options, &JsValue::from_str("transport"))
            && !found.is_undefined()
            && !found.is_null()
        {
            hook = Some(found);
        }
        // `hooks` is live functions, taken off the same object for the same reason `transport` is: a
        // function is not data, so `Opts` never learns about it and `deny_unknown_fields` stays true.
        if let Ok(found) = js_sys::Reflect::get(&options, &JsValue::from_str("hooks"))
            && !found.is_undefined()
            && !found.is_null()
        {
            (slots, dests) = collect_hooks(&found)?;
        }
        // Deserialize a copy without that key: `deny_unknown_fields` is the rule, and a key set to
        // `undefined` is still a key — the caller's object is not ours to rewrite either.
        let source = js_sys::Object::from(options.clone());
        let rest = js_sys::Object::assign(&js_sys::Object::new(), &source);
        let _ = js_sys::Reflect::delete_property(&rest, &JsValue::from_str("transport"));
        let _ = js_sys::Reflect::delete_property(&rest, &JsValue::from_str("hooks"));
        opts =
            Some(serde_wasm_bindgen::from_value(rest.into()).map_err(|e| {
                JsValue::from_str(&format!("fluxen: mount(element, options): {e}"))
            })?);
    }

    boot(el, opts, hook, slots, dests).map_err(|e| JsValue::from_str(&e))
}

/// Read `hooks.slots` / `hooks.dests` off the mount options: keep the functions here, hand the context
/// their ids. A key that is present but not a function is a refusal, not a silent skip — the same rule
/// every other option follows.
fn collect_hooks(hooks: &JsValue) -> Result<(Vec<(String, u32)>, Vec<(String, u32)>), JsValue> {
    let mut slots: Vec<(String, u32)> = Vec::new();
    let mut dests: Vec<(String, u32)> = Vec::new();
    for (key, into) in [("slots", &mut slots), ("dests", &mut dests)] {
        let Ok(found) = js_sys::Reflect::get(hooks, &JsValue::from_str(key)) else {
            continue;
        };
        if found.is_undefined() || found.is_null() {
            continue;
        }
        let map = found
            .dyn_into::<js_sys::Object>()
            .map_err(|_| JsValue::from_str(&format!("fluxen: hooks.{key} must be an object")))?;
        for entry in js_sys::Object::entries(&map).iter() {
            let pair: js_sys::Array = entry.into();
            let name = pair
                .get(0)
                .as_string()
                .ok_or_else(|| JsValue::from_str(&format!("fluxen: hooks.{key} needs string keys")))?;
            let callback = pair
                .get(1)
                .dyn_into::<js_sys::Function>()
                .map_err(|_| {
                    JsValue::from_str(&format!("fluxen: hooks.{key}[{name}] is not a function"))
                })?;
            let id = next_hook_id();
            if key == "slots" {
                SLOT_HOOKS.with(|table| {
                    table.borrow_mut().insert(id, callback);
                });
            } else {
                DEST_HOOKS.with(|table| {
                    table.borrow_mut().insert(id, callback);
                });
            }
            into.push((name, id));
        }
    }
    Ok((slots, dests))
}

/// One boot for both entrances: resolve, open the socket, mount the root, publish the uplink handle.
fn boot(
    el: web_sys::HtmlElement,
    opts: Option<Opts>,
    hook_value: Option<JsValue>,
    slots: Vec<(String, u32)>,
    dests: Vec<(String, u32)>,
) -> Result<(), String> {
    // Mounting is a page-level operation; every step can fail (not in a browser, WS won't connect); these are
    // states the caller must surface, so they go through Result, no panic.
    let win = web_sys::window().ok_or("mount() outside a browser (no window)")?;
    let doc = win.document().ok_or("no document")?;
    let loc = doc.location().ok_or("no document.location")?;

    let boot = Boot::resolve(opts.as_ref())?;

    let host = match &boot.host {
        Some(h) => h.clone(),
        None => loc.host().map_err(|_| "document.location has no host")?,
    };
    let codec_str = match boot.codec {
        CodecType::Json => "json",
        CodecType::Cbor => "cbor",
    };
    // THE ADDRESS IS THE HOST'S (ADR-0046). We do not look at `document.location`: the host read
    // whatever it had to read (a token in storage, a query parameter, nothing) and handed us the
    // parts. All we do here is join them, escaped, in a fixed order.
    // A token is nothing special here: it is one more query parameter, put there by whoever knows the
    // session (ADR-0046). The boot has no field for it, and no branch for it.
    let mut url = format!("ws://{}{}?codec={}", host, boot.path, codec_str);
    for (key, value) in &boot.query {
        url.push_str(&format!("&{}={}", encode(key), encode(value)));
    }
    let codec = ActiveCodec::new(boot.codec);

    // Three modes, decided here and never later (ADR 0040 §3, ADR 0044 §3, ADR 0045 §2):
    //   * the page passed a hook to `mount`, or installed `window.fluxenTransport` → use it (it owns its
    //     socket and its retries);
    //   * otherwise our own live socket → heartbeat + reconnect with backoff;
    //   * a hook that is present but malformed is an error, not a silent fallback.
    let hooked = match hook_value {
        Some(v) => Some(hook::wrap(v, codec.clone())?),
        None => hook::from_window(codec.clone())?,
    };
    let transport: std::rc::Rc<dyn transport::Transport> = match hooked {
        Some(hooked) => {
            if boot.policy_said {
                // A knob that silently does nothing is the defect this file keeps refusing: the hook owns
                // the socket, so the policy of ours it would have tuned is not in play.
                web_sys::console::warn_1(&JsValue::from_str(
                    "fluxen: the transport policy configures the built-in socket; with a hook installed it does nothing — set the heartbeat in the hook",
                ));
            }
            hooked
        }
        None => std::rc::Rc::new(transport_ws::LiveWs::new(&url, codec.clone(), boot.policy)?),
    };

    // The UnmountHandle returned by mount_to unmounts the view when dropped — `let _ =` tears the UI down as soon as it mounts
    // (the symptom is exactly "ws works but the UI is blank"). It must be forgotten to live for the whole app lifetime.
    leptos::mount::mount_to(el, move || {
        // Must be assembled inside the Leptos runtime: `Ctx::new` uses Executor::spawn_local, and calling
        // it before mount_to panics ("before a global executor was initialized").
        let mut ctx = Ctx::new(transport, codec);
        // The host's seams, registered before the first frame can be consumed (ADR 0047). The names
        // are the page's; the closures hold only ids, so nothing here is a JS type in `Ctx`.
        for (name, id) in slots {
            ctx.hooks.slots.push((name, slot_observer(id)));
        }
        for (name, id) in dests {
            ctx.hooks.dests.push((name, dest_rewriter(id)));
        }
        ctx.set_config_slot(boot.config.clone());
        // Mirror page options onto the document root (ADR 0038 §3): things like `skin` are sent down when the producer builds the page,
        // and CSS only looks at `html[data-<name>]`. One line of assembly; a host swapping the boot does the same one line.
        ctx.mirror_config();
        publish_page_send(ctx.clone());
        // Reactive root: rebuilt only when the layout changes
        move || {
            let layout = ctx.layout.get();
            ui_core::render::dispatch(&layout, &ctx)
        }
    })
    .forget();
    Ok(())
}

/// One address component, escaped the way a query string wants it.
fn encode(text: &str) -> String {
    js_sys::encode_uri_component(text).as_string().unwrap_or_else(|| text.to_owned())
}

/// A mount failure is a state the page must surface: the log *and* the element we were going to mount into.
/// A blank page plus a devtools line is how a configuration mistake gets read as a renderer bug.
fn report(message: &str) {
    tracing::error!("fluxen: mount failed: {message}");
    web_sys::console::error_1(&JsValue::from_str(message));
    if let Some(doc) = web_sys::window().and_then(|w| w.document())
        && let Ok(Some(el)) = doc.query_selector("#fluxen")
    {
        el.set_text_content(Some(message));
    }
}

/// Page-level uplink handle: `window.fluxen.send(event, data)`.
///
/// It is the same leg the canvas module gets as `host.send` — both land on `Ctx::send`;
/// the page has only this one uplink channel. The framework's client runtime uses it to send hello and nav.
///
/// Calls made while the connection is still opening are not lost: `emit` is a future, and gloo's `Sink::poll_ready`
/// suspends while `readyState == 0` and wakes on `open`, so all this does is spawn.
fn publish_page_send(ctx: Ctx) {
    let send = Closure::<dyn FnMut(JsValue, JsValue)>::new(move |name: JsValue, data: JsValue| {
        let Some(name) = name.as_string() else {
            web_sys::console::warn_1(&JsValue::from_str(
                "fluxen.send: event name must be a string",
            ));
            return;
        };
        // `Outflow.data` is a serde_json::Value while the page hands over an arbitrary JS value:
        // unserializable values like undefined become null, so they never turn into a bogus frame.
        let data = js_sys::JSON::stringify(&data)
            .ok()
            .and_then(|text| serde_json::from_str(&String::from(text)).ok())
            .unwrap_or(Value::Null);
        let ctx = ctx.clone();
        leptos::task::spawn_local(async move {
            ctx.send(name, None, data).await;
        });
    });

    let handle = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &handle,
        &JsValue::from_str("send"),
        send.as_ref().unchecked_ref(),
    );
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Reflect::set(&window, &JsValue::from_str("fluxen"), &handle);
    }
    // Live until the page unloads: reclaiming it would pull the handle away (same reasoning as mount's forget).
    send.forget();
}
