//! WebSocket transport implementation (gloo-net, wasm environment, framework agnostic).
//!
//! The framework-bound parts (connection-state signals, bridging the byte stream into a framework signal) stay in each UI crate;
//! this crate only provides the pure `Transport` implementation: connect, emit encodes upstream, on yields downstream frames.

use content::Outflow;
use content::codec::ActiveCodec;
use futures::StreamExt;
use futures::stream::{LocalBoxStream, SplitSink};
use futures::{SinkExt, Stream};
use gloo_net::websocket::Message;
use gloo_net::websocket::futures::WebSocket;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use transport::{Event, Transport};

type SharedRx = Rc<RefCell<futures::channel::mpsc::UnboundedReceiver<Vec<u8>>>>;

/// WebSocket transport. wasm is single-threaded; `Rc<RefCell<...>>` holds both the send and receive ends.
pub struct WsTransport {
    write: Rc<RefCell<SplitSink<WebSocket, Message>>>,
    rx: SharedRx,
    codec: ActiveCodec,
}

impl WsTransport {
    /// Connect and start the read loop. Downstream frames are bridged into a stream via an internal channel.
    ///
    /// Failure returns an error instead of panicking: being unable to connect (server not up, proxy/handshake
    /// rejected) is a state the page must be able to surface, not a process-level impossibility.
    pub fn new(url: &str, codec: ActiveCodec) -> Result<Self, String> {
        let ws = WebSocket::open(url).map_err(|e| format!("ws open failed: {e:?}"))?;
        let (write, mut read) = ws.split();
        let (tx, rx) = futures::channel::mpsc::unbounded();

        wasm_bindgen_futures::spawn_local(async move {
            while let Some(Ok(m)) = read.next().await {
                let bytes = match m {
                    Message::Text(t) => t.into_bytes(),
                    Message::Bytes(b) => b,
                };
                let _ = tx.unbounded_send(bytes);
            }
        });

        Ok(Self {
            write: Rc::new(RefCell::new(write)),
            rx: Rc::new(RefCell::new(rx)),
            codec,
        })
    }
}

/// Stream wrapper around the shared receiver: under single-threaded wasm several consumers poll the same stream in turn.
struct SharedRxStream(SharedRx);

impl Stream for SharedRxStream {
    type Item = Vec<u8>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.borrow_mut().poll_next_unpin(cx)
    }
}

impl Transport for WsTransport {
    fn emit(&self, event: Event) -> Pin<Box<dyn Future<Output = ()>>> {
        let out = Outflow {
            dest: event.name,
            id: event.id,
            data: event.data,
        };
        if let Ok(buf) = self.codec.encode(&out) {
            let msg = match &self.codec {
                ActiveCodec::Json => Message::Text(String::from_utf8(buf).unwrap_or_default()),
                ActiveCodec::Cbor => Message::Bytes(buf),
            };
            let write = Rc::clone(&self.write);
            Box::pin(async move {
                let _ = write.borrow_mut().send(msg).await;
            })
        } else {
            Box::pin(async {})
        }
    }

    fn on(&self) -> LocalBoxStream<'static, Vec<u8>> {
        Box::pin(SharedRxStream(Rc::clone(&self.rx)))
    }
}
