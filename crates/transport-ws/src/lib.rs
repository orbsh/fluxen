//! WebSocket 传输实现（gloo-net，wasm 环境，框架无关）。
//!
//! 框架绑定的部分（连接状态信号、字节流桥接到框架 signal）留在各 UI crate；
//! 本 crate 只提供纯粹的 `Transport` 实现：连接、emit 编码上行、on 吐下行帧。

use content::codec::ActiveCodec;
use content::Outflow;
use std::future::Future;
use futures::stream::{LocalBoxStream, SplitSink};
use futures::StreamExt;
use futures::{SinkExt, Stream};
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use transport::{Event, Transport};

type SharedRx = Rc<RefCell<futures::channel::mpsc::UnboundedReceiver<Vec<u8>>>>;

/// WebSocket 传输。wasm 单线程，`Rc<RefCell<...>>` 持有收发两端。
pub struct WsTransport {
    write: Rc<RefCell<SplitSink<WebSocket, Message>>>,
    rx: SharedRx,
    codec: ActiveCodec,
}

impl WsTransport {
    /// 连接并启动读循环。下行帧通过内部 channel 桥接为流。
    pub fn new(url: &str, codec: ActiveCodec) -> Self {
        let ws = WebSocket::open(url).expect("ws open failed");
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

        Self {
            write: Rc::new(RefCell::new(write)),
            rx: Rc::new(RefCell::new(rx)),
            codec,
        }
    }
}

/// 共享 receiver 的流包装：wasm 单线程下多个消费者顺序拉取同一流。
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
            event: event.name,
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
