# 外部集成

写给任何把本渲染器送到浏览器的一方：Rust 网关、Python 宿主、一个内嵌页面。它说明复用哪些、
必须自备哪些、线上说什么，以及两个实例——prism 会场（Rust，重新包装核心）与 reflux
（Python，钉住发布产物）。

全文只讲一条规则：**渲染器不为任何宿主适配。** 适配的是宿主——它提供 transport 与 boot，
并说渲染器的线上格式。

## 1. 复用与自备

复用（核心，不做 I/O，不假设浏览器）：

| crate | 内容 |
|:--|:--|
| `accrete`（+ `accrete_macro`） | 节点词汇与八个动作 |
| `content`（+ `codec`） | `Message` / `Outflow` / 动作结构体；JSON 与 CBOR 编解码 |
| `ui-core`（今天的 `ui_leptos` 去掉 boot） | `Ctx`（layout/data/list/vals/drag 信号）、`render::dispatch`、组件、widgets |
| `forcegraph` | 三维图核心（相机、布局、拾取）——不认框架、不认 DSL、不认协议 |
| `ui-assets` | CSS 登记处（`manganis` 要求资源定义在 crate 内） |

刻意不在表里的：`transport-ws`（`examples/transport-ws`）与 boot（`examples/renderer`）。两者都在 `examples/` 下——它们是**我们自己的宿主**，不是消费者
依赖的面。判据一句话：`crates/` 只放外部按名字依赖的东西；我们自己的宿主实现住 `examples/`；`stage` 留在
crate 里，因为它是调试**工具**而不是示例。核心不依赖这两者中的任何一个。

自备：

1. **transport**——`transport::Transport` 的一个实现：出字节，入原始帧流。它不编码、不解帧，
   该 crate 的注释原话如此。
2. **boot**——页面从哪来、socket 在哪、页面是否得到一个 JS 上行把手。
3. **帧**——应用的视图操作；若应用有静态骨架，还有一份布局文档。

接缝只有一处：`Ctx::new(transport: Rc<dyn Transport>, codec: ActiveCodec)`。它之上不知道字节
从哪来，它之下不知道 Accrete。

## 2. 线上格式

下行（宿主 → 页面）：`content::Message { ev, sender, created?, content }`。

- `ev == "draw"` 才渲染；**其余 `ev` 值渲染器忽略**（记 debug 日志）。这就是操作通道
  （ADR 0003），也是宿主自己的动词住的地方——`connected`、`login.result`、`error`、`turn.delta`。
  一个 socket，一种帧形状。
- `content` 是八个动作的列表：`create` / `set` / `append` / `remove` / `patch` / `inject` /
  `tmpl` / `empty`。

上行（页面 → 宿主）：`content::Outflow { event, id?, data }`。

编解码：默认 CBOR；读侧按帧首字节逐帧自适应，`?codec=` 只钉客户端**发**什么。宿主可以自由混用。

纪律：帧的形状在生产端检查，不在渲染器里碰运气。字段不对的动作是解码失败加一条日志，绝不是
画出一半。reflux 仓的 `reflux/wire.py` 是这道闸的完整例子——词汇封闭性加取值域检查，都发生在
帧被构建的地方。

## 3. boot，以及它为什么该独立成 crate

今天 `ui_leptos::mount()` 是唯一的 boot：读 `#main` 的 `data-host` / `data-path` / `data-codec` /
`data-token`，开 `ws://<host><path>`，挂响应式根，并把 `window.fluxen.send` 发布成页面级上行把手。
这对**钉住的发布产物**是好 boot，对"在自己二进制里重新包装核心"的宿主是不合身的。

决定（2026-10-29）：把 `ui_leptos` 拆成

- **`ui-core`**——`ctx` / `render` / `components` / `widgets`。没有 `#main`，没有 `transport-ws`。
- **`boot`**——`#main` 解析、WS 接线、`mount()`、`window.fluxen.send`。

