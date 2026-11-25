# 外部集成

写给任何把本渲染器送到浏览器的一方：Rust 网关、Python 宿主、一个内嵌页面。它说明复用哪些、
必须自备哪些、线上说什么、一帧怎么变成画面，以及两个实例——prism 会场（Rust，把已构建的产物挂成静态根）与
reflux（Python，钉住发布产物）。

全文只讲一条规则：**渲染器不为任何宿主适配。** 适配的是宿主——它提供 transport 与 boot，
并说渲染器的线上格式。

## 1. 复用与自备

复用（核心，不做 I/O，不假设浏览器）：

| crate | 内容 |
|:--|:--|
| `accrete`（+ `accrete_macro`） | 节点词汇与八个动作 |
| `content`（+ `codec`） | `Outflow` / 动作结构体；JSON 与 CBOR 编解码 |
| `ui-core`（今天的 `ui_leptos` 去掉 boot） | `Ctx`（layout/data/list/vals/drag 信号）、`render::dispatch`、组件、widgets |
| `forcegraph` | 三维图核心（相机、布局、拾取）——不认框架、不认 DSL、不认协议 |
| `ui-assets` | CSS 登记处（`manganis` 要求资源定义在 crate 内） |

刻意不在表里的：`transport-ws` 与 boot（`ui_leptos`）。它们也是 crate，但它们是**我们自己的宿主**——参考构建要发的支持性代码，不是消费者
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

下行，一帧**就是**动作列表——`create` / `set` / `append` / `move` / `remove` / `patch` / `tmpl` / `empty`
八个动作按序组成的数组。渲染器这一侧没有信封（ADR 0021 决策 6）：`content` 只剩动作与
`Outflow`；宿主只有在「帧不止一类」时才需要信封——prism 有（`connected`、`init.error`、
`echo.result`），`stage` 与 reflux 各只有一个动词，所以都没有。

所以一帧永远是数组——单条动作也是「一个元素的数组」——线上要么是 JSON 数组，要么是 CBOR 数组。其余
形状在入口就被拒，并且说清理由：旧的信封形状（JSON 对象或 CBOR map）解出来是
`frame is an envelope (map), not an action list`，于是还在发 `Message` 的宿主看得到原因，而不是一片
空白。

宿主动词（`connected`、`login.result`、`turn.delta`）走宿主自己的信封、走宿主自己的腿。渲染器从
来看不到它们，也不提供过滤——ADR 0003 的 `ev` 闸跟着信封一起走了。

上行（页面 → 宿主）：`content::Outflow { dest, id?, data }`——**目标**：这次交互调起宿主上的哪个
处理器。它刻意不叫 `slot`，两个方向不对称。`event` 在这条线上根本不出现（ADR 0023）。

编解码：默认 CBOR；读侧按帧首字节逐帧自适应，`?codec=` 只钉客户端**发**什么。宿主可以自由混用。

纪律：帧的形状在生产端检查，不在渲染器里碰运气。字段不对的动作是解码失败加一条日志，绝不是
画出一半。reflux 仓的 `reflux/wire.py` 是这道闸的完整例子——词汇封闭性加取值域检查，都发生在
帧被构建的地方。

## 3. 渲染流程——进来的是字节，出去的是画面

帧不是对 DOM 的补丁。它是对一个信号的写入，重画的也是那个信号。两个方向都穿过同一处接缝：以下
全部发生在 `ui-core` 加上你自备的 transport 之内。

### 3.1 下行：宿主 → 页面 → 屏幕

1. **字节。** `WsTransport::new(url, codec)` 打开 socket 并拆成写、读两半，一个读循环把每条入站
   消息推进无界通道——`Text` 取其 UTF-8 字节，`Bytes` 原样。`Transport::on()` 把这个通道暴露为
   `Stream<Vec<u8>>`。你的 transport 到此为止：不编码，不解帧。
2. **字节进队列。** `Ctx::new` 起一个循环，把每帧字节 push 进 `ctx.transport.frame`——那是待处理的
   **队列**（`RwSignal<Vec<Vec<u8>>>`），不是「最近一帧」信号。读循环一次可以连取多帧，而 Effect
   要等微任务边界才跑；用单值信号就是后到覆盖先到，症状是连上就推的布局丢掉、页面空白。次序在这里
   是语义，队列把它守住。socket 因此在渲染路径之外，调度归 Leptos。
3. **解码。** 该队列上一个 `Effect` 取走整批，逐帧调
   `ActiveCodec::decode_auto::<Vec<Content<Accrete>>>`：首字节定格式——`[` 是 JSON 数组，
   `0x80..=0x9F` 是 CBOR 数组——与页面为上行为钉住的编码无关，所以宿主可以在同一路数据里换格式。
   解不出的帧记日志后丢弃，绝不半应用。
