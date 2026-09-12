# ADR 0001: UI 传输层默认 WebSocket，HTTP 作为扩展点

状态：已接受
日期：2026-09-12
仓库：fluxen

## 背景

fluxen 的 UI 与服务端之间需要一条通道：服务端下发布局命令（`content::Content` 的 Create/Set/Join/Tmpl），UI 上报用户事件（`content::Outflow`）。传输层抽象为 `transport::Transport` trait（`emit` 出、`on` 入，字节流级别），任何实现都可接入。

用户可能不用 WebSocket 而用纯 HTTP：把事件映射为路径，store 发出的消息 POST 出去；服务端→UI 方向可以 SSE 持续接收，或用户操作触发时请求后端拿数据、转成 Brick 格式放回。需要决定默认实现与取舍。

## 决策

默认实现 WebSocket（`transport-ws` crate）；HTTP 不实现，只在 trait 层面保留扩展点。

## 理由

下行方向决定了这个权衡。服务端对 UI 的推送（Create 设整棵布局、Set 写数据、Join 合并列表）是持续、双向、由服务端状态变化驱动的流。用 HTTP 承载它只有两条路：

1. SSE 下行 + POST 上行——两条独立通道，等于手工重组 WebSocket 已有的单连接双工，还要自己解决重连、消息顺序、codec 协商（ws 升级握手时一次带 `?codec=` 参数完成的事，HTTP 要拆到两个端点上各管一遍）。
2. 纯轮询/请求响应——读模型场景可用（用户操作触发、拉一次数据转成 Brick 放回 store），但对推送型命令不自然：要么丢实时性，要么退化为长轮询。

WebSocket 单连接同时覆盖两个方向，codec 协商和会话语义（`Message<T>` 的 sender）都在一条通道内成立。这就是它成为默认实现的原因。

HTTP 的正确位置是「读模型扩展点」：请求/响应型的取数场景（POST 事件 → JSON 回包 → 转 Brick 入 store）不需要持久连接，此时走 HTTP 反而更简单。`Transport` trait 的字节流级别抽象（`on` 返回流）允许 SSE 实现挂进来，但该实现等到真实需求出现再做。

## 后果

- `transport-ws` 是两个 UI crate 的默认依赖；替换传输只需在装配点（`mount()` / `STATUS`）换 `Rc<dyn Transport>` 实现。
- SSE/HTTP 实现未来接入时，上行路径映射与下行 SSE 帧的拼接逻辑自担；trait 不变。
- wasm 单线程约束下，`Transport` 的方法不要求 `Send`（`emit` 返回 boxed future，`on` 返回 `LocalBoxStream`）。
