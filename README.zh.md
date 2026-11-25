# Fluxen（中文）

> 本文是 [README.md](README.md) 的中文镜像；表述冲突时以英文版为准。

AI 原生 UI 渲染库：Accrete DSL + Leptos 渲染 + 操作协议（create/set/append/move/remove/patch）+ CBOR 编码。自 Fluxora 拆出（事件总线/Gateway 半区成为 [Prism](../prism/)；Fluxora 本体迁向 Leptos 并保留原名）。

设计文档：[Fluxora 架构](https://github.com/orbsh/wiki/blob/main/projects/fluxora-architecture.md)（wiki —— Accrete DSL、操作协议、codec 决策均在彼处）。

## 迁入内容

- **Accrete DSL**（`accrete` / `accrete_macro`）：闭合类型 enum，`#[serde(tag = "type")]`，Bind 系统，面向 AI 生成的 JsonSchema 校验
- **操作协议**（ADR 0005，取代原流式合并）：create/set/append/move/remove/patch 动作；`move` 把已在列表里的行挪位、行本身不重建；`patch` 以 JSON Pointer 寻址任意节点的线上形状；`tmpl` 注册 stamp 定义——一棵节点树，`"$name"` 整值在实例化时填进去（ADR 0037）
- **编码**（`codec`）：`ActiveCodec` 枚举分发（Json/CBOR），URL 参数握手定版，`encode_ws()` 辅助函数

## 定位

只做渲染层——没有事件总线，没有 gateway，没有传输。上游（Prism / Aura realm / 任何生产者）发送 Accrete 操作，Fluxen 负责渲染与操作应用。厚壳原则不变：AI 生成经 schema 校验的结构化 JSON（内容 + 结构），框架掌握样式与渲染确定性。

## 在页面里挂载

渲染器是库，不是应用：页面点名一个元素，把设置当数据传进来（ADR 0045）。没有属性，所以"我这两份配置谁赢"这个问题不存在；**地址也不是它的输入**（ADR 0046）：`?codec=` 由页面自己的四行脚本读出来交给它，token 是宿主塞进 `query` 的一个普通参数。渲染器只负责拼接与转义。

```html
<div id="app"></div>
<script type="module">
  import init, { mount } from '/vendor/fluxen/ui_leptos-<hash>.js';
  await init({ module_or_path: '/vendor/fluxen/ui_leptos-<hash>_bg.wasm' });
  await mount(document.querySelector('#app'), { codec: 'json', path: '/ws' });
</script>
```

每个字段都可省：

- `host` —— socket 去哪；不写即文档自身的 host。
- `path` —— socket 路径（缺省 `/channel`），必须以 `/` 开头。
- `codec` —— 这个页面**发出去**的编码：`cbor`（缺省）或 `json`。收到的帧按首字节逐帧识别，所以网关可以自由混用。
- `hooks` —— 四个口子：连接开/关（`open`/`close`）、整页重铺（`create`）、槽观察（`slots`）、上行改写（`dests`）。全部语义（何时触发、补发、合并、异常隔离）见 ADR 0047。
- `query` —— 额外拼进 socket 地址的参数（`Record<string, string>`），保序、逐项转义；键名的含义是宿主的，不是渲染器的。**token 就在这里**：它是一个普通查询参数，不再是一个特殊设置。
- `config` —— 本页选项所在的槽（缺省 `__CONFIG__`）；空串表示本页没有选项槽（ADR 0038）。
- `policy` —— socket 自己的那几个数：`{heartbeat, dead, retry: {base, max, jitter}}`（ADR 0044 §2）。`heartbeat: 0` 同时关掉探针和"静默即自断"；`retry: false` 表示只连一次，之后是终态 `closed`，不假装在重连。
- `transport` —— 页面自带 socket 时传入的活钩子 `{emit(bytes), subscribe(push)}`（ADR 0040）。与 `window.fluxenTransport` 同形，但按值传：活对象不是数据。

`<div id="fluxen">` 的意思是"用缺省挂我"，有它在时，加载模块就是全部集成——`init` 会走这条路。它是我们的 id，不是页面的地标：宿主的 `#main` 什么都不挂，所以"这页有渲染器"和"这页配置了渲染器"不会重叠，也就不需要撤销开关来仲裁。

值给了但不可用是拒绝，不是降级：未知键、既不是 `json` 也不是 `cbor` 的 codec、不以 `/` 开头的 `path`、死窗落在探针窗里的 policy。`mount` 抛出原因；在 `#fluxen` 那条路上，同一句话还会写进元素——空白页面加一行 devtools 日志，正是把配置错误读成渲染 bug 的方式。

上行走 `window.fluxen.send(event, data)`，或者由宿主自己的 runtime 驱动：reflux 用它的客户端 runtime 驱动渲染器，从不碰这个句柄。

## 数据绑定

两个方向各一种类型：读是 `Source` —— `{slot?, scope?, path?, default?}`，写是 `Sink` —— `{dest?, scope?, path?}`
（ADR 0031 拆开的是通道，ADR 0039 拆开的是类型）。两者都描述一个值和一个作用域；不同的是每个方向各自需要说什么。

```yaml
type: text
source: {slot: "notice", path: "/title", default: "untitled"}
```

- `slot` —— 值住在哪个名字下。不写 = 没有槽：只剩 `default` 就是一个常量。
- `dest` —— 值送到哪里去。和上行帧用的是同一个词（Outflow `{dest, id?, data}`，ADR 0023），所以 sink 和帧对同一个地址说的是同一句话。
- `scope` —— 谁权威：`remote`（缺省；由 op 写入、重放）、`local`（只在这个页面，随树消亡）、`both`（先落本地，再镜像上行）。
- `path` —— 指向值内部的 JSON Pointer（ADR 0008）。读侧：显示哪个成员。写侧：这个节点拥有哪个成员，它只写这一个、
  其余不动，只有 `scope: local` 的写会用到。`"/"` 是根：写在那里的对象按成员合并进已有内容，所以一个控件可以只
  设置自己的选项，不接管整个槽。
- `default` —— 只在读侧：没有值时显示什么。写用不到它，sink 也不接受它。

`source` 和 `sink` 是两条通道，不是同一个类型用两次。哪种节点能有它，是设计的一部分 —— `text`、`tiles`
是读；**字段不写**：`input` 只有 `source` + `kind`（哪种字段：`number`、`bool`、`text`、`password`，
元素类型和解析同一处声明），`select` 只有 `source` + `options`，两者都由 `tag` 命名。发出去的是外面那层 **loom**：它把带 tag 的字段打成一个对象，
拥有出口、时机（`submit: [enter, button, change]`）、次数（`sends`）、善后（`reset`），以及上行携带的 `id`
（ADR 0032）。那个 `id` 是请求的身份 —— 下发表单的服务端用它匹配答复 —— 而 `tag` 是对方在 `data` 里读到的名字：

```yaml
type: loom
id: "login-42"                       # 这次请求：回来的答复按它匹配
sink: {dest: "session"}              # 打好的对象送到哪
attrs: {submit: [enter, button]}     # 什么时候送
children:
- {type: input, tag: user, kind: text}
- {type: input, tag: pass, kind: password}
```

loom 可以嵌在 loom 里：内层按自己的时机打包（比如自动补全用 `change`），外层在自己的时机把全部字段打包 ——
一个字段，几种发送方式（docs/interaction-patterns.zh-CN.md §4）。

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

console REPL 把每一帧回显成 YAML——单独一行 `<-`，其后是缩进正文，JSON 与 CBOR 上行皆然——包括 UI 上报的事件：既是发送端也是事件监视器。命令：`/send <file.yaml>`、`/raw <json>`（发送裸 `Message<Accrete>`）、`/quit`。

推送任意帧批次（YAML 文件 = 一个 Content 项或其数组；action 头被保留：create/set/append/move/remove/patch/tmpl，见 docs/decisions/0005-operation-layer-value-patch.md。KDL 载体已于 2026-10-02 退役，只维护 YAML/JSON）：

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

## 发布渲染器

渲染器以 release 资产的形式发布，宿主因此既不需要这个仓，也不需要 `trunk`：

```
python3 scripts/ui_dist.py            # 构建，然后装配到 dist-release/
python3 scripts/ui_dist.py --no-build # 装配已有的 crates/ui_leptos/dist
```

打 `v*` tag 时 CI 跑同一个脚本（`.github/workflows/ui-release.yml`），发布：

- `manifest.json` —— `entry.js`／`entry.wasm`、`page`（入口文档），以及每个文件的 sha256 与大小
- 它点名的五件（`ui_leptos-<hash>.js`、`…_bg.wasm`、`assets/core.css`、`assets/custom.css`、
  `index.html`）
- `SHA256SUMS`，以及 `fluxen-ui-<tag>.tar.gz`（同样五件 + manifest）

按 tag 取用（例如 `https://github.com/orbsh/fluxen/releases/download/v0.3.3/manifest.json`），把五件
拷进你的静态根，`/` 就是一个能用的页面：入口文档把 fluxen 挂到 `<div id="fluxen">`，用它的默认值
（`path: /channel`，`codec` 取地址上的 `?codec=`、缺省 `cbor`），宿主什么都不用配。

`index.html` 是**默认值**，不是唯一入口。boot 属于宿主（ADR 0045）：要自己配置的宿主用自己那页替换它
（reflux 那页是 `#main` 加一句 `mount(el, {codec, path})`）。保留这份自带的页面时有两件事要知道：它的
引用是**绝对路径**（`/ui_leptos-…`），所以只在「bundle 就是 origin」时成立（prism、reflux 都是），挂在
子路径下就不对了；它的 `<title>` 是 fluxen 自己的。无论用哪页，传给 `init({ module_or_path })` 的必须是
**带哈希的** wasm 路径 —— glue 的默认 `ui_leptos_bg.wasm` 在 trunk 的哈希名下并不存在（PLAN §24）。

不含：`assets/3dbrowser`、`assets/splatviewer`、`assets/monaco`、`assets/mermaid.min.js`、
`assets/g2chart`。这些是模块媒体，只有帧点名时才加载，由部署侧自行组装（见 Dockerfile）。

### 宿主接管那五个口子

```js
await mount(document.querySelector('#app'), {
  codec: 'json',
  query: { token },                       // 会话就是查询参数，不是特殊设置（ADR 0046）
  hooks: {
    // 每次 socket 打开（含重连）。句柄就是 window.fluxen.send，也是页面发第一条的地方。
    open: (send) => send('identify', { token }),
    close: () => {},
    // 整页被替换（create）：手上的 DOM 引用过期了。
    create: (root) => {},
    // 槽的值：一次渲染 pass 里同名只报最新一次；订阅时补发当前值。
    slots: { 'ui:token': (token) => localStorage.setItem('token', token) },
    // 上行：编码前同步改载荷；返回 null 表示不改。凭证不需要在这里——它是连接级事实。
    dests: { '*': (dest, data) => ({ ...data, trace: myTrace() }) },
  },
});
```

全部语义见 [ADR 0047](docs/decisions/0047-one-table-four-seams.md)。
