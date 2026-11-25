# PLAN

决策在 `docs/decisions/`（ADR 0003 … 0040）；本文件是索引——什么时候落了什么、什么还开着、宿主必须知道什么。

## 执行中的计划（2026-09 … 2026-10）

- [x] 0 — brick → accrete 更名 — ADR 0004 — 2026-09-26
- [x] 1 — `ev` 操作通道 — ADR 0003 — 2026-09-26
- [x] 2 — KDL 解析换到 kdl 6.7.1（真 KDL v2 语法）— 2026-09-26
- [x] 3 — YAML 载体并存，保留全表达力兜底 — 2026-09-26
- [x] 4 — 示例迁移：fluxora 的 `data/message/*.yaml` 进 `examples/yaml/` — 2026-09-26
- [x] 5 — nushell 发送工具 `examples/fluxen.nu` — 2026-09-26
- [x] 6 — 操作层：`Value` 与 `Patch` — ADR 0005 — 2026-09-30
- [x] 7 — Canvas 组件：外部渲染模块原位挂载 — ADR 0007 — 2026-10-01
- [x] 8 — 移除 minijinja，template 降为纯槽位替换 — ADR 0006 — 2026-10-01
- [x] 9 — bind 本地路由：Local 落点与值平面订阅 — ADR 0008 — 2026-10-01
- [x] 10 — Chart 渲染器：G2 spec 与模块通道 — ADR 0009 — 2026-10-01
- [x] 11 — Pages 组件与统一事件形状 — ADR 0010 — 2026-10-02
- [x] 12 — 远程 Vars 注入 — ADR 0011 — 2026-10-02；2026-11-24 撤除（`inject` 删除，ADR 0031 §2 由 ADR 0038 执行）
  - [ ] 页面状态快照/恢复 — 2026-10-02 暂缓（先不做）
