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

`stage` 是开发网关：WS 镜像 + console REPL + UI 服务器，一条命令跑通整个闭环，不需要任何上游生产者。镜像从不解析载荷——只在 peer 之间路由原始帧。

启动后的端点（默认端口 3002）：

- `GET /channel` —— UI 渲染端连接此处（Fluxen 的 `WsTransport` 路径）
- `GET /cli` —— 程序化 peer（console、curl-ws、你自己的工具）
- `POST /send` —— 请求体为 KDL；解析成 `Content::Create` 帧后广播
- 其余路径 —— UI 本体：启动时若 trunk 端口（默认 8281）可达则反向代理过去，
  否则静态托管 trunk 产物目录（默认 `crates/ui_leptos/dist`）。探测仅看 TCP
  且一次定终身（sticky）——先起 trunk 再起 stage；中途启动 trunk 需重启 stage。

启动后直接打开 <http://localhost:3002/>：

```
cargo run -p stage -- serve            # 镜像 + UI + console，单进程
cargo run -p stage -- serve --trunk 8281 --dist crates/ui_leptos/dist   # 显式指定
```

UI 的 WS 地址默认取页面 origin，无需配置。接收侧逐帧自适应识别编码（首字节
`{` = JSON，CBOR map 主类型 = CBOR），网关可混发两种格式；`?codec=json` 只
决定 UI 自身*发送*（用户事件）的格式——方便在 devtools 里读。发送默认为 CBOR。

console REPL 会把每一帧回显出来（`<- {...}`），包括 UI 上报的事件——既是发送端也是事件监视器。命令：`/send <file.kdl>`、`/raw <json>`（发送裸 `Message<Brick>`——发 Set/Join 帧必须走这条）、`/quit`。

从任意位置推送内容（KDL 一律包装成 Create——替换整个根布局）：

```
curl -X POST --data-binary @examples/kdl/chat_layout.kdl http://localhost:3002/send
```

YAML 文件承载完整 Content 表达力（action 可为 create/set/join/tmpl——不再被强制
包成 create）。文件内容 = 一个 Content 项或其数组；`?fmt=yaml` 显式选择解析器
（默认仍是 KDL）：

```
curl -X POST --data-binary @examples/yaml/02.concat.yaml "http://localhost:3002/send?fmt=yaml"
```

`examples/fluxen.nu` 是两种载体的 nushell 封装（`send <file> [-p <patch>]` 按扩展名
分派，另有 border-flashing / message-concat / message-replace 演示循环），
`examples/push_demo.py` 经裸 `/cli` WebSocket 流式推 Set/Join 帧：

```
nu -c 'use examples/fluxen.nu *; send 02.concat.yaml'
python3 examples/push_demo.py
```

帧必须携带渲染通道：顶层 `"ev": "draw"`（见 docs/decisions/0003-ev-channel.md），
非 draw 帧被 UI 忽略。

要在不重建布局的前提下流式推 Set/Join 帧，经 console 的 `/raw` 发裸消息，例如一次聊天 token 追加：

```
/raw {"ev":"draw","sender":"demo","content":[{"action":"join","event":"chat","method":"concat","data":{"type":"text","id":"m1","bind":{"value":{"kind":"default","default":"hello "}}}}]}
```

流入 rack 的行应携带 `id`——合并与 DOM 身份都以它为键（见 docs/PLAN.md 约定）。

UI 开发要热重建时，在 `stage serve` **之前**起 `trunk serve`（:8281）——网关会代理到它而不是读盘。trunk 是可选开发工具，不是运行时依赖。

离线助手（不需要起服务）：

```
cargo run -p stage -- tojson examples/kdl/chat_layout.kdl   # KDL -> Brick JSON
```

流式语义、行身份行为、codec 细节：见上文 wiki 链接与 `docs/decisions/` 下的 ADR。
