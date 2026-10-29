# ADR 0021: 外部集成——宿主自备 transport 与 boot，核心不自适配

状态：已接受
日期：2026-10-29
仓库：fluxen

## 背景

已经存在两份消费形态：

- **reflux**（Python 宿主）：生产端构建 SDK 节点与八个动作，静态壳加载**钉住的渲染器 bundle**，
  页面上行走 `window.fluxen.send`——`mount()` 发布的那只把手。
- **prism**（Rust 会场，在计划中）：连接面、身份、turn 都在它那里，前端要装进它自己的二进制，
  不打算用钉住的 bundle。

prism 侧最早写的草案要求 fluxen 加一层"prism 帧适配"（`{ev,args}` ↔ `Message`），让渲染器去认
连接面的帧形状。方向是反的：fluxen 是纯 UI，不可能为每个使用方单独适配；`transport-ws` 本来就
只是一个**接口示例**（其注释原文："框架绑定的部分留在各 UI crate"），`stage` 也只是服务端的参考
实现——它的 mirror 不看内容，只转发原始帧。宿主就应当是这个形状：说同样的帧，而不是要求渲染器
换一套协议。

拆分 `ui_leptos` 的动机是同一个：`mount()` 把 `#main` 解析、`transport-ws` 与页面级上行把手写死
在渲染核心旁边，于是"核心 + 自己的 boot"与"boot + 自己的帧"两种方式里只有后者能成立，`fluxen`
门面也就没有内容可包。

## 决策

1. **适配属于宿主。** fluxen 核心不认任何宿主的帧形状。宿主提供 `transport::Transport` 的一个实现
   和自己的 boot；核心只提供 `Ctx::new(transport, codec)` 这一处接缝。
2. **线上格式就是 `content`。** 下行是**动作列表**（Accrete 的八个动作，按序应用），上行是
   `Outflow{event, id?, data}`。宿主的自有动词（`connected` / `login` / `error` / `turn.*`）不在
   这个契约里：它们住在宿主自己的信封内，渲染器不认——`ev` 通道（ADR 0003）随信封一起归宿主，
   见决策 6。
3. **boot 从核心拆出。** `ui_leptos` → `ui-core`（`ctx` / `render` / `components` / `widgets`，
   无 `#main`、无 `transport-ws`）+ `boot`（`#main` 解析、WS 接线、`mount()`、
   `window.fluxen.send`）。`fluxen` 门面总是重导出 `ui-core`，`boot` 走 feature。
4. **布局是数据，且是第一个视图操作。** 宿主以普通 `create` 帧下发，没有"布局信封"；客户端不需要
   为布局留一条与其它视图操作不同的路径。
5. **生产端的输出格式由调用参数决定。** 例：gravity 的 turn 调用带 `protocol` 参数，默认 `data`
   （裸 delta，给非 UI 消费者），`fluxen` 则返回视图帧（对槽做 `append`/`set`）。应用知识留在
   生产端，槽名不进 bundle。
6. **信封归宿主（2026-10-29，已落地）。** `content::Message{ev, sender, created?, content}` 是宿主
   包装，不是线上模型，已从核心里删除：`content` 只剩动作（`Content` 与各 op）与 `Outflow`，`Ctx` 解
   `Vec<Content<Accrete>>`，`ev` 闸随之消失（ADR 0003 的这一部分因此失效）。信封按宿主自己的需要存在，
   而不是每个宿主都该有一份：prism 有自己的动词（`connected` / `init.error` / `echo.result`），它包
   自己的帧；**stage 与 reflux 只有一个动词（画）——`ev` 只会复述它，`sender` 没有读者，`created`
   从来没人写，所以它们没有信封**，直接发那份动作列表。落地前线上是 `Message`——旧宿主会看到一条明确
   的 `frame is an envelope (map), not an action list` 而不是静默空白。

## 归属：crates/ 只放别人按名字依赖的东西

判据只有一条：**外部消费者会不会按名字依赖它。** 会 → `crates/`；我们自己拿来跑、没人依赖 → `examples/`；
调试工具另计（它是给人用的正式工具，不是一个示例）。

| 位置 | crate | 理由 |
|:--|:--|:--|
| `crates/` | `accrete`、`accrete_macro`、`content`、`transport`（trait） | 词汇、线上模型、接缝 |
| `crates/` | 渲染核心（`ui_leptos` 拆出的 `ui-core`）、`ui_leptos_macro` | 宿主复用的就是它 |
| `crates/` | `forcegraph`、`ui-assets` | 组件与资源登记，核心依赖 |
| `crates/` | `fluxen`（门面） | 消费者按这一个名字依赖 |
| `crates/` | `stage` | **例外**：调试工具（mirror/console/静态服务），是给人用的工具，不是示例 |
| `examples/transport-ws` | `transport-ws` | 92 行，唯一的消费者是 boot；WS 只是一个宿主的倾向，不是协议 |
| `examples/renderer` | boot（`mount()`、`#main` 解析、`window.fluxen.send`、`main.rs`、`index.html`、trunk 配置、assets/modules） | 发布产物的那半；reflux 用产物、prism 自写 boot，没有人按名字依赖它 |

包名不随目录改：`examples/renderer` 的包名仍是 `ui_leptos`。产物文件名是 `ui_leptos-<hash>.js`
与 `ui_leptos-<hash>_bg.wasm`，reflux 在 `www/static/vendor/fluxen/` 里钉住的就是这两个名字，
改名是一次跨仓 re-pin。名字与位置不一致是有意的，代价写在 PLAN 的落地记录里。

`examples/modules/*` 是另一回事（独立的 wasm 模块 workspace，编出 blob 给 canvas 宿主用）；本条的
`examples/` 指**示例宿主**。搬运时保持工作区语义：`cargo test --workspace` 不该因此开始依赖 wasm-only 的
东西，若真发生就把示例放在嵌套工作区里（`examples/modules/3dbrowser` 已是这个先例）。

搬完之后 `crates/` 的承诺变成一句话：**这里没有 boot，也没有任何具体传输实现**——两者都是宿主的，而我们的
宿主放在 `examples/`，和别人的宿主处在同一个位置。

## 后果

- **fluxen 一行代码不为 prism 改。** prism 侧新增一个很小的前端 crate：一个 `Transport` 实现
  （它的 `/ws` 腿）、一个 boot（自己的 `index.html` 与 wasm 入口）、以及对 `ui-core` 的挂载。
- **reflux 跟随 `boot` crate。** `reflux.js` 依赖 `window.fluxen.send`；该把手若改名，改在 `boot`，
  reflux 跟着 crate 走。
- **产物按挂载点构建。** public URL 是构建参数：reflux 服在页面根，prism 服在 `/assets/`，同一个
  核心可能有两份构建。宿主不改写产物的链接——否则网关成了第二个构建系统。
- **集成说明见 `docs/external-integration.md`（en）与 `docs/external-integration.zh-CN.md`（zh）**，
  以 prism 与 reflux 为两个实例。
- **信封也是宿主的。** 移完之后 `content` crate 只剩两样：动作（`Content` 与各 op）与 `Outflow`。
  stage、prism、reflux 各有各的信封，彼此不需要兼容——它们只在动作列表这一层对齐。
- 本文取代同日先前那份"客户端适配 prism 帧"的草案（未提交，已删除）。