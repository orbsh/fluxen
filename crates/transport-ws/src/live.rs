//! A WebSocket that keeps itself alive: heartbeat, dead-link detection, and reconnection with
//! exponential backoff (ADR 0044).
//!
//! `WsTransport` (next to this file) is the honest minimum — one `open`, no retry. This one is what the
//! reference boot uses, because a page that goes deaf when a laptop wakes up is not a page. Three parts,
//! and each one exists because of a specific failure:
//!
//! * **A probe.** A browser cannot send a WebSocket *ping* from JS or wasm — those frames belong to the
//!   network stack, and were invisible even if they were not. So liveness is asked in the wire's own
//!   language: an uplink `__ping__`, answered by the host with `__pong__`. An idle page is the case that
//!   needs it, because silence is what a healthy idle page and a dead link have in common.
//! * **A dead window.** Silence past `dead_ms` means the link is gone, and the socket is closed on
//!   purpose so the supervisor starts over. Waiting for the browser to notice a half-open TCP is not a
//!   plan: a slept laptop's socket stays "open" for minutes and nothing is reported.
//! * **Backoff with a cap.** 500 ms doubling to 10 s, jittered downward so a fleet recovering from one
//!   restart does not knock in lockstep. The policy lives in `transport::reconnect`, where it is testable
//!   without a browser.
//!
//! What it does not do: resynchronise state. The page keeps rendering what it has, and the host re-sends
//! on connect (both hosts do). Uplinks emitted while the socket is down are queued and flushed on open, up
//! to a bound — a page offline for a minute should not replay a minute of typing into a fresh session.

use content::codec::ActiveCodec;
use content::{Outflow, PING_DEST};
use futures::channel::mpsc;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use transport::reconnect::Heartbeat;
use transport::{Event, Transport};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::spawn_local;

/// How many uplinks may wait for a socket. Bounded on purpose: a long outage must not become a burst of
/// stale frames when the connection returns.
const OUTBOX_CAP: usize = 256;

/// What the boot hands over: the two halves, each optional, already validated (ADR 0044 §2). `None` is off —
/// the page said so in its transport policy, and the transport is not going to second-guess it.
pub use transport::reconnect::Policy as LiveConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    Connecting,
    Open,
    Reconnecting,
    /// Connected once, then gone, and not coming back: `retry: false` was asked for.
    Closed,
}

impl ConnState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Open => "open",
            Self::Reconnecting => "reconnecting",
            Self::Closed => "closed",
        }
    }
}

/// What the supervisors share. Single-threaded wasm, so `Rc<Cell>` is the whole locking story.
struct Shared {
    ws: RefCell<Option<Rc<web_sys::WebSocket>>>,
    /// Uplinks written while the socket was not open, flushed in order on the next `open`.
    outbox: RefCell<Vec<Vec<u8>>>,
    last_inbound: Cell<f64>,
    last_probe: Cell<f64>,
    state: Cell<ConnState>,
    /// Consecutive failed attempts; reset by a successful open. The backoff counts these.
    attempts: Cell<u32>,
    frames_in: Cell<u64>,
    frames_out: Cell<u64>,
    dropped: Cell<u64>,
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// A debug hatch the gates read: `window.__fluxenConn = {state, attempts, framesIn, framesOut, dropped,
/// silentMs}`. Not API — it is how a browser test can see the socket's opinion of itself, which is
/// otherwise invisible from outside the wasm.
fn publish(shared: &Shared) {
    let Some(win) = web_sys::window() else { return };
    let o = js_sys::Object::new();
    let set = |k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(&o, &JsValue::from_str(k), &v);
    };
    set("state", JsValue::from_str(shared.state.get().as_str()));
    set(
        "attempts",
        JsValue::from_f64(f64::from(shared.attempts.get())),
    );
    set("framesIn", JsValue::from_f64(shared.frames_in.get() as f64));
    set(
        "framesOut",
        JsValue::from_f64(shared.frames_out.get() as f64),
    );
    set("dropped", JsValue::from_f64(shared.dropped.get() as f64));
    // Absolute, not "how long since": a reader that wants the silence computes it against its own clock,
    // and the value stays meaningful even when nothing has republished the object for a while.
    set("lastInbound", JsValue::from_f64(shared.last_inbound.get()));
    let _ = js_sys::Reflect::set(&win, &JsValue::from_str("__fluxenConn"), &o);
}

