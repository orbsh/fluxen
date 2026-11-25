//! The transport abstraction between the UI and the server: it only handles the flow of bytes in and out, and does not care about codec or frame semantics.
//!
//! Decoding (`content::codec`) and dispatch (the UI store logic) live on the consumer side;
//! an implementer only has to send out the `Outflow` from before the uplink event is encoded, and yield the raw downlink frames as a stream.

pub mod reconnect;

use futures::stream::LocalBoxStream;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// UI → server events. Decoupled from the wire format (content::Outflow),
/// with the implementer deciding the encoding (the ws implementation uses the content codec, the http implementation can map straight to a JSON body).
#[derive(Debug, Clone)]
pub struct Event {
    pub name: String,
    pub id: Option<String>,
    pub data: Value,
}

/// The transport trait: emit out, on in.
///
/// The byte-stream-level abstraction makes ws / http(SSE+POST) interchangeable:
/// - ws: one duplex connection, `on` returns the stream of received frames
/// - http: POST uplink + SSE downlink, `on` returns the stream of SSE data frames (ADR 0001)
pub trait Transport {
    /// Send one UI event to the server.
    fn emit(&self, event: Event) -> Pin<Box<dyn Future<Output = ()>>>;

    /// Subscribe to the raw byte stream of the server's downlink frames.
    fn on(&self) -> LocalBoxStream<'static, Vec<u8>>;
}
