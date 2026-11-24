use crate::Ctx;
use accrete::classify::Classify;
use accrete::{AccreteOps, Canvas};
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// In-place mounting of an external render module (ADR 0007). This is the **only** visualization host:
///
/// The core does not know what a module is: the url points to an ES module that must export
/// mount/update/resize/unmount, four functions; the data area (the `bind["value"]` convention)
/// is pushed to the module as CBOR bytes, and the semantics belong entirely to the module (3dbrowser uses
/// `{primitives, assets}`, g2chart uses a G2 spec). The render loop and input events belong to
/// the module itself; the host only provides the container, payload-change notification and size.
///
/// The host does not know what a module is and does not classify it: `url` points to an ES module that exports four functions,
/// and the kind lives only in the data. Charts, editors, relation graphs and flowcharts are the same node + different urls;
/// the framework has no names for them (ADR 0020).
pub fn canvas_(accrete: Canvas, ctx: &Ctx, id: String) -> AnyView {
    let style = accrete
        .attrs
        .as_ref()
        .map(|a| a.size_style())
        .unwrap_or_default();
    let css = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.get_class().clone().map(|c| c.join(" ")))
        .unwrap_or_default();
    let (source, inline) = bind_value(&accrete);
    mount_module(
        &id,
        &accrete.url,
        style,
        css,
        source,
        inline,
        accrete.options.clone(),
        ctx,
    )
}



/// `source` → (scope, slot name, inline default). A literal keeps its default; a
/// slot keeps its address, and the scope says which plane it is read from.
fn bind_value(a: &impl AccreteOps) -> (Option<(accrete::Scope, String)>, Option<Value>) {
    match a.get_source() {
        Some(b) => match &b.slot {
            Some(slot) => (Some((b.scope, slot.clone())), None),
            None => (None, b.default.clone()),
        },
        None => (None, None),
    }
}

/// Empty-payload criterion (ADR 0012): the value the host synthesises for a missing/empty slot — `Null` or a zero-entry
/// map, i.e. "no data yet" for `payload` below. An empty data area = takes no place: the container collapses,
/// and opens up to the node's size the first time the payload is non-empty. Anything other than `{}`/`Null` (an empty string,
/// an empty array included) counts as "has data"; the semantics look only at whether there is data, never guessing at content.
pub(crate) fn payload_is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Object(m) => m.is_empty(),
        _ => false,
    }
}

/// The container's style while it takes no place (ADR 0012). Inline style rather than the `hide` class: `.f` and
/// `.hide` on the same div were measured suppressing each other (see Pitfalls), and inline has no such ambiguity. Shared by the canvas/chart host
/// and layer (ADR 0014).
pub(crate) const HIDDEN_STYLE: &str = "display: none;";