/// The reconnecting transport.
pub struct LiveWs {
    shared: Rc<Shared>,
    codec: ActiveCodec,
    rx: Rc<RefCell<mpsc::UnboundedReceiver<Vec<u8>>>>,
}

impl LiveWs {
    /// Open the first connection and start both supervisors. The transport is usable immediately: an
    /// `emit` before the socket opens is queued, not lost.
    pub fn new(url: &str, codec: ActiveCodec, cfg: LiveConfig) -> Result<Self, String> {
        if let Some(hb) = cfg.heartbeat {
            hb.checked()?;
        }
        if web_sys::window().is_none() {
            return Err("LiveWs: no window (not a browser)".into());
        }
        let (tx, rx) = mpsc::unbounded::<Vec<u8>>();
        let shared = Rc::new(Shared {
            ws: RefCell::new(None),
            outbox: RefCell::new(Vec::new()),
            last_inbound: Cell::new(now_ms()),
            last_probe: Cell::new(0.0),
            state: Cell::new(ConnState::Connecting),
            attempts: Cell::new(0),
            frames_in: Cell::new(0),
            frames_out: Cell::new(0),
            dropped: Cell::new(0),
        });
        publish(&shared);

        // Supervisor 1 — connect, wait for the socket to die, back off, repeat.
        {
            let shared = Rc::clone(&shared);
            let url = url.to_owned();
            let tx = tx.clone();
            let codec2 = codec.clone();
            spawn_local(async move {
                loop {
                    let closed = connect(&shared, &url, &tx, &codec2);
                    let _ = closed.await;
                    shared.ws.replace(None);
                    let Some(backoff) = cfg.backoff else {
                        // Retry is off: the socket is done, and the page is told so instead of being
                        // left in a state that looks like it is about to reconnect.
                        shared.state.set(ConnState::Closed);
                        publish(&shared);
                        return;
                    };
                    let attempt = shared.attempts.get() + 1;
                    shared.attempts.set(attempt);
                    shared.state.set(ConnState::Reconnecting);
                    publish(&shared);
                    let wait = backoff.wait_ms(attempt, roll_percent());
                    web_sys::console::warn_1(&JsValue::from_str(&format!(
                        "fluxen: link down (attempt {attempt}), retrying in {wait} ms"
                    )));
                    sleep_ms(wait).await;
                }
            });
        }

        // Supervisor 2 — the heartbeat, unless the page turned it off (`heartbeat: 0`). One interval for
        // the page's lifetime; it reads the shared state, so a reconnect does not restart it.
        if let Some(hb) = cfg.heartbeat {
            let shared = Rc::clone(&shared);
            let codec = codec.clone();
            let timer = Closure::<dyn FnMut()>::new(move || tick(&shared, &codec, hb));
            if let Some(win) = web_sys::window() {
                let _ = win.set_interval_with_callback_and_timeout_and_arguments_0(
                    timer.as_ref().unchecked_ref(),
                    hb.probe_ms.min(i32::MAX as u64) as i32,
                );
                // Live as long as the page: reclaiming it would stop the heartbeat (same reasoning as
                // every other page-lifetime handle in the boot).
                timer.forget();
            }
        }

        Ok(Self {
            shared,
            codec,
            rx: Rc::new(RefCell::new(rx)),
        })
    }

    pub fn state(&self) -> ConnState {
        self.shared.state.get()
    }

    fn write(&self, bytes: Vec<u8>) {
        if self.shared.state.get() == ConnState::Open && self.send_now(&bytes) {
            return;
        }
        let mut out = self.shared.outbox.borrow_mut();
        if out.len() >= OUTBOX_CAP {
            out.remove(0);
            self.shared.dropped.set(self.shared.dropped.get() + 1);
        }
        out.push(bytes);
    }