4. **应用。** 动作列表按序应用，每个动作只写一个信号：

   | 动作 | 写入 | 细节 |
   |:--|:--|:--|
   | `tmpl` | stamp 定义表 | `{name, params, data}`：`data` 就是节点树，`"$name"` 这种整值在 `create`/`set`/`append` 时被替换（ADR 0037） |
   | `create` | `ctx.layout` | 根文档；页面的响应式根重画 |
   | `set` | 数据槽 `slot` | 一个具名节点 |
   | `append` | 列表槽 `slot` | 行身份取自 `data.id`；重复 id 拒收；`position` 从尾往内数（0 = 末尾） |
   | `move` | 列表槽 `slot` | 行 `id` 必须已在列表里；`position` 记法与 `append` 同向，但按"该行先拎出来"之后的列表数；域外拒收 |
   | `remove` | 列表槽 `slot` | 按行 id |
   | `patch` | 布局根、某一行、或数据槽 | 对节点的**线上形状**打 JSON Pointer（RFC 6901）；`slot: ""` 是根，带 `id` 是那一行，其余是具名槽 |

   **一个词一个意思。** 树里的地址叫 `slot`——`create` 用 `slot: ""` 指向根，其余都写槽名（或带 `id`
   指向某个列表槽里的一行）。上行不是槽（`Outflow.dest`，以及节点 `kind: dest` 绑定里的 `dest`）——
   两个方向并不对称，所以不共用一个词：下行往页面里的某个位置写，上行调起宿主上的一个处理器，形状也
   不同。`event` 留给 DOM 与后端自己的领域事件（槽见 ADR 0022，目标见 ADR 0023）。

   这些写入一律先 untracked 读、再 `set`：dispatch 运行在消费该帧的 Effect 里，tracked 读会让
   这个 Effect 订阅自己马上要写的槽。
5. **渲染。** 树是数据。`render::dispatch` 把节点映射到组件（`gen_dispatch!` 由 `accrete` 的枚举
   生成匹配），每个组件把自己的主体放进一个读取它所需槽的闭包里——`text_` 经 `use_source_value`，
   `tiles` 读它的列表槽。于是 `set` 或 `append` 只重画绑在该槽上的节点：没有 diff，没有虚拟 DOM
   ——信号图**就是**更新范围。空的 data 槽不渲染、不占空间（ADR 0012），所以流式段落可以先出现，
   它的框后长出来。

一个洞，以及谁来填。布局按名字声明槽，生产端就写那个名字：

```yaml
# 布局里（create）
- type: text
  source: {slot: intro}
```
```yaml
# 之后某一帧
action: set
slot: intro
data: { type: text, source: {default: "hello"} }
```
`set` 写这个槽，读 `intro` 的节点重新渲染。要让同一段继续长，用 `patch` 带 `slot: intro` 和指向
槽节点字段的指针——`patch` 是流式动词，`set` 是整值动词。列表则由布局给壳和行模板：

```yaml
- type: tiles
  source: {slot: chat}
  children:
  - type: frame
    attrs: { class: [as-stretch, box, is-border, round] }
```
此后每次 `append { slot: chat, data: { id: m42, ... } }` 加一行，按 `data.id` 认身份，所以追加
不会重建它上面的行。

### 3.2 上行：页面 → 宿主

1. **组件发射。** 节点的写口是 `sink`：`Sink { dest, scope, path }`（ADR 0031 §2）。有写口的节点经
   `use_target` 拿到一个闭包，它调 `ctx.emit(&sink, id, payload)`；没有 `sink` 的节点拿不到闭包，
   它只读。`id` 是发射节点自己的 id（没有则 `None`）；`payload` 是该组件产出的东西——表单值、
   拖拽货物、页码。`dest` 缺席的 sink 无处可去：`emit` 返回 false，什么也不发。`input` 的改动按它自己的
   时钟到达 loom（`emit`：缺省 250ms 静默，另有 `{throttle: ms}` 与 `{debounce: 0}`，ADR 0032 §8），所以
   `submit: [change]` 发几帧是生产端选的速率，不是击键次数。
2. **`Event` 上线。** `ctx.send` → `Transport::emit(transport::Event)` → `WsTransport::emit`：组装
   `content::Outflow { dest, id?, data }`，用页面构建时钉住的编码（JSON → text 帧，CBOR → 二进制
   帧）编码后发出。`send` 是 future：socket 尚在连接时 gloo 的 sink 挂起、open 时唤醒，所以抢在
   `open` 之前的发射不会丢。
3. **`scope: local` 不出页面。** 带 `path` 的本地写只改那个槽里值的一个成员（合并写，ADR 0031 §2）；
   不带 `path` 的把整个 `Outflow` 存进以 `dest` 命名的槽——本地 emit 与上行帧因此共一种形状
   （ADR 0010），整条链可以在没有 socket 的情况下演练。