/// All of the mounting logic: resolve the payload closure → import(url) → contract check → mount →
/// update subscription → ResizeObserver → unmount. Every failure path is warn + empty container
/// (fail-loud, matching the ADR 0003 convention).
fn mount_module(
    id: &str,
    url: &str,
    style: String,
    css: String,
    source: Option<(accrete::Scope, String)>,
    inline_default: Option<Value>,
    options: Option<Value>,
    ctx: &Ctx,
) -> AnyView {
    // Payload resolution: a literal default is used directly; with a slot, read the node in the slot (its source.default
    // first, otherwise the whole node JSON), and `scope: local` looks at this page's value plane first.
    // Streaming data = set/patch on the source slot (ADR 0005 pointer protocol), zero new machinery.
    // Empty slot/no payload = `{}`: every field of a module's spec has a default, so an empty map is a legitimate
    // "no data yet"; feeding null would make a module with strict decode reject at mount — and
    // "mount the empty layout first, write the data slot later" is exactly the standard streaming order (a defect caught by e2e).
    let empty = Value::Object(Default::default());
    let inline = inline_default.unwrap_or(Value::Null);
    let payload: Rc<dyn Fn() -> Value> = {
        let source = source.clone();
        let ctx = ctx.clone();
        let empty = empty.clone();
        Rc::new(move || {
            if let Some((scope, src)) = &source {
                // A local assignment (ADR 0031 §1) is the payload as it stands.
                if scope.local()
                    && let Some(v) = ctx.slot_for_value(src).get_untracked()
                {
                    return v;
                }
                return match ctx.slot_for_data(src).get_untracked().map(|a| (*a).clone()) {
                    Some(node) => node
                        .get_source()
                        .and_then(|b| b.default.clone())
                        .unwrap_or_else(|| serde_json::to_value(&node).unwrap_or(empty.clone())),
                    None => empty.clone(),
                };
            }
            if inline.is_null() {
                empty.clone()
            } else {
                inline.clone()
            }
        })
    };

    // Placement follows the payload (ADR 0012): an empty data area = takes no place. The criterion is the same "no
    // data yet" as `payload` (see `payload_is_empty`) — this only extends it from payload semantics to placement
    // semantics, zero new vocabulary and the wire is unchanged.
    // This gives only the value at **the moment of construction** (it decides before the module mounts and again after a row rebuild);
    // the sync after mounting happens in the update Effect below: a reactive attribute (`.style(closure)`) requires
    // the closure to be `Send`, while the emptiness test must read a slot (`Ctx` is not Send), hence the imperative write through an Effect.
    // Opening up does not remount: once the container goes from hidden to visible and the size is in place, the module's own
    // ResizeObserver → resize() takes over redrawing; import does not rerun and the GL context is not lost.
    let style_now = if payload_is_empty(&payload()) {
        HIDDEN_STYLE.to_string()
    } else {
        style.clone()
    };
    // When non-empty, write the node's size style back to the container: the mounting section (update Effect) and the payload guard each hold one.
    let size_style = style.clone();
    let style_w = style;

    let nr = NodeRef::<Div>::new();
    let el_id = id.to_string();
    let ctx2 = ctx.clone();
    let url = url.to_string();
    // Module-private options (ADR 0017): the wire Value is serialised verbatim as JSON text, and only at the mount hop
    // is it `JSON.parse`d into a JS object (fourth argument). No CBOR — this is one-off configuration at mount time, not streaming,
    // so modules need not carry a decoder for it; the data plane (data) is still CBOR, the two legs have different jobs.
    // Absent/Null = `{}`: the module applies its own defaults, the host guesses no keys.
    let options_json = match options {
        None | Some(Value::Null) => "{}".to_string(),
        Some(v) => v.to_string(),
    };
    nr.on_load(move |el| {
        // on_load hands back the very div the host built, so the type narrowing can only succeed.
        #[allow(clippy::expect_used)]
        let el: web_sys::HtmlElement = el.dyn_into().expect("div is element");
        // Idempotency guard (an in-place rebuild reruns on_load): the state is written on the element's attribute.
        // 1) marked "true" and the container still has content the module wrote into it → alive, do nothing;
        // 2) marked "true" but the content was wiped by a rebuild (tachys rewrites the host's child nodes per view, leaving only a
        //    placeholder comment) → clear the marker = treat as never mounted, and the guard below remounts;
        // 3) anything else → install the guard ("pending" = mount in flight, the guard only syncs style, does not re-issue).
        // CDN modules like splatviewer import slowly, so a row rebuild inevitably lands inside the mount window,
        // which is why only it ever exposed this; 3dbrowser is local wasm, milliseconds, and never hit it.
        let mounted = el.get_attribute("data-module-mounted");
        if mounted.as_deref() == Some("true") && el.child_element_count() > 0 {
            return;
        }
        if mounted.as_deref() == Some("true") {
            el.remove_attribute("data-module-mounted").ok();
        }

        // ADR 0007's import + mount is triggered by **the first non-empty payload**, not hooked on on_load
        // (user ruling 2026-10-14: a component's library must be "loaded on use" — a canvas bound to a slot whose data has
        // not arrived must not pull down three.js / G2 / wasm first). ADR 0012's
        // "an empty payload takes no place" therefore extends from container style to module loading: empty payload = zero import.
        // This Effect is the only mount initiator: read the slot → sync the placement style → payload non-empty and the host
        // has no mount marker yet → import + mount; later changes go through the existing update leg.
        // The mount marker is still the single source of state, so a second guard installed by a rebuild rerunning on_load
        // will not initiate twice.
        let el_w = el.clone();
        let id_w = el_id.clone();
        let url_w = url.clone();
        let ctx_w = ctx2.clone();
        let source_w = source.clone();
        let payload_w = Rc::clone(&payload);
        let style_w = style_w.clone();
        let options_w = options_json.clone();
        Effect::new(move |_| {
            if let Some((scope, src)) = &source_w {
                if scope.local() {
                    let _ = ctx_w.slot_for_value(src).get();
                }
                if scope.remote() {
                    let _ = ctx_w.slot_for_data(src).get();
                }
            }
            let p = payload_w();
            // ADR 0012: placement follows the payload — an empty payload collapses the container, a non-empty one writes the node's size back.
            // Every run writes (idempotently). Opening up does not remount: once the size is in place, the module's own
            // ResizeObserver → resize() takes over; import does not rerun and the GL context is not lost.
            let want = if payload_is_empty(&p) {
                HIDDEN_STYLE.to_string()
            } else {
                style_w.clone()
            };
            el_w.set_attribute("style", &want).ok();
            if payload_is_empty(&p) || el_w.get_attribute("data-module-mounted").is_some() {
                return;
            }
            let el = el_w.clone();
            let id = id_w.clone();
            let url = url_w.clone();
            let ctx2 = ctx_w.clone();
            let source0 = source_w.clone();
            let payload0 = Rc::clone(&payload_w);
            let size_style = size_style.clone();
            let opts_json = options_w.clone();
            // A failed attribute write can only mean the element left the document; the marker is an idempotency guard, not a data plane.
            #[allow(clippy::expect_used)]
            el.set_attribute("data-module-mounted", "pending")
                .expect("set mounted marker");
            // The payload mount actually pushes to the module = the one that triggered the mount, and it must also be the baseline of
            // the update below: the slot must not be read again at that point — if a slot write lands inside the import/mount
            // window (an append row and a set payload sent back to back is the normal case), re-reading would take the new value as the
            // baseline, the Effect would judge "no change" → update never fires → the module stays on the empty
            // spec it mounted with. Symptom: the host is marked mounted and the canvas is there, yet not one asset is requested
            // (the import window of a local wasm module is masked by the writer's interval; a CDN module shows it every time).
            let mounted_payload = p;
            leptos::task::spawn_local(async move {
                // Initial data (CBOR bytes) — the data argument of mount
                let data0 = encode(&mounted_payload);

                // Host channel: host.send(event, cborBytes) → the existing action frame uplink.
                // The closure holds Rc<Ctx> + id, and hands the event name plus the decoded JSON to
                // ctx.send (the same uplink as a form submit).
                let host = js_sys::Object::new();
                let cb = Closure::<dyn FnMut(String, js_sys::Uint8Array)>::new(
                    {
                        let ctx = ctx2.clone();
                        let id = id.clone();
                        move |ev: String, bytes: js_sys::Uint8Array| {
                            let buf = bytes.to_vec();
                            let decoded: Value = content::codec::ActiveCodec::Cbor
                                .decode(&buf)
                                .unwrap_or(Value::Null);
                            let ctx = ctx.clone();
                            let id = id.clone();
                            leptos::task::spawn_local(async move {
                                ctx.send(ev, Some(id), decoded).await;
                            });
                        }
                    },
                );
                let _ = js_sys::Reflect::set(&host, &JsValue::from_str("send"), cb.as_ref());
                // cb is not dropped (once the closure becomes a JS function the module holds it, so the leak is the module's lifetime to manage)
                cb.into_js_value(); // hand ownership to JS, the host no longer retains it

                // Dynamic import(url). Any contract failure = warn + empty container (fail-loud, no guessing).
                let module = match import_module(&url).await {
                    Some(m) => m,
                    None => {
                        tracing::warn!("module host {id}: import {url:?} failed");
                        el.remove_attribute("data-module-mounted").ok();
                        return;
                    }
                };
                let names = ["mount", "update", "resize", "unmount"];
                let mut contract: [Option<js_sys::Function>; 4] = [None, None, None, None];
                for (slot, name) in contract.iter_mut().zip(names) {
                    *slot = js_sys::Reflect::get(&module, &JsValue::from_str(name))
                        .ok()
                        .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
                }
                let [Some(mount), Some(update), Some(resize), Some(unmount)] = contract else {
                    tracing::warn!("module host {id}: module {url:?} misses contract exports");
                    el.remove_attribute("data-module-mounted").ok();
                    return;
                };

                // mount(el, data, host, options) → ctx id (sync or Promise<ctx>)
                // options is a JS object (ADR 0017): one-off configuration at mount time, interpreted by the module itself.
                let opts: JsValue = match js_sys::JSON::parse(&opts_json) {
                    Ok(v) => v,
                    Err(e) => {
                        // The text comes from serde_json and normally cannot fail; on failure mount with an empty configuration (does not block rendering).
                        tracing::warn!("module host {id}: options not JSON: {e:?}");
                        js_sys::Object::new().into()
                    }
                };
                let args = js_sys::Array::new();
                args.push(&JsValue::from(el.clone()));
                args.push(&data0.into());
                args.push(&JsValue::from(host));
                args.push(&opts);
                let ret = match js_sys::Reflect::apply(&mount, &JsValue::UNDEFINED, &args) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("module host {id}: mount threw: {e:?}");
                        el.remove_attribute("data-module-mounted").ok();
                        return;
                    }
                };
                let cid: JsValue = if let Ok(p) = ret.clone().dyn_into::<js_sys::Promise>() {
                    match wasm_bindgen_futures::JsFuture::from(p).await {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!("module host {id}: mount rejected: {e:?}");
                            el.remove_attribute("data-module-mounted").ok();
                            return;
                        }
                    }
                } else {
                    ret
                };
                // Only set "true" once the mount actually succeeds (the guard honours only that); before that it is "pending" —
                // an in-progress mount is not re-initiated by an on_load triggered by a rebuild.
                el.set_attribute("data-module-mounted", "true").ok();

                // Data area changes → update(cid, cbor): the Effect subscribes to the source slot and compares payloads.
                {
                    // Baseline = the one pushed to the module at mount, not the slot value re-read now (see above).
                    let mut last = mounted_payload;
                    let cid = cid.clone();
                    let update = update.clone();
                    let id = id.clone();
                    let ctx = ctx2.clone();
                    let src = source0.clone();
                    let el_eff = el.clone();
                    Effect::new(move |_| {
                        if let Some((scope, src)) = &src {
                            if scope.local() {
                                let _ = ctx.slot_for_value(src).get();
                            }
                            if scope.remote() {
                                let _ = ctx.slot_for_data(src).get();
                            }
                        }
                        let p = payload0();
                        // ADR 0012: placement follows the payload — an empty payload collapses the container, a non-empty one writes the node's
                        // size back (`size_style`). Every run writes (idempotently): if the payload arrived before mounting
                        // completed, the value at construction time is still "empty", and this run is at the same time
                        // the p == last case (it cannot enter the update branch below), so it is the only
                        // chance to sync. Opening up itself does not remount — size changes are taken over by the module's own
                        // ResizeObserver → resize().
                        let want = if payload_is_empty(&p) {
                            HIDDEN_STYLE.to_string()
                        } else {
                            size_style.clone()
                        };
                        el_eff.set_attribute("style", &want).ok();
                        if p != last {
                            let d = encode(&p);
                            last = p;
                            let a = js_sys::Array::new();
                            a.push(&cid);
                            a.push(&d.into());
                            if let Err(e) = js_sys::Reflect::apply(&update, &JsValue::UNDEFINED, &a) {
                                tracing::warn!("module host {id}: update threw: {e:?}");
                            }
                        }
                    });
                }

                // Size belongs to the host: ResizeObserver watches the container (css pixels) → resize(cid, w, h).
                // dpr conversion and setting canvas.width belong to the module (see each index.js).
                {
                    let cid_ro = cid.clone();
                    let cb = Closure::<dyn FnMut(js_sys::Array, JsValue)>::new(
                        {
                            let resize = resize.clone();
                            let id = id.clone();
                            move |entries: js_sys::Array, _obs: JsValue| {
                                let e = js_sys::Reflect::get(&entries, &JsValue::from(0))
                                    .ok()
                                    .unwrap_or(JsValue::UNDEFINED);
                                let rect = js_sys::Reflect::get(&e, &JsValue::from_str("contentRect"))
                                    .ok()
                                    .unwrap_or(JsValue::UNDEFINED);
                                let num = |k: &str| {
                                    js_sys::Reflect::get(&rect, &JsValue::from_str(k))
                                        .ok()
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0)
                                };
                                let a = js_sys::Array::new();
                                a.push(&cid_ro);
                                a.push(&JsValue::from(num("width")));
                                a.push(&JsValue::from(num("height")));
                                if let Err(err) =
                                    js_sys::Reflect::apply(&resize, &JsValue::UNDEFINED, &a)
                                {
                                    tracing::warn!("module host {id}: resize threw: {err:?}");
                                }
                            }
                        },
                    );
                    let ro = web_sys::ResizeObserver::new(cb.as_ref().unchecked_ref());
                    // if ro is unavailable (old browser) sizes are simply not forwarded, which does not affect mounting
                    if let Ok(ro) = ro {
                        ro.observe(&el);
                        cb.forget(); // the observer holds the callback long-term
                        let unmount = unmount.clone();
                        let ro_disconnect = ro.clone();
                        on_cleanup(move || {
                            ro_disconnect.disconnect();
                            let a = js_sys::Array::new();
                            a.push(&cid);
                            let _ = js_sys::Reflect::apply(&unmount, &JsValue::UNDEFINED, &a);
                        });
                    } else {
                        let unmount = unmount.clone();
                        on_cleanup(move || {
                            let a = js_sys::Array::new();
                            a.push(&cid);
                            let _ = js_sys::Reflect::apply(&unmount, &JsValue::UNDEFINED, &a);
                        });
                    }
                }
            });
        });
    });

    div()
        .id(id)
        .class(css.as_str())
        .style(style_now)
        .node_ref(nr)
        .into_any()
}