- [x] 13 — 流式内容走订阅槽，并修好模块 update 通道 — 2026-10-02
- [x] 14 — 容器子节点级 Memo — 实测否证 2026-10-12：流式只走数据槽
- [x] 15 — 空载荷的模块宿主不占位 — ADR 0012 — 2026-10-10
- [x] 16 — Float overlay：背景模糊、不可操作 — ADR 0013，被 ADR 0014 取代 — 2026-10-10
- [x] 17 — 聊天框自动滚动：只在已经位于底部时跟随 — 2026-10-10
- [x] 18 — Canvas 模块的加载过渡 — 2026-10-10
- [x] 19 — custom CSS 层排在最前 — 2026-10-10 已修
- [x] 20 — layer + portal：模态层渲染到哪里 — ADR 0014 — 2026-10-12
- [x] 21 — 组件库按需加载：宿主 import 时机、模块求值、mermaid 预加载 — ADR 0015 — 2026-10-14
- [x] 22 — editor 组件：Monaco 走模块通道，保存走上行 — ADR 0016 — 2026-10-15
- [x] 23 — `append` 的落点参数 position — ADR 0005 — 2026-10-16
- [x] 24 — 页面级上行把手：入口读 `data-path` / `data-codec`，对外发 `window.fluxen.send` — 2026-10-19
- [x] 25 — 渲染器的发布产物：一份页面能直接引用的 UI bundle（tag `v*` → release 资产）— 2026-10-20
- [x] 26 — Canvas 的模块选项与 forcegraph 变体 — ADR 0017 — 2026-10-23
- [x] 27 — 删 `inject`：op / 消费臂 / 同值守卫 / 测试 / 示例帧 / wire 文档行 — ADR 0031 §2、0038 §1 — 2026-11-24
- [x] 28 — 页面选项就是槽：`#main[data-config]` 指定槽名（默认 `__CONFIG__`），选项对象是节点的载荷（`source.default`）；`sink` 带 `path` 写进 local 面并按成员合并，读侧按成员回落；`config` op / `Ctx.config` / `set_config` / 每个选项一个 signal 全部退役 — ADR 0038 §2 — 2026-11-24
- [x] 29 — 皮肤：`config.skin` 镜像到 `<html data-*>` + roughr 实时画边（frame / panel / loom；帧自己声明 `is-border` 才挂）— ADR 0038 §3-4 — 2026-11-24
- [x] 30 — 开关落在控件本身：`bool` 字段读自己写的槽、change 即出口（文本字段仍以 Enter 为出口），复选框读 `checked()` 而非 `value()`；示例把它放进图的右下角（`panel place: anchor` 浮层，卡片向上开，占图的空边角），shell 不加任何栏 — ADR 0030、0038 §2、§5 — 2026-11-24
- [x] 31 — 字段没有自己的上行：无 tag 的 sink 只剩本地落地（remote/both 报警告），清空改由 loom 的 `reset: keep|clear|default` 决定；`sends: n` 每发一次减一、为 0 锁定（字段 disabled、拒发），缺省/-1 无限 — ADR 0032 §2、§6、§7 — 2026-11-24
- [x] 32 — 字段的 `sink` 整个删掉（shape 也不再接受，`Input`/`Select` deny unknown keys）：字段只剩 `source` + `kind`（number/bool/text/password，缺省 text，元素类型与解析同一处声明）；本地落地归 loom（`sink: {scope: local, path: /}` 把打包对象并进槽根，`write_local_at` 的根路径按对象合并）；例子里的 13 处字段 sink 全部改成 loom — ADR 0032 §2 — 2026-11-24
- [x] 34 — 嵌套 loom 的打包键只认 `tag`：`id` 是身份（按它匹配答复）、`dest` 是它自己发送的去向，两者都不是名字，`loom_key` 不再从 `id` 后备；无 `tag` 的内层 loom 不进外层的包（它的字段留在自己包里，自己有 `dest` 就自己发）；顶层 `tag` 不读 — ADR 0032 §4 — 2026-11-24
- [x] 33 — 读和写分成两个类型：`Source {slot, scope, path, default}` / `Sink {dest, scope, path}`（`dest` 与上行帧同名，`emit` 不再翻译；sink 没有 `default`，两个类型各自 deny unknown keys，`sink: {slot: …}` 解码即错）；例子/文档 29 处 `sink: {slot: …}` 改 `dest`，两条死 `default` 一并删掉 — ADR 0039（0031 §2 加 amend）— 2026-11-24
- [ ] 32 — 皮肤的边角：`select` 的稳定子树、`button` 的类名、input 保持 CSS 边（void 元素装不下子节点）— ADR 0038 §4 — 待做
- [x] 35 — `move` 动作：把一个**已经在**列表里的行挪到另一个位置，行本身不重建 — ADR 0005 加一节 — 2026-11-24
- [x] 36 — 节点改名 `bundle` → `loom`：`bundle` 只说"打包"，对"收集输入"与"字段按 `tag` 命名"两层都沉默，读起来又像是 `frame`/`panel`/`tiles` 那一类"装东西的盒子"；`loom` 一次说出三件事——线=字段、织法=`tag`→键、布=送出的对象，且织机天然可嵌套（ADR 0032 §4）。`bus` 否（Prism 自己就是总线、共享通路不属于哪一组、总线不嵌套）、`trunk` 否（本仓构建工具、git 主干、方向朝根而载荷离树） — ADR 0032 §1 — 2026-11-25
- [x] 37 — 字段的时钟：`input.emit` = `{debounce: ms}` | `{throttle: ms}`，闭枚举互斥，缺省 `{debounce: 250}`；`{debounce: 0}` 回到"每键一帧"。被定时的只有"通知 loom"，字段自己的显示信号照旧每键同步写，所以打字不等窗口；`submit` 先取消挂起的窗口再打包（Enter／按钮永远送屏幕上的值），`reset` 也取消（不让旧值复活） — ADR 0032 §8（§3 加"两个 owner"段）— 2026-11-25
- [x] 38 — 传输钩子 + 活着的 socket：`window.fluxenTransport = {emit, subscribe, close?}` 在 mount 前读一次（形状不对=mount 报错，不回退）；默认路径换成 `LiveWs`（探针 `__ping__`→`__pong__`、5s 静默问一次、10s 静默判死自己关、重连 500ms 起翻倍到 10s 封顶、抖动 20%、离线帧排队 256 上限）；整条策略是一个 `#main` 的 `data-transport` 对象（`{"heartbeat":5000,"dead":10000,"retry":{"base":500,"max":10000,"jitter":20}}`；`heartbeat:0` 关心跳、`retry:false` 关重连、未知键/坏 JSON/死窗≤探针窗都是 mount 报错）；stage 按到达编码回 pong 且不转发，reflux 在 dispatch 前回 pong（不进 handler/refused/log）；与服务端自带心跳的分工写在 ADR 0044 §4（客户端先放手，服务端窗口是它的几倍）— ADR 0040 §3.1 加节 + ADR 0044 — 2026-11-25
  - 帧形状 `{"action": "move", "slot": <列表槽>, "id": <行的 id>, "position": ±n}`；`position` 与 `append` 同向（`0` 尾、`+n` 从尾往里数、`-n` 从头往里数），但**按"把这一行先拎出来"的列表计数**（`len - 1`）——否则"删之前数还是删之后数"两条都自洽，实现和人读的会是两条。
  - 域：行必须在列表里（不在就拒收，与 `append` 拒重复 id 是同一条边界）；`position` 越界由渲染器拒（`move_index`，与 `append_index` 并列，域比 `append` 少一格）。
  - 渲染器：改的是信号里的行序，diff 交给 keyed 列表（`components/tiles.rs` 的 `tachys::view::keyed`）——id 没变的行复用节点，所以焦点、动画、滚动位置都不丢；非 keyed 的容器按"重建子树"兜底，语义仍正确。今天要重排只有"删光再插回"，中间态一旦渲染，节点就真的没了。
  - 连带：`content::Content` 加 `Move(MoveOp)`（`crates/content/src/lib.rs` 的七动作集变八）；`stage -- schema` 导出；ADR 0005 补一节（`append` 的 position 记法在那儿，`move` 沿用并说明域）；下游 reflux 的 `wire.move()` 与 `Slot.move(id, position)`，`docs/ideas/reactive-signals.md` §七那条"wire 没有 move"的天花板随之撤掉。
  - 判据：一个十行列表把最后一行挪到最前，帧里只有一条 `move`（不是十条 remove/append），页面上那一行的输入框焦点还在。

