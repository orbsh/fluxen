# Fluxen（中文）

> 本文是 [README.md](README.md) 的中文镜像；表述冲突时以英文版为准。

AI 原生 UI 渲染库：Brick DSL + Leptos 渲染 + 流式合并 + CBOR 编码。自 Fluxora 拆出（事件总线/Gateway 半区成为 [Prism](../prism/)；Fluxora 本体迁向 Leptos 并保留原名）。

设计文档：[Fluxora 架构](../../.hermes/wiki/projects/fluxora-architecture.md)（wiki —— Brick DSL、merge 策略、codec 决策均在彼处）。

## 迁入内容

- **Brick DSL**（`brick` / `brick_macro`）：闭合类型 enum，`#[serde(tag = "type")]`，Bind 系统，面向 AI 生成的 JsonSchema 校验
- **流式合并**（`merge`）：Replace / Concat / Delete 三种策略，`Vec<String>` 片段缓冲
- **编码**（`codec`）：`ActiveCodec` 枚举分发（Json/CBOR），URL 参数握手定版，`encode_ws()` 辅助函数

## 定位

只做渲染层——没有事件总线，没有 gateway，没有传输。上游（Prism / Aura realm / 任何生产者）发送 Brick 操作，Fluxen 负责渲染与合并。厚壳原则不变：AI 生成经 schema 校验的结构化 JSON（内容 + 结构），框架掌握样式与渲染确定性。

## 开发工作流（`stage`）

`stage` 是开发网关：WS 镜像 + console REPL，不接任何上游生产者即可驱动 UI。它从不解析载荷——只在 peer 之间路由原始帧。

启动后的端点（默认端口 3002）：

- `GET /channel` —— UI 渲染端连接此处（Fluxen 的 `WsTransport` 路径）
- `GET /cli` —— 程序化 peer（console、curl-ws、你自己的工具）
- `POST /send` —— 请求体为 KDL；解析成 `Content::Create` 帧后广播

终端 1 —— 启动网关（mirror + 交互 console 单进程）：

```
cargo run -p stage -- serve            # 监听 :3002
```

console REPL 会把每一帧回显出来（`<- {...}`），包括 UI 上报的事件——既是发送端也是事件监视器。命令：`/send <file.kdl>`、`/raw <json>`（发送裸 `Message<Brick>`——发 Set/Join 帧必须走这条）、`/quit`。

终端 2 —— 构建并托管 UI（默认值已对齐 :3002：`index.html` 携带 `data-host="localhost:3002"`）：

```
cargo install trunk       # 一次性
cd crates/ui_leptos && trunk serve    # 监听 :8281
```

打开 <http://localhost:8281/> —— 页面经 `/channel` 连上镜像。查询参数：`?token=***（认证透传）、`?codec=json`（默认 CBOR；调试期用 `json` 可直接在 devtools 里读帧）。

终端 3（或 console）—— 推送内容：

```
curl -X POST --data-binary @examples/kdl/chat_layout.kdl http://localhost:3002/send
curl -X POST --data-binary @examples/kdl/login_set.kdl  http://localhost:3002/send
```

`/send` 一律包装成 Create（替换整个根布局）。要在不重建布局的前提下流式推 Set/Join 帧，经 console 的 `/raw` 发裸消息，例如一次聊天 token 追加：

```
/raw {"sender":"demo","content":[{"action":"join","event":"chat","method":"concat","data":{"type":"text","id":"m1","bind":{"value":{"kind":"default","default":"hello "}}}}]}
```

流入 rack 的行应携带 `id`——合并与 DOM 身份都以它为键（见 docs/PLAN.md 约定）。

离线助手（不需要起服务）：

```
cargo run -p stage -- tojson examples/kdl/chat_layout.kdl   # KDL -> Brick JSON
```

流式语义、行身份行为、codec 细节：见上文 wiki 链接与 `docs/decisions/` 下的 ADR。