/// `import(new URL(url, document.baseURI))` — relative urls work too (absolute CDN addresses pass straight through).
/// URL parsing happens entirely on the JS side: `new URL(...)` returns a URL object rather than a string,
/// so dyn_into::<JsString>() on the Rust side always fails (a defect caught on the first e2e run).
async fn import_module(url: &str) -> Option<JsValue> {
    let escaped = url.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("import(new URL(\"{escaped}\", document.baseURI).href)");
    let p = match js_sys::eval(&script) {
        Ok(v) => match v.dyn_into::<js_sys::Promise>() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("canvas import: eval did not return a Promise for {script}");
                return None;
            }
        },
        Err(e) => {
            tracing::warn!(
                "canvas import: eval threw: {:?}",
                e.as_string().unwrap_or_else(|| format!("{e:?}"))
            );
            return None;
        }
    };
    match wasm_bindgen_futures::JsFuture::from(p).await {
        // Note: an ES module namespace object has a null prototype chain, so dyn_into::<Object>()
        // (instanceof semantics) necessarily fails on it — keep the JsValue, and contract lookup goes through
        // Reflect::get, which works for any receiver.
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(
                "canvas import: promise rejected: {:?}",
                e.as_string().unwrap_or_else(|| format!("{e:?}"))
            );
            None
        }
    }
}

/// Value → CBOR bytes (JS Uint8Array → wasm Vec<u8>); hand data across the boundary, not objects.
fn encode(v: &Value) -> js_sys::Uint8Array {
    let mut out = Vec::new();
    match ciborium::ser::into_writer(v, &mut out) {
        Ok(()) => js_sys::Uint8Array::from(&out[..]),
        Err(e) => {
            tracing::warn!("canvas: payload encode failed: {e}");
            js_sys::Uint8Array::new(&JsValue::NULL)
        }
    }
}
