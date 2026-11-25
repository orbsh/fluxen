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
use settings::{Boot, Opts, Query};
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
    if let Err(e) = boot(el, None, None) {
        report(&e);
    }
}

/// Mount explicitly: `await mount(document.querySelector('#app'), { host, path, codec, token, config,
/// policy, transport })`.
///
/// The URL's `?token=` and `?codec=` are the producer's word and apply where `options` is silent; every field
/// the call names wins, quietly, because an override is not a disagreement (ADR 0045 §2). `transport` is a live
/// hook object — the same shape as `window.fluxenTransport` — and passing it is how a page that boots itself
/// keeps its own socket.
#[wasm_bindgen]
pub fn mount(element: web_sys::Element, options: JsValue) -> Result<(), JsValue> {
    let el = element
        .dyn_into::<web_sys::HtmlElement>()
        .map_err(|_| JsValue::from_str("fluxen: mount() needs an HTML element"))?;

    let mut opts = None;
    let mut hook = None;
    if !options.is_undefined() && !options.is_null() {
        // A hook is a live object, not data: take it off first, then deserialize the rest, so an unknown key
        // or a mis-shaped value is still a loud refusal.
        if let Ok(found) = js_sys::Reflect::get(&options, &JsValue::from_str("transport"))
            && !found.is_undefined()
            && !found.is_null()
        {
            hook = Some(found);
        }
        // Deserialize a copy without that key: `deny_unknown_fields` is the rule, and a key set to
        // `undefined` is still a key — the caller's object is not ours to rewrite either.
        let source = js_sys::Object::from(options.clone());
        let rest = js_sys::Object::assign(&js_sys::Object::new(), &source);
        let _ = js_sys::Reflect::delete_property(&rest, &JsValue::from_str("transport"));
        opts =
            Some(serde_wasm_bindgen::from_value(rest.into()).map_err(|e| {
                JsValue::from_str(&format!("fluxen: mount(element, options): {e}"))
            })?);
    }

    boot(el, opts, hook).map_err(|e| JsValue::from_str(&e))
}

/// One boot for both entrances: resolve, open the socket, mount the root, publish the uplink handle.
fn boot(
    el: web_sys::HtmlElement,
    opts: Option<Opts>,
    hook_value: Option<JsValue>,
) -> Result<(), String> {
    // Mounting is a page-level operation; every step can fail (not in a browser, WS won't connect); these are
    // states the caller must surface, so they go through Result, no panic.
    let win = web_sys::window().ok_or("mount() outside a browser (no window)")?;
    let doc = win.document().ok_or("no document")?;
    let loc = doc.location().ok_or("no document.location")?;

    let query = query_of(&loc);
    let boot = Boot::resolve(&query, opts.as_ref())?;

    let host = match &boot.host {
        Some(h) => h.clone(),
        None => loc.host().map_err(|_| "document.location has no host")?,
    };
    let codec_str = match boot.codec {
        CodecType::Json => "json",
        CodecType::Cbor => "cbor",
    };
    let url = match &boot.token {
        Some(token) => format!(
            "ws://{}{}?token={}&codec={}",
            host, boot.path, token, codec_str
        ),
        None => format!("ws://{}{}?codec={}", host, boot.path, codec_str),
    };
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
        let ctx = Ctx::new(transport, codec);
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

/// The address's opinion — `?token=` and `?codec=`, the two a producer can put on a URL it hands out.
fn query_of(loc: &web_sys::Location) -> Query {
    let mut query = Query::default();
    if let Ok(href) = loc.href()
        && let Ok(url) = web_sys::Url::new(&href)
    {
        let params = url.search_params();
        query.token = params.get("token");
        query.codec = params.get("codec");
    }
    query
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
