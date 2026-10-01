# Fluxen Plan

## Active plan（2026-09，已确认执行中）

### 0. brick → accrete 更名（ADR 0004，done 2026-09-26）

- 全仓机械改名 48 文件：crate 目录、Accrete/AccreteOps/ClassifyAccrete、
  #[ui_acrete]、render_accrete/parse_kdl_to_accretes；firebrick 保护。
  wire 格式零变化（type 值不含 crate 名，非破坏性）。三门槛 + e2e 复验全绿。

### 1. ev 操作通道（ADR 0003，先行）— done 2026-09-26

- `content::Message<T>` 顶层加 `ev: String`（必填，破坏性；与 sender/content 平级，
  不新增嵌套层）。渲染类取值固定 `"draw"`。
- `ui_leptos::dispatch_msg` 首行过滤：`ev != "draw"` → debug 日志 + return，
  不触碰 layout/data/list。其余内部（create/set/join、Influx.event 寻址）不动。
- `stage::proto::parse_kdl_to_frame` 填 `ev: "draw"`；content codec 测试补字段；
  全仓 Message 构造点同步。

### 2. KDL 解析换 kdl 6.7.1（真 KDL v2 语法）— done 2026-09-26

- 原计划用 knus，实测否证：knus 3.4 的 `keyword()` 只有裸 `true`/`false`/`null`，
  `#true`/`#null` 一律报错——它不是 v2。现用 `kdl = "2.0.0"` 同为 v1 语法。
- 改采回退选项 `kdl = "6.7.1"`：实测 `foo #true`/`foo #null` OK、裸 `true` 拒——
  真 v2。且 nushell 0.115 内置 `from kdl`/`to kdl` 的 v2 实现即此 crate（诊断文本
  逐字一致），选型获双重背书；本地 registry 已有，不引 v1 feature（kdlv1 依赖不在
  离线 registry）。
- `stage::kdl_parse` 重写为 kdl 6.x AST（entries 带 Option<name>，args/properties
  两路）→ serde_json::Value，保留现有映射约定（bind/item/attrs/style/grid/data
  子块 + 裸值参数）。实测该 crate 连默认与 v2_parser 都拒子块内单行/多行
  `key=value`（nushell 同拒），"子块条目=值参数节点"的约定维持不变。补两处旧限制：
  - `template` 节点：`template { name "x"  data { ... } }` → 现有 data 子块
    映射（`data_*`）是错的，改为产 `{name, data:{}}`，对齐 accrete::Template。
  - bind kind 节点支持子节点 `payload { ... }` → `{kind:"field", field:"s",
    payload:{...}}`（serde flatten 后 payload 平铺，字段名改 `ev` 与否无关，
    bind 里的 `event` 键不动）。
- 布尔一律 `#true`：`examples/kdl/chat_layout.kdl`、测试 SAMPLE、README 示例
  同步改写；`cargo run -p stage -- tojson` 逐文件验证。
- 回退选项（原计划：kdl 6.7.1 v2_parser）已被采纳为主路径。

### 3. YAML 载体并存（全表达力兜底）— done 2026-09-26

- stage 加 `serde_yaml`；`POST /send?fmt=yaml`（默认 kdl，显式选择优于嗅探）。
- YAML 文件约定：内容 = 一个 Content 项或 Content 数组（`action:/event:/method:/data:`
  头，同 fluxora 文件形状）；stage 包成 `{ev:"draw", sender:"stage", content:[...]}`，
  action 不再被强制成 create（/send 现状问题）。retired `sub:` 键在原始树上递归
  拒绝（serde 会静默丢弃未知字段——空子树无报错不可接受）。
- Accrete 用现行形状（children/tagged bind），不是 fluxora 的 sub 形状。
- `tojson` 子命令同样 sniff 扩展名支持 .yaml。

### 4. 示例迁移：fluxora/data/message/*.yaml → fluxen/examples/yaml/ — done 2026-09-26

- 23 个文件逐个转换 + `stage tojson` 验证；`sub:`→`children:`、bind 补 kind 标签。
- 追加规则：fluxora 的 `type: render` 砖即 fluxen 的 `template`（name+data 同形），
  迁移时改名。`/tmp/push_demo.py` → `examples/push_demo.py`：帧补 `ev:"draw"`、
  selector 移进 `attrs`（旧脚本顶层 selector 会被 serde 丢弃）。e2e 实跑：2 行进
  rack、ask 行命中 accent 模板、token 追帧 merge 进同一行。

