# Fluxen（中文）

> 本文是 [README.md](README.md) 的中文镜像；表述冲突时以英文版为准。

AI 原生 UI 渲染库：Accrete DSL + Leptos 渲染 + 操作协议（create/set/append/patch/remove）+ CBOR 编码。自 Fluxora 拆出（事件总线/Gateway 半区成为 [Prism](../prism/)；Fluxora 本体迁向 Leptos 并保留原名）。

设计文档：[Fluxora 架构](../../.hermes/wiki/projects/fluxora-architecture.md)（wiki —— Accrete DSL、操作协议、codec 决策均在彼处）。

## 迁入内容

- **Accrete DSL**（`accrete` / `accrete_macro`）：闭合类型 enum，`#[serde(tag = "type")]`，Bind 系统，面向 AI 生成的 JsonSchema 校验
- **操作协议**（ADR 0005，取代原流式合并）：create/set/append/patch/remove 动作；`patch` 以 JSON Pointer 寻址任意节点的线上形状；`tmpl` 注册槽位替换模板（ADR 0006）
- **编码**（`codec`）：`ActiveCodec` 枚举分发（Json/CBOR），URL 参数握手定版，`encode_ws()` 辅助函数

## 定位

只做渲染层——没有事件总线，没有 gateway，没有传输。上游（Prism / Aura realm / 任何生产者）发送 Accrete 操作，Fluxen 负责渲染与操作应用。厚壳原则不变：AI 生成经 schema 校验的结构化 JSON（内容 + 结构），框架掌握样式与渲染确定性。

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

console REPL 会把每一帧回显出来（`<- {...}`），包括 UI 上报的事件——既是发送端也是事件监视器。命令：`/send <file.yaml>`、`/raw <json>`（发送裸 `Message<Accrete>`）、`/quit`。

推送任意帧批次（YAML 文件 = 一个 Content 项或其数组；action 头被保留：create/set/append/patch/remove/tmpl，见 docs/decisions/0005-operation-layer-value-patch.md。KDL 载体已于 2026-10-02 退役，只维护 YAML/JSON）：

```
curl -X POST --data-binary @examples/yaml/00.main.yaml http://localhost:3002/send
```

`x.nu`（仓库根）是该载体的 nushell 封装（`send <file> [-p <patch>]` 按扩展名
分派，另有 border-flashing / message-concat / message-replace 演示循环），
`examples/push_demo.py` 经裸 `/cli` WebSocket 流式推 Set/Append/Patch 帧：

```
nu -c 'use x.nu *; send 02.concat.yaml'
python3 examples/push_demo.py
```

帧必须携带渲染通道：顶层 `"ev": "draw"`（见 docs/decisions/0003-ev-channel.md），
非 draw 帧被 UI 忽略。

要在不重建布局的前提下流式推更新帧，经 console 的 `/raw` 发裸消息，例如向 `m1` 行追加一个聊天 token（JSON Pointer 指向行的线上形状）：

```
/raw {"ev":"draw","sender":"demo","content":[{"action":"patch","event":"chat","id":"m1","path":"/bind/value/default","op":"append","value":"hello "}]}
```

流入 rack 的行应携带 `id`——补丁寻址与 DOM keyed 身份都以它为键（见 docs/PLAN.md 约定）。

Bind 路由（ADR 0008）：`kind: event` 走动作协议上行；`kind: local { slot }`
把载荷留在浏览器内的值平面（`ctx.vals`），任意读方组件订阅同槽（可选
`path` 提取——`kind: source` 也获得了同一 `path`，指向节点的线上形状）。
`examples/kdl/14.local_routing.kdl` / `examples/yaml/14.local_routing.yaml`
演示菜单→标题的频道模式，零 transport 往返。

UI 开发要热重建时，在 `stage serve` **之前**起 `trunk serve`（:8281）——网关会代理到它而不是读盘。trunk 是可选开发工具，不是运行时依赖。

离线助手（不需要起服务）：

```
cargo run -p stage -- tojson examples/kdl/chat_layout.kdl   # KDL -> Accrete JSON
cargo run -p stage -- schema                                # Accrete 线上形状 JSON Schema
```

流式语义、行身份行为、codec 细节：见上文 wiki 链接与 `docs/decisions/` 下的 ADR。