## 开着

- [ ] 模块实例在宿主重建或换布局后不进 `unmount` — 2026-10-23 实测，未修

## 约定（非代码）

- 流式行要带 `id`。
- 导航（以及任何同质分组）走数据；`frame` 只是静态骨架。
- 流式值只落在数据槽，永不进布局平面。
- 图的渲染器是场景层，边住在场景里 — ADR 0018 — 2026-10-26。

## 外部集成（2026-10-29）

- [x] `ui-core` / `boot` 拆分 — ADR 0021
- [x] 信封归宿主 — ADR 0021 决策 6
- [x] 线上槽位的词是 `slot` — ADR 0022
- [x] 上行目标叫 `dest` — ADR 0023
- [ ] 传输是一个运行时钩子：`window.fluxenTransport = { emit, subscribe, close? }`，mount 前读一次、只搬字节（codec 仍归 Rust）、形状不对即 mount 报错且不静默回退、重连归钩子；默认路径仍是 `WsTransport`，reflux/prism 不装钩子 — ADR 0040 — 待做

## 已修 — 这条路付过的代价

- [x] trunk 0.22 拒绝 copy-dir 的绝对 href，`x start` 在装配阶段就死 — 2026-10-23 修
- [x] 文本节点第二次 `.class()` 抹掉第一次；markdown 正文没有排版规则 — 2026-10-12 修
- [x] 镜像构建的 glTF 拉取死在 `curl: (23)`：被 gitignore 的目录在 CI 里不存在 — 2026-10-12 修
- [x] rack 的自动跟随只在列表写入时触发，内部变长的一行永远不跟 — 2026-10-12 修
- [x] 模块 update 通道是死的：G2 v5 没有 `chart.update()` — 2026-10-02 修
- [x] 写一行会重建那一行并重挂所有模块宿主 — 对流式值 2026-10-02 已修
- [x] 发出的事件不带节点 id：wrapper 的 `id` 一直是空的 — 2026-10-02 修
- [x] Enter 什么也不做：console 把 CBOR 上行丢了 — 2026-09-30 修
- [x] 输入框回车即清空（dioxus 移植遗留）— 2026-09
- [x] 通知分发，走 per-key 槽 — 2026-09