这样 `fluxen` 门面才有内容：它总是重导出 `ui-core`，`boot` 走一个 feature。两种消费方式从此有名字，
不靠口口相传：

| 方式 | 谁 | 复用 | 自备 |
|:--|:--|:--|:--|
| 核心 + 自己的 boot | prism（Rust，进程内） | `ui-core`、`accrete`、`content`、`forcegraph`、`ui-assets` | 自己的 transport、自己的页面入口 |
| boot + 自己的帧 | reflux（Python，钉住产物） | 由示例里的 boot 构建出的 bundle | 帧本身，走自己的 socket |

## 4. 实例一：prism，会场

prism 是 WS 连接面（网关、身份、turn）。它的 UI 腿是 `/ws`，而这条腿说**本文件这套**线上格式，
不是 prism 自己的 `{ev, args}` 动词帧：

- **布局。** 客户端第一个上行是 `Outflow { event: "init" }`。prism 用普通的 `create` 帧答布局：
  `file:` 来源把 YAML 帧列表原样送出，`event:` 来源提交给一个全局摊位并把返回值转发。没有布局
  信封，也没有第二种形状——布局就是第一个视图操作。
- **prism 自己的动词**走 `Message { ev: "<verb>", content: [] }`；渲染器按设计忽略它们，在意的
  客户端（比如登录表单）读它们。同一个 socket 上没有第二种帧形状。
- **流式。** 格式由生产端决定。gravity 的 turn 调用带一个 `protocol` 参数：默认 `data`（裸
  delta，给非 UI 消费者），`fluxen`（视图帧——对布局声明的槽做 `append`/`set`）。应用知识留在
  摊位，渲染器保持通用，槽名不编进 bundle。
- **认证。** 连接即有 device 身份，`login` 只是普通上行，没有登录墙：认证是逐 event 的业务
  决策，纯阅读的布局一个都不声明。
- **prism 自备：** transport（它的 `/ws` 腿）、boot（自己的 `index.html` 加一个很小的 wasm 入口，
  用该 transport 挂 `ui-core`）、帧（布局 + 摊位输出）。
- **prism 不许做：** 教会 `ui-core` 认 `{ev,args}`，或者自带第二个渲染器。

## 5. 实例二：reflux，Python 宿主

reflux 是"钉住产物"这条路的参考宿主：Python 生产端用 SDK 节点与八个动作构建帧
（`reflux/wire.py`），静态壳（`www/`）加载钉住的渲染器 bundle，页面上行走 `window.fluxen.send`
——`mount()` 发布的那个把手。它的线上格式已经是 `Message { ev: "draw", sender, content }`，
与本文件逐字段一致。

在拆分后它消费 **`boot`**（它的 bundle 就是一次 boot 构建），不直接碰核心。若将来需要进程内渲染，
它再迁到"核心 + 自己的 boot"，与 prism 同一条路。

按现在的标准，它欠两处调整：

- **public URL。** 发布产物在构建时声明挂载点。reflux 把 bundle 服在页面根，所以它钉住的那次
  构建保持根相对 public URL；prism 服在 `/assets/`，就按 `/assets/` 构建。同一个核心的两份构建
  ——宿主从不改写产物的链接。
- **上行把手属于 `boot`。** `reflux.js` 依赖 `window.fluxen.send`；这个名字或契约若要改，改在
  `boot`，reflux 跟着 crate 走，不是反过来。

## 6. 本文不是

- 不是第二种协议。渲染器的词汇**就是**线上词汇；宿主需要更多东西就发更多 Accrete，不发新帧型。
- 不是对生产端语言的限定。Python、Rust，或任何能写 JSON/CBOR 的东西。
- 不是允许你教会核心认你的 transport、你的认证、你的数据库。那些以帧、动词、文档的形式到达。

参见：`docs/decisions/0021-external-integration.md`（本文件解释的那条决定）、`docs/PLAN.md`
（`ui-core`/`boot` 拆分作为排期工作）。