    fn send_now(&self, bytes: &[u8]) -> bool {
        send_bytes(&self.shared, &self.codec, bytes)
    }
}

/// Write bytes on the current socket, if there is one. Returns whether they went out.
fn send_bytes(shared: &Shared, codec: &ActiveCodec, bytes: &[u8]) -> bool {
    let Some(ws) = shared.ws.borrow().clone() else {
        return false;
    };
    let sent = match codec {
        ActiveCodec::Json => std::str::from_utf8(bytes)
            .map(|text| ws.send_with_str(text).is_ok())
            .unwrap_or(false),
        // `binaryType = arraybuffer` is set at connect, so a `Uint8Array` here is the CBOR frame itself.
        ActiveCodec::Cbor => ws.send_with_u8_array(bytes).is_ok(),
    };
    if sent {
        shared.frames_out.set(shared.frames_out.get() + 1);
    }
    sent
}

impl Transport for LiveWs {
    fn emit(&self, event: Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        let out = Outflow {
            dest: event.name,
            id: event.id,
            data: event.data,
        };
        if let Ok(bytes) = self.codec.encode(&out) {
            self.write(bytes);
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

/// One connection attempt: build the socket, wire its handlers, hand back a future that resolves when the
/// attempt is over (close, or an error the browser does not follow with a close).
fn connect(
    shared: &Rc<Shared>,
    url: &str,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    codec: &ActiveCodec,
) -> futures::channel::oneshot::Receiver<()> {
    let (done, closed) = futures::channel::oneshot::channel::<()>();
    let done = Rc::new(RefCell::new(Some(done)));

    let ws = match web_sys::WebSocket::new(url) {
        Ok(ws) => Rc::new(ws),
        Err(_) => {
            if let Some(tx) = done.borrow_mut().take() {
                let _ = tx.send(());
            }
            return closed;
        }
    };
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    {
        let shared = Rc::clone(shared);
        let ws_open = Rc::clone(&ws);
        let onopen = Closure::<dyn FnMut()>::new(move || {
            shared.attempts.set(0);
            shared.state.set(ConnState::Open);
            shared.last_inbound.set(now_ms());
            publish(&shared);
            // Flush what waited: this is what keeps an emit made during a reconnect from vanishing.
            let pending: Vec<Vec<u8>> = shared.outbox.borrow_mut().drain(..).collect();
            for bytes in pending {
                if ws_open.send_with_u8_array(&bytes).is_err() {
                    shared.dropped.set(shared.dropped.get() + 1);
                    break;
                }
                shared.frames_out.set(shared.frames_out.get() + 1);
            }
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();
    }

    {
        let shared = Rc::clone(shared);
        let tx = tx.clone();
        let onmessage =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
                let data = e.data();
                let bytes: Option<Vec<u8>> = if let Some(text) = data.as_string() {
                    Some(text.into_bytes())
                } else {
                    data.dyn_ref::<js_sys::ArrayBuffer>()
                        .map(|buf| js_sys::Uint8Array::new(buf).to_vec())
                };
                let Some(bytes) = bytes else { return };
                shared.last_inbound.set(now_ms());
                shared.frames_in.set(shared.frames_in.get() + 1);
                // The host's answer to our probe is transport business: swallow it, so no decoder ever
                // sees a frame that is not an action list. Checked in both codecs — the host's answer
                // does not have to arrive in the encoding we send in.
                if !content::is_transport_frame(&bytes) {
                    let _ = tx.unbounded_send(bytes);
                }
                publish(&shared);
            });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
    }

    {
        // One `end` per handler: the oneshot may be taken by whichever fires first, and both handlers
        // need their own handle to try.
        let end =
            |shared: Rc<Shared>,
             done: Rc<RefCell<Option<futures::channel::oneshot::Sender<()>>>>| {
                move || {
                    shared.state.set(ConnState::Reconnecting);
                    // Frames that never reached the wire belong to the connection that just died, and they
                    // are not carried into the next one (ADR 0047 §2). A click from the previous session,
                    // replayed into a new one, is an action nobody claims — worse than losing it, so it is
                    // dropped here with a count, not silently at the next open.
                    let dropped = {
                        let mut out = shared.outbox.borrow_mut();
                        let n = out.len();
                        out.clear();
                        n
                    };
                    if dropped > 0 {
                        web_sys::console::warn_1(
                            &format!("fluxen: {dropped} unsent frame(s) dropped with the connection").into(),
                        );
                    }
                    publish(&shared);
                    if let Some(tx) = done.borrow_mut().take() {
                        let _ = tx.send(());
                    }
                }
            };
        let onerror = Closure::<dyn FnMut()>::new(end(Rc::clone(shared), Rc::clone(&done)));
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();

        let onclose = Closure::<dyn FnMut()>::new(end(Rc::clone(shared), Rc::clone(&done)));
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();
    }

    shared.ws.replace(Some(ws));
    closed
}

/// The heartbeat tick: declare the link dead after `dead_ms` of silence, otherwise ask whether it is idle.
fn tick(shared: &Rc<Shared>, codec: &ActiveCodec, hb: Heartbeat) {
    if shared.state.get() != ConnState::Open {
        return;
    }
    let now = now_ms();
    let silent = now - shared.last_inbound.get();
    if silent >= hb.dead_ms as f64 {
        web_sys::console::warn_1(&JsValue::from_str(&format!(
            "fluxen: {silent:.0} ms of silence — closing the socket to reconnect"
        )));
        if let Some(ws) = shared.ws.borrow().as_ref() {
            let _ = ws.close();
        }
        shared.state.set(ConnState::Reconnecting);
        publish(shared);
        return;
    }
    // Only ask when nothing else is talking: a frame inside the last window is proof enough.
    if silent >= hb.probe_ms as f64 && now - shared.last_probe.get() >= hb.probe_ms as f64 {
        shared.last_probe.set(now);
        if let Ok(bytes) = codec.encode(&Outflow {
            dest: PING_DEST.to_owned(),
            id: None,
            data: serde_json::Value::Null,
        }) {
            send_bytes(shared, codec, &bytes);
        }
    }
}

fn roll_percent() -> u32 {
    (js_sys::Math::random() * 100.0) as u32
}

/// A pending timeout, released with the future that awaited it.
///
/// `Closure::once_into_js` cannot be used for this timer: it hands JS a function that releases itself
/// on the first call, so a second call traps in wasm-bindgen's trampoline (`_wbg_cb_unref` on freed
/// state → `unreachable`) and the trap carries no panic message — the failure looks like a silent wasm
/// abort with only function numbers in the stack. Holding the closure here and clearing the timeout on
/// drop means a stray second fire cannot happen: the callback is gone when the wait is abandoned.
struct Timer {
    window: web_sys::Window,
    id: i32,
    _cb: Closure<dyn FnMut()>,
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.window.clear_timeout_with_handle(self.id);
    }
}

fn sleep_ms(ms: u64) -> impl std::future::Future<Output = ()> {
    let (tx, rx) = futures::channel::oneshot::channel::<()>();
    // One send, whichever way it arrives: a callback that fires after the waiter gave up finds the
    // sender taken and does nothing.
    let tx = Rc::new(RefCell::new(Some(tx)));
    let cb = {
        let tx = Rc::clone(&tx);
        Closure::<dyn FnMut()>::new(move || {
            if let Some(tx) = tx.borrow_mut().take() {
                let _ = tx.send(());
            }
        })
    };
    let timer = web_sys::window().map(|window| {
        let id = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                ms.min(i32::MAX as u64) as i32,
            )
            .unwrap_or(0);
        Timer { window, id, _cb: cb }
    });
    async move {
        let _ = rx.await;
        drop(timer);
    }
}
