
//! ui-leptos — the REFERENCE BOOT: page shell + WS wiring + the JS uplink handle.
//!
//! Everything that is rendering lives in `ui-core`; what stays here is the part a host is free to
//! replace, and this crate is one such host (ours): read `#main`, open the socket, hand the page a
//! `window.fluxen.send` handle. A host that wraps the core in its own binary (prism) writes its own
//! boot and never depends on this crate — the ownership rule is
//! `docs/decisions/0021-external-integration.md`, the section on ownership (`## 归属`).

// Unit tests may use the unwrap/panic vocabulary the production targets are
// denied (a panicking test is a failing test). `cfg(test)` is on only for this
// crate's own test build; `tests/` crates set the same line at their own root.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn))]
use leptos::prelude::Get;

use content::codec::{ActiveCodec, CodecType};
use serde_json::Value;
use ui_core::Ctx;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

/// Entry point: resolve host/token/codec/path from `#main`, establish the WS, mount the reactive root,
/// and hand the page-level uplink handle (`window.fluxen.send`) to the page's JS.
pub fn mount() -> Result<(), String> {
    // Mounting is a page-level operation; every step can fail (not in a browser, page lacks #main,
    // WS won't connect); these are states the caller must surface, so they go through Result, no panic.
    let win = web_sys::window().ok_or("mount() outside a browser (no window)")?;
    let doc = win.document().ok_or("no document")?;
    let loc = doc.location().ok_or("no document.location")?;
    let mut host = "".to_owned();
    let mut token = None;
    let mut codec_type = CodecType::Cbor;
    let mut path = "/channel".to_owned();
    // The slot this page's options live in (ADR 0038 §2 revised): `data-config` names it, absent means the
    // default, and an empty attribute says the page has no options at all.
    let mut config_slot = Some(ui_core::ctx::DEFAULT_CONFIG_SLOT.to_owned());

    if let Ok(Some(ele)) = doc.query_selector("#main") {
        if let Some(h) = ele.get_attribute("data-host") {
            host = h;
        } else {
            host = loc.host().map_err(|_| "document.location has no host")?;
        }

        // The page's option slot (ADR 0038 §2 revised): `data-config` names it. Absent = the default name,
        // an empty attribute = this page has no options at all (no slot, no mirrored attributes).
        if let Some(c) = ele.get_attribute("data-config") {
            let name = c.trim();
            config_slot = (!name.is_empty()).then(|| name.to_owned());
        }

        // Page-level config: the framework pins its channel path and encoding on `#main` (the host may not use `/channel`,
        // or may only accept JSON). The page URL's query string is more specific and still wins.
        if let Some(p) = ele.get_attribute("data-path") {
            if p.starts_with('/') {
                path = p;
            } else {
                web_sys::console::warn_1(&JsValue::from_str(
                    "fluxen: data-path must start with '/' — keeping /channel",
                ));
            }
        }
        if let Some(c) = ele.get_attribute("data-codec")
            && let Ok(t) = c.parse::<CodecType>()
        {
            codec_type = t;
        }

        if let Ok(href) = loc.href()
            && let Ok(href) = web_sys::Url::new(&href)
        {
            if let Some(t) = href.search_params().get("token") {
                token = Some(t);
            }
            if let Some(codec_str) = href.search_params().get("codec") {
                if let Ok(t) = codec_str.parse::<CodecType>() {
                    codec_type = t;
                }
            }
        } else if let Some(t) = ele.get_attribute("data-token") {
            token = Some(t);
        }
    }

    let codec_str = match codec_type {
        CodecType::Json => "json",
        CodecType::Cbor => "cbor",
    };
    let query = if let Some(token) = token {
        format!("?token={}&codec={}", token, codec_str)
    } else {
        format!("?codec={}", codec_str)
    };
    let url = format!("ws://{}{}{}", host, path, query);
    let codec = ActiveCodec::new(codec_type);

    let parent = doc
        .query_selector("#main")
        .ok()
        .flatten()
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        .ok_or("no #main element to mount into")?;

    // Build the transport first: failing to connect is a page-level error (Result); don't mount the UI and blow up after.
    let transport: std::rc::Rc<dyn transport::Transport> = std::rc::Rc::new(
        transport_ws::WsTransport::new(&url, codec.clone())?,
    );

    // The UnmountHandle returned by mount_to unmounts the view when dropped — `let _ =` tears the UI down as soon as it mounts
    // (the symptom is exactly "ws works but the UI is blank"). It must be forgotten to live for the whole app lifetime.
    leptos::mount::mount_to(parent, move || {
        // Must be assembled inside the Leptos runtime: `Ctx::new` uses Executor::spawn_local, and calling
        // it before mount_to panics ("before a global executor was initialized").
        let ctx = Ctx::new(transport, codec);
        ctx.set_config_slot(config_slot);
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
