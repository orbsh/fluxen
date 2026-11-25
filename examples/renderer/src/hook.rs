//! `window.fluxenTransport` — the host's own transport, taken as a hook (ADR 0040).
//!
//! The hook is read once, before anything is built, and it only moves bytes: the codec stays in Rust, so a
//! page cannot end up with a JS transport that speaks a different dialect than its own decoder. A hook that
//! is present but not shaped right is a mount error, never a silent fallback — a page whose JS believes it
//! owns the socket while wasm opens a second one is the worst of both worlds and it fails invisibly.
//!
//! Reconnection is the hook's business in this mode (ADR 0040 §3, ADR 0044 §3): a JS transport owns its
//! socket, including retry and backoff. `LiveWs` (the default path) is the one that retries for us.

use content::codec::ActiveCodec;
use futures::channel::mpsc;
use std::cell::RefCell;
use std::rc::Rc;
use transport::{Event, Transport};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

const NAMES: &str = "window.fluxenTransport must be { emit(bytes), subscribe(push) }";

/// The transport a JS host installed, or `None` when it did not install one.
///
/// `Err` means the hook is there and wrong: the caller must refuse to mount.
pub fn from_window(codec: ActiveCodec) -> Result<Option<Rc<dyn Transport>>, String> {
    let Some(win) = web_sys::window() else {
        return Ok(None);
    };
    let hook = match js_sys::Reflect::get(&win, &JsValue::from_str("fluxenTransport")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return Ok(None),
    };
    let field = |name: &str| js_sys::Reflect::get(&hook, &JsValue::from_str(name)).ok();
    let emit = field("emit").filter(JsValue::is_function);
    let subscribe = field("subscribe").filter(JsValue::is_function);
    let (Some(emit), Some(subscribe)) = (emit, subscribe) else {
        let msg = format!("fluxen: {NAMES}; both are required — refusing to mount");
        web_sys::console::error_1(&JsValue::from_str(&msg));
        return Err(msg);
    };

    // Downlink: the boot hands the hook a sink and the hook pushes every frame into it.
    let (tx, rx) = mpsc::unbounded::<Vec<u8>>();
    let push = Closure::<dyn FnMut(JsValue)>::new(move |bytes: JsValue| {
        let out: Option<Vec<u8>> = if let Some(buf) = bytes.dyn_ref::<js_sys::ArrayBuffer>() {
            Some(js_sys::Uint8Array::new(buf).to_vec())
        } else {
            bytes.dyn_ref::<js_sys::Uint8Array>().map(|a| a.to_vec())
        };
        match out {
            // A hook that also answers the heartbeat must not leak its answer into the page: the same
            // rule as the default transport (ADR 0044), applied to whatever encoding the host chose.
            Some(v) if !content::is_transport_frame(&v) => {
                let _ = tx.unbounded_send(v);
            }
            Some(_) => {}
            // A hook that pushes strings is a codec opinion, and the codec is not its to have.
            None => web_sys::console::warn_1(&JsValue::from_str(
                "fluxen: fluxenTransport.subscribe pushed a non-binary value; expected a Uint8Array",
            )),
        }
    });
    let subscribe_fn = js_sys::Function::from(subscribe);
    if subscribe_fn
        .call1(&hook, push.as_ref().unchecked_ref())
        .is_err()
    {
        let msg = "fluxen: fluxenTransport.subscribe threw — refusing to mount".to_owned();
        web_sys::console::error_1(&JsValue::from_str(&msg));
        return Err(msg);
    }
    // The sink outlives this call: the hook keeps the function for the page's lifetime.
    push.forget();

    Ok(Some(Rc::new(HookTransport {
        hook,
        emit,
        rx: Rc::new(RefCell::new(rx)),
        codec,
    })))
}

struct HookTransport {
    hook: JsValue,
    emit: JsValue,
    rx: Rc<RefCell<mpsc::UnboundedReceiver<Vec<u8>>>>,
    codec: ActiveCodec,
}

impl Transport for HookTransport {
    fn emit(&self, event: Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        let out = content::Outflow {
            dest: event.name,
            id: event.id,
            data: event.data,
        };
        if let Ok(bytes) = self.codec.encode(&out) {
            let arr = js_sys::Uint8Array::from(bytes.as_slice());
            if js_sys::Function::from(self.emit.clone())
                .call1(&self.hook, arr.as_ref())
                .is_err()
            {
                web_sys::console::warn_1(&JsValue::from_str(
                    "fluxen: fluxenTransport.emit threw; the frame was not sent",
                ));
            }
        }
        Box::pin(async {})
    }

    fn on(&self) -> futures::stream::LocalBoxStream<'static, Vec<u8>> {
        use futures::StreamExt;
        struct SharedRx(Rc<RefCell<mpsc::UnboundedReceiver<Vec<u8>>>>);
        impl futures::Stream for SharedRx {
            type Item = Vec<u8>;
            fn poll_next(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Self::Item>> {
                self.0.borrow_mut().poll_next_unpin(cx)
            }
        }
        Box::pin(SharedRx(Rc::clone(&self.rx)))
    }
}
