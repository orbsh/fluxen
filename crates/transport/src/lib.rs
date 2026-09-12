//! UI 与服务端之间的传输抽象：只管字节流的出入，不关心编解码与帧语义。
//!
//! 解码（`content::codec`）与分发（UI store 逻辑）都在消费端；
//! 实现方只需把上行事件编码前的 `Outflow` 送出去、把下行原始帧吐成流。

use futures::stream::LocalBoxStream;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// UI → 服务端的事件。与线上格式（content::Outflow）解耦，
/// 由实现方决定编码方式（ws 实现用 content codec，http 实现可直接映射为 JSON body）。
#[derive(Debug, Clone)]
pub struct Event {
    pub name: String,
    pub id: Option<String>,
    pub data: Value,
}

/// 传输层 trait：emit 出、on 入。
///
/// 字节流级别抽象使 ws / http(SSE+POST) 可互换：
/// - ws: 单连接双工，`on` 返回收到帧的流
/// - http: POST 上行 + SSE 下行，`on` 返回 SSE data 帧的流（ADR 0001）
pub trait Transport {
    /// 发送一个 UI 事件到服务端。
    fn emit(&self, event: Event) -> Pin<Box<dyn Future<Output = ()>>>;

    /// 订阅服务端下行帧的原始字节流。
    fn on(&self) -> LocalBoxStream<'static, Vec<u8>>;
}