### 5. nushell 发送工具：examples/fluxen.nu（移植 x.nu）— done 2026-09-26

- `send <file> [-p <record>]`：KDL/YAML 按扩展名分派，POST 到
  `127.0.0.1:3002/send`（?fmt=yaml 相应）；patch 仅 YAML。
- `border-flashing`：x.nu 的 deep-merge 列表补丁在 nushell 0.115 的 table 收拢
  下语义不可靠，改为 cell-path `update` 精确设值（同一帧形状）。DOM 实测
  primary→disable→secondary→accent 轮换。
- `message-concat` / `message-replace`：0.8s 循环发 02.concat/02.replace。
- `watch-message` 不移植（gateway 不存在）。README/README.zh 补 YAML + ev 文档。

### 执行顺序与门槛

1 → 2 → 3（同一次 stage 改动集）→ 4 → 5；每步过
`cargo build --workspace && cargo test --workspace && cargo build -p ui_leptos --target wasm32-unknown-unknown`；
渲染行为相关（§1 过滤路径）跑 e2e（draw 帧正常渲染、伪造 ev 值不渲染且无 panic）。
主题分提交：ADR+代码各一、kdl_parse+示例改写一、yaml+examples 迁移一、nu 工具一（提交前确认分组）。

收尾修复（2026-09-30 e2e 暴露）：`dispatch_msg` 的 layout 根 patch 路由应用成功
后未写回 `ctx.layout` 信号，响应式根永不重建——现 `is_ok()` 时 `set(d)`。
`x.nu` 的 `flashing-frame` 同步改为单条 layout 根 patch 帧（不再重发整棵树）。
e2e 复验：create 布局 → patch `/children/.../item/1/attrs/class` → 新行 DOM 带
新 class（primary→disable 两轮）、已有行 `===` 存活。

### 6. 操作层重构：Value 与 Patch（ADR 0005）— done 2026-09-30

- `content`：`Content` enum 改为 create/set/append/patch/remove/empty；
  新增 `PatchOp { event, id, path, op, value }`（path=JSON Pointer，
  op=replace|append）；删 `Method`、`Influx.method` 字段、`join`。
- `accrete`：删 `merge.rs`（位置合并）；`AccreteOps`/`classify` 相应收缩；
  patch 应用逻辑放 accrete（`apply_patch(&mut self, path, kind, value)`），
  未命中返回 Err 供上层 warn。
- `stage`：proto/kdl_parse 的 join→append；新增 patch 的 KDL 语法
  （`patch { event "chat" id "a1" path "/bind/value/default" append "tok" }`）
  与 YAML 载体支持。
- `ui_leptos`：`dispatch_msg` 删 merge 分支，Set 保持整体替换；Append 仅
  push，行 id 与现有列表冲突则拒收 + `tracing::warn`（keyed 身份在边界守住，
  不留给渲染层）；Remove 按 id 删行；Patch 按平面路由
  （""→layout，id→行内，其余→数据槽）。
- 测试先行（RED→GREEN）：`apply_patch` 单测——replace 深层值、append
  字符串、append 数组、未命中 Err、attrs 路径、类型冲突写回失败回滚
  （节点保持原值）；`dispatch_msg` 平面路由单测；append 同 id 拒收单测。
- 示例迁移：02.concat/02.replace 改写为 patch 帧；push_demo.py 的 a1 追帧
  改 `patch id=a1 append`；examples/kdl 与 x.nu 工具同步。
- 门槛：workspace build/test + wasm build；e2e 跑 push_demo 等价流程
  （create layout → append 行 → patch token 流逐帧增长、兄弟行 DOM 存活）。
- 删除项记录：`Method`、`merge.rs`、join 的合并职能、02.concat/02.replace
  旧形态。fluxora 为存档，不做兼容。

### 7. Canvas 组件：外部渲染模块原位挂载（ADR 0007）— done 2026-10-01

- accrete 新变体 `Canvas { id, url, attrs, bind }`（wire tag `canvas`），
  derive 门控照 Chart 抄（schema/ops/classify + `has_id="true"`）。