4. **页面自己的把手。** boot 发布 `window.fluxen.send(name, data)`，它汇入同一个 `Ctx::send`。
   页面脚本与组件共用一条上行、一种帧形状。
5. **宿主收到的只有** `Outflow`：`id` 是发射者，`data` 自由，完全属于产出它的那个组件。渲染器
   不加自己的信封，也不加自己的元数据。

### 3.3 这套流程承诺什么，假定什么

- **次序。** 一帧之内动作按列表顺序应用；帧之间按到达顺序应用。
- **失败按动作计，且留痕。** 被拒的动作记日志、同帧其余动作照常应用；解不出的帧记日志、一个动作
  也不应用。形状错误由生产端负责（§2）——解出来但字段错的帧只能被跳过，不能被修补。
- **地址必须先存在。** 往空槽 `patch`、删不存在的行、用重复 id `append`，一律「跳过并告警」。
  生产端可以连着发 `create` 与 `patch`；但不能指望渲染器替它重放或改写任何东西。
- **transport 里没有状态。** 重连就重发布局；页面只是它挂载以来收到那些帧的投影。

### 页面自己的传输

`window.fluxenTransport = { emit(bytes), subscribe(push), close? }`——在 mount 之前装好，它就接管整条链路：boot
读一次、只搬字节（编解码仍在 Rust，仍由 `data-codec`／`?codec=` 决定），上行下行都经它；重连、退避与恢复正常是
宿主的责任（ADR 0040 §3）。没装则跑默认的 `LiveWs`：入向静默 5 秒发一次 `__ping__`（宿主回 `__pong__`）、
静默 10 秒判死并由页面自己关掉 socket、重连从 500ms 起翻倍到 10 秒封顶并加抖动（ADR 0044）。整条策略写在
`#main` 的一个属性上：`data-transport='{"heartbeat": 0}'` 关掉探针、`{"retry": false}` 关掉重连，两个数字与退避
表都可以写出来；不认识的键是 mount 报错，不是"设了但没用的旋钮"。装了却缺
`emit` 或 `subscribe` 是 mount 错误——绝不静默回退到自带的 socket，那会让页面的 JS 与 wasm 各持一条彼此看不见的
连接。

## 4. boot，以及它为什么该独立成 crate

今天 `ui_leptos::mount()` 是唯一的 boot：读 `#main` 的 `data-host` / `data-path` / `data-codec` /
`data-token`，开 `ws://<host><path>`，挂响应式根，并把 `window.fluxen.send` 发布成页面级上行把手。
这对**钉住的发布产物**是好 boot，对"在自己二进制里重新包装核心"的宿主是不合身的。

决定（2026-10-29）：把 `ui_leptos` 拆成

- **`ui-core`**——`ctx` / `render` / `components` / `widgets`。没有 `#main`，没有 `transport-ws`。
- **`boot`**——`#main` 解析、WS 接线、`mount()`、`window.fluxen.send`。

这样 `fluxen` 门面才有内容：它总是重导出 `ui-core`，`boot` 走一个 feature。三种消费方式从此有名字，
不靠口口相传：

| 方式 | 谁 | 复用 | 自备 |
|:--|:--|:--|:--|
| 核心 + 自己的 boot | 今天没有消费者（这条路留给要进程内包装核心的宿主） | `ui-core`、`accrete`、`content`、`forcegraph`、`ui-assets` | 自己的 transport、自己的页面入口 |
| boot + 自己的帧 | prism（Rust，挂产物）、reflux（Python，钉住产物） | 由示例里的 boot 构建出的 loom（`#main` 解析、WS、`window.fluxen.send`） | 帧本身，走自己的 socket；prism 另备静态根与 `/app.css` |
| 我们的 boot + 你的 transport | 需要 JS 传输的宿主（ADR 0040） | 示例里的 boot 构建出的 loom（`#main` 解析、`Ctx`、`mount_to`、`window.fluxen.send`） | 只有 transport：`window.fluxenTransport` 的 `emit`/`subscribe`/`close?`，字节进出；codec 仍归 Rust |

两条路的判据是「谁建 socket」，不是「谁写 Rust」：prism 用 Rust 写的是**生产端**（网关、身份、turn），
渲染器那一侧它和 reflux 一样只挂产物——`crates/prism` 唯一的 fluxen 依赖是 `content`（线上词汇），
没有 `ui-core`，也没有自己的 boot。

### 第三种走法：留着我们的 boot，只换 transport

需要 POST+SSE、与页面其它 JS 共用一条 socket、带鉴权头、或自己那套重连退避的宿主，不必写一个 Rust boot，也不必另编一个 `ui-core` 的 wasm（那条路的代价见 ADR 0040 §2：它并没有把 boot 移出 Rust，只多出一份 ABI、一份 codec 与第二个产物）。做法是一个全局钩子：