- ui_leptos 挂载：NodeRef on_load → 动态 `import(url)` → 导出契约检查
  （mount/update/resize/unmount）→ mount(el, CBOR(bind["value"]), host)；
  槽信号订阅 → update；ResizeObserver → resize；Owner drop → unmount。
  失败/缺导出 = warn + 空容器。
- data 区零新机制：载荷 = `bind["value"]` 槽，流式喂 = 现有 set/patch
  pointer 路径；`host.send` 上行通道留口不设语义（事件绑定后置）。
- 测试：serde 回环 + schema dump 含新变体；挂载路径 e2e（用最小探针模块：
  导出四函数、mount 画个色块，验证 create→update→resize→remove 全链）。
- 后续独立小改动（勿耦合提交）：Chart 渲染器演进（ApexCharts eval → GoG
  spec + 别的渲染实现）——独立议题、独立 ADR，与 Canvas 无关。

收尾（2026-10-01）：待办五项落地情况——
- [x] 模块产物落 `crates/ui_leptos/assets/3dbrowser/`（glue+wasm+index.js+
      tile.glb 样例模型；目录已进 .gitignore，重建配方在模块 Cargo.toml
      头注释；wasm-bindgen CLI 须与 wasm-bindgen crate 同版本，本机用
      trunk 缓存 0.2.128 配 0.2.129 依赖会报 schema 错配）。
- [x] 示例帧 `examples/yaml/13.canvas.3dbrowser.yaml`：create(source 绑
      空槽) → set(完整载荷) → patch(槽内指针流式改色)——source 形态
      是流式的正解（inline default 不随后续帧重推，e2e 抓到的形态差）。
- [x] e2e 全链：create→update→resize→remove 通过（remove=换布局，
      unmount 无残留报错；tile.glb 实际被拉取入场景）；host.send 上行
      已验证（模块 dblclick → host.send("pick", CBOR {})，stage console
      收到 `<- {"event":"pick"}`）。后台标签页 rAF 节流会冻结模块的
      asset 泵——测试须 bringToFront，产品无关。
- [x] ADR 0007 已接受。
- [ ] forget() 仍是失败重试桩（示例够用，生产前补）。
宿主 e2e 修复三缺陷（canvas.rs）：`new URL()` 返回对象不是字符串
（原 Rust 侧 dyn_into::<JsString> 必失败，import 解析整体移 JS 侧）；
ES module namespace 原型链为 null，`dyn_into::<Object>` 必拒（保留
JsValue，契约查找走 Reflect）；空槽喂 CBOR null 令严格 decode 模块
mount 即拒（改喂 `{}`——"先挂空布局后写槽"是流式标准顺序）。
模块侧（3dbrowser）：glue 引用名笔误 `3dbrowser.js`→`browser3d.js`；
three-d-asset 补 `http` feature（否则 FeatureMissing("reqwest")）；
相对 asset URL 在 frame() 泵里按 document.baseURI 归一化（reqwest
只吃绝对 URL）。

后续单元（本地联动：bind 路由机制，统一协议而非 Canvas 专属）：
- [ ] 需求：点击频道列表切频道、点菜单切页面——组件间纯前端联动，不走
      transport 往返。现状所有 emit（button/select/form/input 的
      `kind: event`）一律上行；接收侧（`kind: source` 读 data/list 槽）
      机制已存在且组件无关。缺的只是路由：把某个事件的落点从服务端改成
      本地信号。
- [x] 命名裁决（用户 2026-10-01）：事件类本地落点用 `Local { signal }`
      新变体；历史死变体 `Target { target }` 太模糊、删除（不复活）。
      kind 集合改动是 wire 面——同步 schema/示例/生产端。
- [ ] 方案：bind 的事件类 kind 增加本地目标 `Local { signal }`：
      emit 组件命中本地目标 → 直接写 Ctx 具名 data 槽，不上行。接收侧
      零改动：case/placeholder 渲染 source 槽 = 切页面；rack 订阅 list
      槽 = 切内容。Canvas 只是新增的一个 emit 方（host.send 同样可路由
      本地），机制对它无特判。
- [ ] wire 面影响：kind 集合改动需同步 schema/examples/生产端；布局帧
      由 AI 生产者声明连线（事件名→槽名），UI 核心不预设任何业务联动。