```
window.fluxenTransport = {
  emit(bytes)      { /* 一帧上行，已编码 */ },
  subscribe(push)  { /* 收下 sink；每条下行帧交给 push(bytes) */ },
  close?()         { /* 可选 */ },
}
```

五条规矩：**在 mount 之前读一次**（`Ctx::new` 构造时就吃掉 transport，之后不能换）；**只搬字节**——codec 仍归 Rust，仍来自 `#main[data-codec]` / `?codec=`；**挂了但形状不对 = mount 报错**，绝不静默回退到我们的 WS（那会让页面以为 JS 掌控传输、而 wasm 自己开了一条）；**重连归钩子**，框架从不重试；两种模式下 `window.fluxen.send` 都在，页面代码不知道用的是哪条传输。代价只有一条：钩子模式下每帧多一次 JS↔wasm 跨界，而默认路径一分钱不付。

## 5. 实例一：prism，会场

prism 是 WS 连接面（网关、身份、turn）。它的渲染器腿是 `/channel`，这条腿说**本文件这套**线上格式，
不是 prism 自己的 `{ev, args}` 动词帧（那些留在 `/ws`——CLI 与对端的程序腿；先例是 stage：
页面走 `/channel`，对端走 `/cli`）：

- **布局。** 连接一建立 prism 就往下推，不等任何上行：先是凭证那一帧，然后是外壳（`shell.yaml` 的 `create`：导航 + 每个 island 一个空区域），再是这一位看客的数据帧，最后在平面上通告 `ui:connect`——声明它的摊位用普通帧作答。没有「请求布局」这个动词，也没有布局信封：布局就是连接的回答。
- **prism 自己的动词**走**它自己的**信封，不是渲染器的。今天那个信封恰好长得像
  `{ev, args}`，渲染腿只运动作列表、别的什么都不解（决策 6 已落地）——prism 的动词因此整条留在它自己
  那一侧，关心它们的客户端（比如登录表单）在那边读
  ——所以动词整个留在 prism 一侧，渲染腿只运动作列表。在意的客户端（比如登录表单）自己读。
- **流式。** 格式由生产端决定。gravity 的 turn 调用带一个 `protocol` 参数：默认 `data`（裸
  delta，给非 UI 消费者），`fluxen`（视图帧——对布局声明的槽做 `append`/`set`）。应用知识留在
  摊位，渲染器保持通用，槽名不编进 loom。
- **认证。** 连接即有 device 身份，`login` 只是普通上行，没有登录墙：认证是逐 event 的业务
  决策，纯阅读的布局一个都不声明。
- **prism 自备：** transport（它的 `/channel` 腿）、帧（布局 + 应用的 view 事件），以及——只在它想要时
  才需要——自己的页面。客户端一侧什么都不用索取：**连上就推布局**，所以示例 loom 不改一行就能配 prism。
- **prism 不许做：** 教会 `ui-core` 认 `{ev,args}`，或者自带第二个渲染器。

## 6. 实例二：reflux，Python 宿主

reflux 是"钉住产物"这条路的参考宿主：Python 生产端用 SDK 节点与八个动作构建帧
（`reflux/wire.py`），静态壳（`www/`）加载钉住的渲染器 loom，页面上行走 `window.fluxen.send`
——`mount()` 发布的那个把手。它的线上格式就是动作列表本身（裸数组），与本文件的契约逐字段一致。

在拆分后它消费 **`boot`**（它的 loom 就是一次 boot 构建），不直接碰核心。若将来需要进程内渲染，
它再迁到"核心 + 自己的 boot"——那条路今天没有消费者，prism 也不在它上面。

按现在的标准，它欠两处调整：

- **public URL。** 发布产物在构建时声明挂载点。两个宿主都把它挂在页面根（bundle 就是 origin：
  `/` 出它的 `index.html`，产物里的 `/ui_leptos-…js`、`/assets/core.css` 按原样解析），所以一份
  根相对的构建两边通用——宿主从不改写产物的链接，也不为挂载点另建一份构建。
- **上行把手属于 `boot`。** `reflux.js` 依赖 `window.fluxen.send`；这个名字或契约若要改，改在
  `boot`，reflux 跟着 crate 走，不是反过来。

## 7. 本文不是

- 不是第二种协议。渲染器的词汇**就是**线上词汇；宿主需要更多东西就发更多 Accrete，不发新帧型。
- 不是对生产端语言的限定。Python、Rust，或任何能写 JSON/CBOR 的东西。
- 不是允许你教会核心认你的 transport、你的认证、你的数据库。那些以帧、动词、文档的形式到达。

参见：`docs/decisions/0021-external-integration.md`（本文件解释的那条决定）、`docs/PLAN.md`
（`ui-core`/`boot` 拆分作为排期工作）。