- [ ] bind 现状核对（2026-09-30）：kind 标签是 serde 统一形态的产物——
      早期组件类型隐式决定方向（输入框=发送、纯展示=接收），加字段后
      历史 `Target { target }` 变体全仓无消费者（ui 不读、stage/examples
      不产，死变体）。注意 kind 集合改动是 wire 面（schema/示例/生产端同扫）。

### 8. 移除 minijinja：template 降级为纯槽位替换（ADR 0006）— done 2026-10-01

- wire 零改动：`action: tmpl {name, data:String}` 注册、
  `Accrete::template {name, data}` 应用形态不变；仅 `expand` 语义收缩为
  `{{key}}` 槽位 → JSON 序列化值文本替换，替换后 `from_str::<Accrete>`。
- 不支持 `{% %}`/过滤器/路径取值；循环逻辑上移生产端（01.segment 示例改造）。
- 错误策略：静默吞掉改为 warn + 节点保留原形（expand 返回结果，warn 在
  ui_leptos 侧，accrete 保持零日志依赖）。
- `template` cargo feature 删除、变体常开（替换实现零依赖，门控失去理由；
  此项为超时未裁决的推荐默认，用户可推翻）。
- 依赖删除：minijinja（根/accrete×2/ui_leptos）；TMPL 静态改
  `RwLock<HashMap<String,String>>`；`crates/accrete/src/template.rs` 重写。
- 测试改写：template_expand.rs 用槽位语义重写（含 parse 失败保留、
  未注册名 warn、循环片段由 data 提供）。
- 门槛：workspace build/test + wasm build + e2e（tmpl 注册 → 挂 template
  节点 → 换数据重发 create，DOM 更新）。

## Resolved

### Enter "does nothing" — console dropped CBOR uplinks (fixed 2026-09-30)

Typing + Enter in the chat input sends the event frame and clears the box, yet
the stage console printed nothing, so the only feedback surface of the dev
mirror looked dead. Root cause was not the widget: `console.rs` printed only
`Message::Text` frames, while the UI's default send codec is CBOR (binary) —
every event frame uplinked by a default page was silently dropped by the
printer. The console now auto-decodes Binary frames back to JSON
(`ActiveCodec::decode_auto`, first byte decides) before printing. Verified
e2e on the default (CBOR) page: `<- {"data":"cbor-fixed","event":"message"}`.

### Input clear-on-Enter (dioxus-port defect) — done 2026-09

Ported from dioxus with the clear logic intact (`slot.set(default)` after
`ctx.send`) yet Enter no longer emptied the box: the leptos closure re-ran
(logged the cleared value) but the DOM input kept showing the typed text.
Root cause is browser value-attribute semantics: once the user types, the
`defaultValue` is dirty and attribute-level updates never reset the shown
value; a persistent node needs an imperative `set_value("")`. Dioxus's
whole-tree vDOM rebuild hid this. `textarea_` still has the same shape
(`slot.set(Value::Null)` with attribute-only binding) and will show the
same symptom — fix only if a consumer needs it.

### Notification fan-out (per-key slots) — done 2026-09

`data` / `list` were single whole-map signals: any `Set`/`Join` frame notified
every bound widget across all keys. Now the outer maps store lazily-created
per-key slots (`DataSlot` / `ListSlot`) and values publish through the inner
signal, so a write notifies only that key's subscribers. Row-level isolation
within one key comes from rack's per-row Owner + `Memo<Accrete>` (untouched rows
recompute to an equal value and never re-render).

Residual tail, not a defect: a frame touching key K still runs every row's
Memo for K's racks — O(rows) cheap recomputes (Arc clone + linear id find),
no DOM work. If row counts reach the thousands, add a per-rack row index
(`HashMap<id, position>` maintained alongside the list) to make the lookup
O(1). Deferred until scale demands it.

## Conventions (not code)

### Streaming rows should carry `id`

Rack keys rows by `Accrete::id`, falling back to position (`#{idx}`) for
id-less rows. Id-less rows render correctly but lose DOM identity whenever a
row is inserted before them (keys shift). Producers that stream (chat, logs)
must set `id`; composite identity is the producer's job (e.g.
`id "alice:m42"`) — the renderer deliberately has no key-template config,
since the Join merge contract (`cmp_id`) is pinned to `id` and a second key
axis would split row identity.
