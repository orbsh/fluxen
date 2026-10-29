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

后续单元（本地联动：bind 路由机制，统一协议而非 Canvas 专属）
— done 2026-10-01，落地记录见 §9 / ADR 0008：
- [x] 需求：点击频道列表切频道、点菜单切页面——组件间纯前端联动，不走
      transport 往返。现状所有 emit（button/select/form/input 的
      `kind: event`）一律上行；接收侧（`kind: source` 读 data/list 槽）
      机制已存在且组件无关。缺的只是路由：把某个事件的落点从服务端改成
      本地信号。
- [x] 命名裁决（用户 2026-10-01）：事件类本地落点用 `Local { signal }`
      新变体；历史死变体 `Target { target }` 太模糊、删除（不复活）。
      kind 集合改动是 wire 面——同步 schema/示例/生产端。
- [x] 方案：bind 的事件类 kind 增加本地目标 `Local { signal }`：
      emit 组件命中本地目标 → 直接写 Ctx 具名 data 槽，不上行。接收侧
      零改动：case/placeholder 渲染 source 槽 = 切页面；rack 订阅 list
      槽 = 切内容。Canvas 只是新增的一个 emit 方（host.send 同样可路由
      本地），机制对它无特判。
- [x] wire 面影响：kind 集合改动需同步 schema/examples/生产端；布局帧
      由 AI 生产者声明连线（事件名→槽名），UI 核心不预设任何业务联动。
- [x] bind 现状核对（2026-09-30）：kind 标签是 serde 统一形态的产物——
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

### 9. bind 本地路由：Local 事件落点 + 值平面订阅（ADR 0008）— done 2026-10-01

- BindVariant：删死变体 `Target`；增 `Local { slot, path? }` 落
  **独立值平面** `ctx.vals`（裸 Value，与 data 的 Accrete 分平面——
  用户裁决：事件数据不是展示形态）；`Source` 增可选 `path`
  （进槽节点 WIRE SHAPE 的只读指针 `get_at`）。
- 发射侧收敛：`Ctx::emit` 助手（Event→ctx.send 上行 / Local→值槽整值），
  input/form/use_target 三路改接；一 bind key 一落点；槽未写入回退
  自身 default（与 Source 同语义）。
- 订阅侧：use_source 统一读（Local 裸值 pointer / Source wire-shape
  pointer），凡取值走它的组件天然可订阅，零逐个改造。
- 边沿=约定非机制：瞬时事件载荷带 seq/时间戳，电平槽写整值天然可见。
- wire 面同步：kdl_parse `local` kind（首参→slot）、schema 自动跟
  serde、示例 kdl/yaml 14.local_routing（select→chan 槽→header）。
- 测试：bind_routing.rs（Local/Source.path serde、Target 拒、get_at、
  roundtrip）、kdl local 解析断言。
- e2e：点菜单项→header 变 `tech` 且 stage console 零上行帧（通过）；
  Event 路径回归（Enter 上行完好）。
- 门槛：三门槛 + e2e；破坏性 wire（kind 集合增删）单独提交。

### 10. Chart 渲染器演进：G2 spec + 模块通道（ADR 0009）— done 2026-10-01

- 载荷 = G2 spec（纯数据、GoG 正统）；`bind["value"]` 槽直装 spec，
  流式更新 = 指针 patch 进 `/data` + 同实例 options 重设 + render 重绘
  （ADR 0005 零新机制；v5 无 `chart.update()`，见 §13）。
- 去 eval：薄封装 ES module（mount/update/resize/unmount 契约，内部
  动态 import g2、CBOR→spec 解码），chart.rs 复用 ADR 0007 的
  import(url) 通道；`Chart { url }` 字段 serde default 向后兼容。
- 资产：g2.min.js（321KB gz，@antv/g2 5.4.8）进 assets/ 照 3dbrowser 的
  gitignore+重建配方模式；ApexCharts 资产与 index.html script 标签退役。
- 示例：08.apexchart.yaml 改写为 G2 spec；08.chart.yaml 保留驱动帧。
- 验证：headless 已证 spec 渲染出 canvas；tooltip/interaction 的鼠标
  行为需有头浏览器目检（实施 e2e 覆盖）；交互细节未取证前 ADR 保持草案。
- 门槛：三门槛 + e2e（create 布局挂 chart → set 换 spec → 重绘；
  patch append 数据行 → update）。
- 实施记录：canvas.rs 抽出共享 mount_module（chart_/canvas_ 均为薄包装，
  差异仅 url/容器样式/绑定解析）；Chart 增 url 字段（serde default
  /assets/g2chart/index.js，向后兼容，测试锁形态）；g2chart 封装 =
  UMD script 自举 + 最小 cbor.js（roundtrip 测过 uint8/16 头、UTF-8、
  空 map）。修一个实施期真缺陷：UMD `<script>` 相对 src 按文档 base
  解析（页面在 / 时 404）→ `new URL(..., import.meta.url)`。
  ApexCharts 资产/index.html 标签/Trunk.toml 代理/08.apexchart.yaml
  退役；08.g2chart.yaml + 08.chart.yaml 为 G2 形态。e2e：headful
  CDP 探针 CHART-OK（inline 路径 canvas 1280x960），最终由用户在
  stage 直连验证通过（图表渲染与交互目检）。

### 11. Pages 组件 + 事件形状统一（ADR 0010）— done 2026-10-02

- emit 载荷形状与落点解耦：Local 落槽改写包装对象
  `{event, id?, data}`（与 content::Outflow 同构，复用该类型）；
  `Local` 增可选 `event` 字段（缺省=槽名）。path 自此有稳定提取对象。
- 新变体 `Pages { id, attrs, bind, display }`：bind.value→list 页面行
  （id=字典键，复用行身份纪律）；bind.select→local/source + path 提键
  （tracked）。display=render（只挂命中行）| dom（全挂、未选中 hide，
  滚动/输入/GL 状态保持）。未命中键 warn+空白。
- 占满空间（用户 2026-10-02 追加裁决，同 case/rack 模式）：pages 容器
  带 `f` + 类型表授予 `col`（use_common_css 的列表已含 Pages），命中页
  包壳 `page f` 占满剩余空间；隐藏页不带 `f`——同 div 上 `.f` 与
  `.hide` 冲突（实测 display 被算回 flex），互斥做进 class 切换。
  e2e 实测：窗口 1084 时 pages 区 1036、可见页与内层根均 1036、
  rack 955；dom 模式切走再切回 textarea 同一节点、草稿值存活。
- 15.tabs.yaml 重写为诚实形态（菜单标签与页面内容互异，同事件两处
  取件：标题 /data、Pages /data）；14.local_routing 示例同步包装形状。
- 00.chat.yaml（原 16.chat_in_pages.yaml 的内容，落地时归入 00.* 系列）：
  Pages 结构为基础 main 布局，00.chat_layout
  的整棵聊天树作为一行（id: chat, display: dom）append 进字典——
  chat 成为其中一个页面，切走滚动/草稿保持。
- 测试：emit 包装形状（Local 带/不带 event、id 透传，content/tests/
  outflow_shape.rs + accrete/tests/bind_routing.rs）、Pages 键提取/
  未命中（components/pages.rs 单测）；三门槛 + stage 直驱 e2e 全过。
- 缺陷已修（用户 2026-10-01 目检发现）：15.tabs.yaml 的 tab 栏
  `horizontal: true` 失效——根因是 `ClassAttr` 没有 horizontal 字段，
  use_common_css 无条件给 Select 加 `col`，父 case 的 horizontal 只作用
  于父 div。修法：ClassAttr 增可选 `horizontal`（ClassifyAttrs derive
  自动认），15.tabs 的 select 自带 `attrs: {horizontal: true}`；e2e 实测
  select computed flex-direction=row。

### 12. 远程 Vars 注入（ADR 0011）— done 2026-10-02；页面状态快照/恢复 — 暂缓（用户 2026-10-02：先不做）

通道（ADR 0011，用户 2026-10-02 定名 `inject` + 字段 `slot`）：

- 新 Content 变体 `Inject(InjectOp { slot, event?, id?, data })`，wire
  `action: inject`；载荷即 ADR 0010 的 `Outflow`，逐字段落在 op 上（不套
  value 外壳）。订阅方与浏览器自发 emit 不可分辨；`event` 缺省 = slot。
- 该变体不泛型：vals 平面的货币是裸 Value，不是 Accrete——两个平面的差异
  第一次落在类型上；accrete 与 `stage schema` 零改动，唯一改动点是
  content crate 的闭词汇。
- 写语义 = 整值覆盖 + 同值短路（写读环在第一跳终止）；每次注入打日志
  （槽名 + 值）；操作名自带成本信号，不加 flag / 开关 / 权限位。
- 排除的形态：合并进 data（形状与写语义结构性不同，快照无法判断可恢复
  性）、restore-only 通道（强制视图状态是真实第二场景）、forced 子通道
  （订阅方要区分"谁写的"，形状一致性丢失）、origin 标记（同值短路已挡住
  常见回显环）。
- 示例 `examples/yaml/00.inject.yaml`（骨架下 inject `page` = home，把 main
  的当前页切过去）；单测（缺省 event = slot、id 透传、同值短路、载荷形状与
  emit 逐字段一致）；e2e（inject `page` → pages 切到目标行；随后点一次同名
  交互，槽值不变）；三门槛 + e2e。
- 实施（2026-10-02）：content crate 加 `Inject(InjectOp)` 与
  `InjectOp::outflow()`（"缺省 event = slot"因此进了 content，可单测）；
  ctx.rs 的 `dispatch_msg` 加 inject 分支——写 `slot_for_value(slot)`，
  同值短路抽成纯函数 `inject_writes`；info 日志经浏览器 tracing_wasm
  （main.rs 装的是 INFO 级）可见。
- 验证：workspace build/test 全过（content 新增 6 条、ui_leptos 新增 3 条）；
  trunk build 出 dist；`stage tojson 00.inject.yaml` 确认 YAML 载体认识新操作
  （event/id 缺省在 wire 上省略）；e2e 实测 pages 受控切换——注入前空白、
  一帧 inject `page` = home 后首页出现、同值重发无变化、改值切到 about、
  未命中的键回落空白。

快照/恢复（暂缓：用户 2026-10-02 决定先不做，方案留档；骑同一条通道，尚未定案）：

- 导出（dump）= 一条上行载荷，装 layout / data / list / vals 四个平面 +
  可滚动容器的 scrollTop；回灌（restore）= 若干 inject + 滚动回填。
- 滚动的锚点复用现有寻址元组 `(event, id?, path)`（rack 某行内的容器 =
  `{event: "chat", id: "<row>", path: "/children/1"}`），不引入第二种
  路径语法；滚动是渲染器私有的 DOM 状态，读写与回填都归渲染器。
- 未定：快照的判定标准——收 Vars 全量还是只收可恢复的那部分（决定要不要
  给槽或值加持久性判据）；dump 的载荷形状；恢复的时机与幂等要求。落地前
  单独写 ADR。

### 13. 流式内容走订阅槽 + 模块 update 通道修复（2026-10-02）

两处实测缺陷（详见 Resolved）：

1. g2chart 封装模块的 update 调 `chart.update()`——G2 v5 没有这个方法，调用抛
   TypeError，宿主只打 warn 不重绘：源槽绑定的 chart 收到逐帧写入后完全不动
   （实测写入前后实例数/重绘数都不变）。
2. 写布局节点（inline default）会重渲染整行：行内模块宿主被 destroy + 重新
   mount + render，实测 4 个 G2 实例 → 一帧后 8 → 再一帧 12，canvas 与宿主
   div 全部换新（DOM identity 探针），而同行的 text 节点原地存活。

修法（本次）：

- 封装模块 update = `chart.options(spec)` + `chart.render()`（v5 的正确增量
  路径：options() 只做 spec 树差量、不落笔，重绘必须显式 render()；增量只在
  同实例续用下成立）。
- 流式内容从布局平面搬进数据槽：00.radar.yaml 的折线图
  `bind.value.kind: source → trend`、摘要 text `→ source: summary`，文件由 1 帧
  变 3 帧（append 行 + set `trend` + set `summary`）；walkthrough 改 patch 槽
  （trend `/bind/value/default/data`、summary `/bind/value/default`），行 id
  注入机制随之删除（不再按行寻址，行保持无 id、文件可反复 send）。
  （2026-10-10 更新：三个雷达也搬进数据槽 `radar_rd` / `radar_mkt` /
  `radar_ops`——空载荷不占位（ADR 0012）要求图表数据可以单独后发，所以该文件
  现在 6 帧；交互演示改名合并为 `examples/walkthrough.py`，它搭自己的一行
  （一个 box：引言 / 折线 / 雷达 / 摘要 / 复核表 / 车 / 鞋），只把 00.radar.yaml
  的 set 帧当图表数据源——发整份会多开一个 box。实测时间线：行入场时 6 个模块
  宿主全 display:none（box 只有 650px 的引言 + 两条图表带），图表数据到达后
  四个图表就地展开，随后依次是摘要（打字机）、复核表（append 进 rack，
  `tables=1`）、车文字 → 泼溅 720×460、鞋文字 → 鞋 640×420，全程 rows=1、
  行高 650 → 2168。）

e2e（stage + headless CDP）：挂载后 4 实例；追加一天（trend 槽）→ 仅折线图
实例 render +1、实例数与 canvas 身份不变、0 销毁、三个雷达零重绘；追加一个
token（summary 槽）→ 零图表重绘；无 update threw 警告。整段演示
`walkthrough.py --auto`：折线图 data 90 行（30 天 × 3 部门）、摘要完整。

门槛：三门槛 + 上述 e2e。

### 14. 容器子节点级 Memo — 实测否证 2026-10-12（流式只走数据槽）

原方案（形态与判据留档）：容器内子节点没有 memo（`render_children` 直接 map
dispatch）：任一子节点变化 → 容器 render 闭包重跑 → 模块宿主重挂。rack 有行级
`Memo<Accrete>`（未变行不重算），容器层没有对应物，所以"写布局节点"这条路线的重挂
问题仍在（§13 只把流式内容搬出布局平面，没有改渲染路径）。判据：Accrete 是纯数据 +
PartialEq，节点值相等即可跳过该子树——数据本身即虚拟树，剪枝搭在数据相等性上，
不需要额外 vdom 层（diff 仍由 tachys 在 view 树上做）。注意跳过只对"节点自身数据
未变"成立：订阅驱动的更新（source/local 槽）在组件自己的闭包里重跑，不依赖父闭包，
memo 不得挡在订阅组件的更新路径上。

实测（2026-10-12）：三条路都走不通，容器级隔离在本渲染栈里不可达。

- 每子节点一个 `Memo<Accrete>`（建在 `render_children` 里）是空转——DOM identity
  探针：给一个 canvas 打标记、patch 它的兄弟，标记消失。根因是 Memo 建在父闭包的
  render 作用域里，父一重跑它就死掉重建（新实例、初值恒等），判等因此恒真。挪进
  每行的 `Owner` 也不行：实测布局信号写入时该 memo 从不重算，尽管根闭包重跑了——
  rack 的行能被通知，是因为每行 memo track 的是**按键的槽信号**，不是整平面那个。
- rack 形状的 `keyed()`（同步建在组件构建路径里）直接 wasm panic
  （`RuntimeError: memory access out of bounds`）。症结不在 Owner 挂哪儿：行视图 +
  行 memo 以 prev=None 反复重跑数百次（每轮重建 Memo，判等空转），而 `create:
  layout set` 只发了 1 次——keyed 自己构成自重建订阅环，最终耗尽 wasm 内存。
- `keyed()` 包进外层动态闭包（rack 的形状）不再 panic，但 tachys 的
  `Render for F::rebuild` 是 teardown + rebuild：每次布局写入都整棵重建（新 Memo、
  prev=None、模块宿主重挂），隔离一样没有。

结论：容器级剪枝这条路封死，**流式的唯一可用机制是数据槽**（§13 的规则，已进
Conventions）。判据那段留着，它是对"数据即虚拟树"的正确推理，只是本栈的重建语义
不给它落点。调试这类问题先给宿主 div 和它的 canvas **都**打标记：宿主标记没了 =
keyed/AnyView teardown，宿主在而 canvas 没了 = `on_load` 重发（原地重建）。

### 15. 空载荷的模块宿主不占位（ADR 0012）— done 2026-10-10

形态：一个 box 内图文混排、内容流式到达时（当时的 `examples/product_intro.py`，
2026-10-10 并入 `examples/walkthrough.py`），模型区
在载荷到达前就把地占了——两段文字之间各挂一块 640×420 / 720×460 的空白。文字
本身是动态的（打字时 bubble 一直在长），定死的只有带尺寸的模块宿主。用户
2026-10-10：「box 固定了大小，我希望是动态的撑开」，并选定"模型区一开始不占位、
载荷到了才撑开"，确认走契约语义（不是改示例写法）。

单元（ADR 0012 定语义，本项定落地）：

1. `canvas.rs::mount_module`：容器尺寸从"渲染时静态写入"改为**随载荷反应式
   生效**——载荷空（`Null` / 空 map，即宿主现在合成的"尚无数据"值）时容器
   `display: none`，非空时写回 `SizeAttr::size_style()`。Chart 无 SizeAttr，
   隐藏同样生效，判据不分叉、不加 attrs 字段（wire 零变化）。
2. 载荷反应性：现有 payload 闭包挂载时读一次（`get_untracked()`），需要补一个
   **受订阅的布尔派生**驱动容器属性；挂载路径与三态 `data-module-mounted`
   护栏不动——撑开靠模块自己的 ResizeObserver → `resize()`，不重挂、不重 import。
3. 不加开关（空即无）；要固定尺寸的空盒子就发非空载荷。

门槛：三门槛 + e2e（当时的 `product_intro.py --auto`、现 `walkthrough.py --auto`：
box 起始只有第一段文字 → 鞋载荷到达
后长 640×420 → 第二段 → 泼溅载荷到达再长 720×460；撑开时**不重挂**——canvas
DOM 标记存活、`data-module-mounted` 不变、import 不重跑；13.canvas.3dbrowser.yaml
与 00.radar.yaml 回归无异常）。风险点：模块在退化尺寸（容器隐藏时 client 尺寸
为 0）下挂载、需 resize 恢复——3dbrowser 齐备，splatviewer 的 `resize()` 是
no-op，恢复能力由 e2e 判定，兜底是让它真正 `setSize`（模块侧修复）。

e2e（实测 2026-10-10）：

- `walkthrough.py --auto`（合并前叫 `product_intro.py`）一个 box 的时间线：载荷到达前两个宿主都是
  `display:none`（canvas 1×1），box 高只含文字（50 → 198 随打字增长）；鞋载荷
  到达 → 该宿主 `height:420px;width:640px`、canvas 640×420，box 高 198 → 618；
  第二段打完 → 泼溅载荷到达 → `height:460px;width:720px`、canvas 720×460，
  box 高 814 → 1274。全程 canvas DOM 标记存活（**不重挂**）、两个模块各 import
  一次、errs 空。
- `13.canvas.3dbrowser.yaml` 拆帧回归：只发 2 个 append → 两个宿主
  `display:none;#1x1`；再发 2 个 set → `height:260px;width:300px;#300x260`，
  两件模型都拉到（MaterialsVariantsShoe.glb / ToyCar.glb）。
- `00.radar.yaml` 拆帧回归：只发 append → 绑 `trend` 的图表宿主
  `display:none;#100x100`，带 inline 数据的三个雷达照常 `#321x607`；发 set →
  图表 `#964x606`（无 SizeAttr，尺寸来自 class）、雷达 `#321x1213`。errs 空。
- 风险点结论：模块在容器隐藏（client 尺寸 0）时挂载，靠 resize 恢复——
  3dbrowser canvas 1×1 → 640×420、splatviewer canvas 0×0 → 720×460，都恢复，
  无需模块侧修复。
- 门槛：`cargo build --workspace` / `cargo test --workspace` 全过（29 组
  `test result: ok`、0 FAILED）、`trunk build` 过。

### 16. Float overlay：背景模糊 + 不可操作（ADR 0013）— 被 ADR 0014 取代 2026-10-10

（用户 2026-10-10：「撤」。模态语义归 layer + portal——遮罩不该声明在消息行的
子树里、靠 `fixed` 逃出去；`PositionAttr` 的 `overlay`/`dismiss` 两个字段已从
wire 撤掉。本节记录保留，两条几何实测教训对 layer 一样成立，见 §20。）

形态：登录模态框要"挡住并模糊后面的页面"，而 Float 只有绝对定位 + Popup 的
hover 展开。做法：`PositionAttr` 加 `overlay` / `dismiss` 两个可选字段（该结构
只被 Float 用，爆炸半径一个组件），`float.rs` 在 overlay 模式加类，几何
（`position: fixed; inset: 0; height: 100vh; z-index: 100` + flex 居中）**内联**。
两处实测教训：① 几何不能写在 custom 层——该层因 main.css 的 `@import` 定序实际
排在最前（见 §19），写那儿会被 element 层的 `.float{position:absolute}` 盖掉；
② 本机 headless chromium 上 `position: fixed` + `inset: 0` 的 div 高度**不撑开**
（干净 div 实测 20px；同一元素给 `height: 100vh` 才是满视口），所以高度显式给。
CSS 只留 `::before` 的模糊/色调（`backdrop-filter`，写法同 `--tooltip`）与把内容
压层。不可操作由铺满视口本身保证（点模糊区 = 点在浮层上），dismiss 是本地瞬时
状态（`e.target === el` → `display:none`），不上线协议。

门槛：全过。`cargo test --workspace` 绿（当时的
`crates/accrete/tests/float_overlay.rs` 已随 ADR 0014 删除，换成 `layer_portal.rs`）
+ wasm 目标构建 + trunk 落进 dist。e2e（stage + headless
CDP，跑 `examples/yaml/17.login.yaml`）：`::before` 的 `backdrop-filter` 计算值为
`blur(6px)`；浮层矩形 = 视口（1200×900）；视口角落 `elementFromPoint` 命中的是
浮层自身（= 背后页面点不到）；卡片居中；点卡片不关闭（display 仍 flex）、点模糊区
关闭（display:none）。0 console error。

### 17. 聊天框自动滚动：在底部 + 开启才跟随 — done 2026-10-10

形态：`RackAttr.scroll` 这个选项本来就在（用户 2026-10-10：「修行为，不动 chat
页面」——不加应用层开关）。旧判据是"离底部不到一个视口高"（rack.rs 注入的那段
JS），于是手动往上拖一点的人会被下一次追加**拽回底部**，这就是"手动拖动后还自动
滚"的来源。改成严格到底（`scrollHeight - scrollTop - clientHeight <= 2`）+ 每元素
一个"是否在底部"的旗标，由只装一次的 scroll 监听维护（我们自己的滚动也保持它为
true）：开启且在底部才跟随，上拖即停，拖回底部恢复。

门槛：全过。e2e 的 `scrollTop` 数值三连（探针自建 `scroll:true` 的 rack 布局 +
逐行 append）：在底部追加 → top 601→621 = max（跟随）；`scrollTop=0`（手动上滚）
后追加 → top 仍 0、旗标 `"0"`（位置不动）；再滚回底部后追加 → top = max 661
（恢复跟随）。

补充（2026-10-12，用户报「walkthrough 里没有自动滚动」）：跟随的触发面原先只有
"列表槽被写"——注入的那段 JS 挂在外层动态闭包上，而闭包唯一 tracked 读的就是列表
槽。聊天"每条消息一行"够用，但**一行内部流式长高**（walkthrough 的"一个 box"、
打字机、模块宿主按 ADR 0012 撑开、图表带渲染）不写列表槽 → 整段演示一次都不跟随。
修法：触发面补 `ResizeObserver`（容器 + 元素子节点，childList 的 MutationObserver
负责把新子节点加进观察）与子树 MutationObserver
（`childList+subtree+characterData+attributes`）；"在底部才跟随"的旗标仍由 scroll
监听维护，但改成**认得出自己的滚动**——落在底部、或落回刚钉下的 scrollTop 都算
跟随，只有明显离开两者才算上滚（生长常晚于 scroll 事件派发，按 live 值重算会把
跟随误判成上滚）；追赶用 40ms `setTimeout` 拍（高度连续三拍不变 / 旗标转 0 / 2s
超时即停），不用 rAF。实测数字见 Resolved 那条。

### 18. Canvas 模块的加载过渡 — done 2026-10-10

形态：泼溅那件早就显式关掉了 GS3D 自带的加载 UI（`showLoadingUI: false`，
examples/modules/splatviewer/index.js 两处），翻成 `true` 即得进度/取消 UI（用户：
「用自带的」）。three-d 那边（.glb）没有自带进度可用，模块在自己的容器里放一层
DOM 加载动画（`.browser3d-loading`，样式随模块自带、不依赖宿主页面 CSS；容器设成
定位祖先），装载队列空了才收（多资产时中途不闪）；失败路径下 asset 不进 seen，
下次重试会再建。

门槛：全过。e2e 用 CDP `Network.emulateNetworkConditions`（150 KB/s）放大加载
窗口——注意限速必须放在应用启动**之后**（dev 的 UI wasm 有 7.7MB，先限速的话应用
一分钟才挂载、帧发在它渲染之前，探针第一版就栽在这里）。实测：限速下 t=12s 两个
3dbrowser 宿主 `data-module-mounted="true"` 且 `.browser3d-loading` 在（截图
loading_3d.png）；放开限速后过渡层消失、canvas 300×260、像素分类仍是鞋的暗色轮廓。
泼溅侧：首版断言（容器子节点 > 2）**太弱**——GS3D 的常规 DOM 本来就有 67 个节点，
UI 关着也能过；改用"节点名里带 loading/progress/spinner"判定重验，限速下 t=3s
容器里出现 `div.progressBarOuterContainer` / `div.spinnerOuterContainer0` /
`div.spinner0.spinnerPrimary0` / `div.messageContainer0…`（GS3D 自带加载 UI，截图
loading_splat2.png）。这次重验还抓到一处真漏：模块的**源文件**改了，但应用服务的是
`crates/ui_leptos/assets/splatviewer/index.js`（tracked 的部署副本）——不同步的话
`showLoadingUI: true` 根本不生效，上一轮读到的 67 节点因此毫无意义（模块头注释
「本文件复制到 …assets/ 即可」就是这条部署约定）。

### 19. custom CSS 层实际排在最前（已修 2026-10-10）

实测：main.css 第 1 行曾是 `@import url("custom.css") layer(custom);`，第 3 行才是
`@layer theme, base, layout, element, custom, style;` 的顺序声明。层序由**首次
出现**决定，于是 `custom` 被那行 import 定成第一个（最弱）层，custom.css 里与
layout/element/style 层同属性的声明全部被盖掉——这也是老坑"`.hide` 打不过 `.f`"
（`display:none` 被 layout 层 `.f{display:flex}` 盖掉）的根因。ADR 0013 的 overlay
几何因此改走内联（见 §16），CSS 只留没有竞争者的伪元素规则。

修（用户 2026-10-10「css 按你的方式改」）：把顺序声明移到 import 之前，两行对调，
`custom` 回到 element 与 style 之间。验证：`class="f hide"` 的元素 display 从 flex
变 none（对照 `class="f"` 仍 flex）、`00.radar.yaml` 的图表照常出图（G2 调色板三色
在）、0 console error。影响面本身覆盖 custom.css 里所有同类声明，这次一次性归位。

### 20. layer + portal：模态层的渲染位置（ADR 0014）— done 2026-10-12

形态：ADR 0013 的 Float overlay 把遮罩留在消息行子树里（全靠 `fixed` 逃出去，祖先有
transform/filter/contain 就困住），且"按钮在某条消息里、遮罩属于应用外壳"是本质错配。
用户 2026-10-10 定方案：类似 Vue Teleport——应用**显式声明**一个全屏 layer 作挂载点
（渲染器不加固定逻辑），portal 用 `to` 指过去，显隐由 layer 的载荷槽决定（空 = 不显示，
复用 ADR 0012）；Float 的 `overlay`/`dismiss` 撤掉；ADR 0013 标记被取代。

实现：accrete 加 `layer`（attrs: class/selector/dismiss；bind.value = Local/Source 槽；
children = 模态内容）与 `portal`（attrs: to；children 传送到目标）；ui_leptos 加
components/layer.rs、components/portal.rs 并接入渲染分派；几何内联在 layer 宿主、
遮罩（`::before` 模糊/色调）进 custom.css（层序已修，见 §19）；dismiss 判等逻辑从
float.rs 搬。

门槛：全过 2026-10-12。`cargo build` / `cargo test --workspace` 绿（含
`crates/accrete/tests/layer_portal.rs` 3 例：layer/portal 各类形状过真实 codec 往返，
且 float 的编码结果里不再出现 `overlay`/`dismiss` 两个键）；wasm 目标构建出真产物
（release bin `deps/ui_leptos-511080b6393c1515.wasm`，4 815 990 B，mtime 是本次构建
而不是陈旧 rlib 的时间戳）、`trunk build` 落进 dist。e2e（stage + headless CDP，跑
`examples/yaml/17.login.yaml`，用真输入事件：点触发输入框 + insertText + 真 Enter）：
- ① 槽空：layer `display:none`、rect 0×0（不占位），portal 的内容已经在 layer 里
  （卡片祖先链 …`login_layer.layer f`）。
- ② 写槽后：`display:flex`、rect = 视口 1200×1000、`::before` 计算值 `blur(6px)`、
  视口角落 `elementFromPoint` = `div#login_layer`（背后页面点不到）。
- ③ 点模态卡片中心不关闭（display 仍 flex）；点角落模糊区关闭（清空槽 → none）。
- ④ 内容确实被传送：卡片在 layer 内，触发行里 `.border` 计数 0，没有就地渲染。
- ⑤ 清空槽后再写能重开；`inject page=about` 切页后 layer 是同一元素（`same: true`）
  且模态仍开着。全程 0 console error。

### 21. 组件库按需加载：宿主 import 时机 + 模块求值 + mermaid 预加载（ADR 0015）— done 2026-10-14

形态（用户 2026-10-14：「js 的库要惰性加载，没有用到 editor 组件的时候不要加载」「canvas
库好像也没有惰性加载？比如加载 splat 播放器的时候才加载 three.js」）：三层同一判据
"用到才加载"——(1) 宿主 `mount_module` 的 import + 契约检查 + mount 从 `NodeRef on_load`
改由**首次非空载荷**触发（空载荷 = 零 import；挂载标记仍是唯一状态源，"pending"（在途）
不再重发是本次收紧的那条，旧的"标了 true 但内容被原地重建抹掉 → 清标记重挂"保留）；
(2) 模块把库下载从**模块求值**挪进 `mount()`（splatviewer 收进 `ensureLibs()`、
3dbrowser 收进 `ensure()`）；(3) diagram 的 mermaid 从 index.html 的 `<script>` 预加载
改成首次挂载注入 script（全局 promise 门控，形状同 g2chart 的 `ensureG2`）。ADR 0012
"明确不做"的第 1 条（"首次非空载荷才 mount"）由此改判，0012 挂 Update 指向 ADR 0015；
ADR 0009 §4 的 mermaid"独立事项"结案。判据（"空"是什么）沿用 `payload_is_empty`，
wire 零变化。

门槛：全过。stage :3102（`--dist target/e2e-dist`，trunk release，wasm 2 317 842 B）
+ headless chromium :9333（新 profile），跑 `scripts/e2e_lazy_libs.py` —— 13 条断言
`FAILED: none`，页面异常 0：

- 开页（00.main.yaml 落地、chat 页注入后）资源表**没有** `mermaid.min.js`、没有
  `splatviewer/*`、没有 `esm.sh`。改动前 mermaid 每次开页必下（2666850 B 原始 /
  gzip 771617 B 本机实测），`rg 'esm\.sh|three'` 在 dist 的 html+js 里命中 0（库从不
  进应用包，这条改动前后都成立）。
- 发 `08.diagram.yaml` 后 `mermaid.min.js` 才出现，`.diagram svg` 数 2（mermaid 真画）。
- append 一个绑槽、载荷未到的 canvas 行：宿主 `style="display: none;"`、
  `data-module-mounted` 空、子节点 0、高度 0，**资源表零模块请求**（改动前这一行就会
  把 splatviewer 拉下来）。
- 载荷 `set` 到达后依次出现 `/assets/splatviewer/index.js` → `esm.sh` 的 three +
  GS3D → `truck.ksplat`；宿主标 `true`，宿主内 canvas 320×240。

回归：`scripts/e2e_chart_regress.py`（`00.radar.yaml` 四个槽绑定图表）——radar 之前
零 g2 请求；之后 `g2.min.js` + g2chart 三件、`window.G2` 为对象、4 个图表宿主挂载
（dept-trend / radar-rd / radar-mkt / radar-ops）、4 个 canvas 有像素、页面异常 0，
`FAILED: none`。即"延迟到载荷才挂载"对 chart 这条既有路径没有回归。

两条 e2e 教训（脚本头部同记）：(1) `/send` 的 `delivered to N peer(s)` **不是就绪门**
——页面 WS 已连、渲染还没订阅时帧被静默丢掉（实测：拿它当门放过去后整页空白、console
零输出）；门必须是 DOM 读数。(2) 页面在重渲染忙窗口（swiftshader + 4 个 G2 + WebGL
同屏）里 CDP `Runtime.enable`/`evaluate` 会挂到 socket 超时——读数要重试，socket 超时
放到 60s。

### 22. editor 组件：Monaco 走模块通道 + 保存上行（ADR 0016）— 代码已落，门槛全绿（2026-10-15）

形态（用户 2026-10-14：「实现 editor 组件（monaco，代码作为参数，保存时发送提交事件）」）：
新增 `Editor { id, url, attrs: SizeAttr, bind }`（wire tag `editor`，`has_id`），数据区
`bind["value"]` 载荷 = `{"code": …, "lang": …, "event": …}`（inline default 或 `kind: source`
槽；`{}`/Null 仍按 ADR 0012 不占位）；渲染走 ADR 0007 模块通道、库在 `mount()` 里才拉
（ADR 0015）；封装模块 `crates/ui_leptos/modules/editor/index.js` 用 AMD loader 起
vendored 的 Monaco，绑 Ctrl/Cmd+S → `host.send("submit", CBOR({code, lang}))` 走现有上行
（与表单 submit 同腿）。宿主侧与 chart 同例：`editor_` 是 `mount_module` 的第三个薄包装。

落地（2026-10-15）：
- `accrete`：`Editor` 结构 + `default_editor_url()`（`/assets/editor/index.js`）+ 枚举变体
  `editor`（`has_id`）；`editor_` 在 `crates/ui_leptos/src/components/canvas.rs`，走
  `bind_value` + `mount_module`，与 `canvas_` 只差三类入参。
- 封装模块 `crates/ui_leptos/modules/editor/{index.js,cbor.js}`（部署拷贝进 gitignore 的
  `assets/editor/`）：AMD loader 注入与 `require(["vs/editor/editor.main"])` 都在 `mount()`
  里（模块被 import ≠ Monaco 被下载）；Monaco 根按 `import.meta.url` 解析（g2chart 那条
  教训同款）；`update()` 的语言切换走 `setModelLanguage`，`code` 只在与"上次收到/上行的那份"
  不同时才覆盖（同载荷重复投递不抹掉用户正在编的文本）；尺寸归宿主的 ResizeObserver →
  `resize()`，模块 `automaticLayout: false`（不与宿主重复布局）。
- Monaco 0.55.1 的 AMD 子集 vendored 在 `assets/monaco/vs/`（gitignore）；Dockerfile 加
  装配步（`MONACO_VERSION` + `MONACO_SHA1` + `sha1sum -c`），`.dockerignore` 排除本地副本。
  剪枝口径按实测修正为**只删四个语言服务 worker**（≈ 9.12 MB，见 ADR 0016 决策 5）。
- 示例 `examples/yaml/18.editor.yaml`：inline 载荷（rust）1 个 + 绑槽（python，set 帧送
  载荷）1 个，演示同一契约两种数据来源。
- 帧层测试 `crates/accrete/tests/bind_routing.rs::editor_url_has_backward_compatible_default`：
  url 缺省 / CDN 覆盖 / 省略 bind 与 attrs / `has_id` / 载荷原样（不做节点字段合成）。

门槛（2026-10-15 第二轮浏览器实测全绿：剪枝后的树 + 全新 `--user-data-dir` 的 headless
chromium，`editor_e2e3.py` 26/26 PASS）：
- [x] `cargo test --workspace` 全绿（bind_routing 9/9）；`cargo build --release -p stage`；
  `cargo build -p ui_leptos --bin ui_leptos --target wasm32-unknown-unknown --release`
  （4 817 678 B）；`trunk build --release`（dist wasm 2 323 655 B，dist 含
  `assets/{editor,monaco}`）。
- [x] 装配件核实：tarball `13 677 219 B` / sha1 `e74c6fe5a6bf985b817d2de3eb88d56afc494a1b`
  与 ADR 记的一致；解出的 `package/min/vs` 与本地 assets 树 `diff -r` 逐字节相同。
- [x] 浏览器：无 editor 节点 → 资源表零 `editor/`+`monaco/` 请求；绑槽空载荷 → 宿主
  `display: none;`、`data-module-mounted` 空、零 import；载荷到达 → 按序 `editor/index.js`
  → `editor/cbor.js` → `monaco/vs/loader.js` → `editor.main.js` → 四块 `monaco.contribution-*`
  + `basic-languages/monaco.contribution` → `editor.api-CalNCsUg.js` → `workers-DcJshg-q.js`
  → `editor.main.css` → `python-Cr0UkIbn.js`；三个宿主按节点尺寸落位（1124×200 / 320 / 240），
  当轮 dist wasm 是 `ui_leptos-bde477929ffb0b80`（页面资源表核对，不是陈旧构建）。
  基线读数取自全新 document——承 §21，旧页面上的资源表基线不作数。
- [x] 交互：三个编辑器各以真 CDP 键事件发 Ctrl+S → 上行帧恰为 `event: submit` + 该节点 id
  + `{code, lang}`（rust / python 原文），无多余帧。
- [x] 剪枝后的 lang=ts/css/html/json：高亮仍由 basic-languages 的 monarch 提供（`tokenize`
  出 `keyword.ts` / `attribute.name.css` / `tag.html` / `string.key.json` 等类型；DOM 里
  `mtk6`/`mtk14`/`mtk10`/`mtk20` 等着色的 span 各 40/22/27/27 个）。**失败面不在网络层**：
  四个 worker 由 blob 壳 `workers-DcJshg-q.js` 内部 `importScripts` 拉取，页面资源表与 CDP
  Network 都是零 404；代价落在错误面——每个剪掉的语言一条
  `Uncaught NetworkError: … importScripts … failed to load`，加 Monaco 自己的一条
  `Could not create web worker(s). Falling back to loading web worker code in main thread`
  警告。语言服务确实不可用：同一份必报错的样本（`const x: number = "bad"`、`colour: red`、
  `{"a": ,}`）在 worker 在时 ts/css/json 各出 2 条诊断 marker（另有 css 服务的 colorpicker
  装饰类），剪掉后 6s 内 0 条，而 `getTypeScriptWorker()` 照常 resolve——失败不经 API 暴露，
  只在 console 里可见。取舍即 ADR 0016 决策 5 所记：评审要的是高亮，诊断与补全放弃。
- [x] 镜像装配步同口径（2026-10-15）：Dockerfile 的 Monaco RUN 在 `cp -r min/vs` 之后删同样四个
  worker 并断言「只剩一个 `*.worker-*.js`」（glob 不再命中 = 构建失败，避免哪天 monaco 升级换了
  哈希名又悄悄装回 9 MB）。无 docker build 的核实：在 scratch 用 `/bin/sh` 重放该 RUN 的尾段
  （解 tarball → cp → 新 rm+断言）得 15 700 863 → 6 582 698 B、恰剩 `editor.worker-Be8ye1pW.js`，
  且与仓库本机剪过的 `assets/monaco/vs` `diff -rq` 一致（镜像 dist 与本轮浏览器实测的树同一份）；
  反向对照：glob 零命中时计数 ≠ 1，断言会拦下构建。（本机树剪枝数同：15 700 863 → 6 582 698 B；
  还原 = 从 `monaco-editor-0.55.1.tgz` 重新解出 `package/min/vs/assets/*.worker-*.js` 覆盖回去。）
- [x] `examples/walkthrough.py` 第 12 拍（2026-10-15 落地并实测）：打字机讲代码 → `set patch_code`
  `{code, lang: rust}` → 编辑器入场 → 提示改两笔按 Ctrl/Cmd+S 看 stage 控制台的 `<-` 提交。
  浏览器实测（当时 :3002 服务的是 13:54 的 debug trunk 构建，wasm 8 003 004 B）：编辑器最终都挂上
  了（1124×280、rust 高亮、链路与 `18.editor.yaml` 同序、剪掉的四个 worker 零请求），但**挂载滞后
  于发送端且幅度不稳**——5 轮里 4 轮在发送端停止后 2–4s 只读到 `data-module-mounted="pending"`
  （0 个或 4 个请求），其中一轮在 60s 窗口里停在 `editor.main.js` 不再动，随后一分多钟自己补齐；
  另一轮 2s 就到位。判据因此是"等编辑器真的出现再断言"，不是"发送端结束"（§21 那条教训的第二个
  实例）。未在 release 构建上复测：重建 release 的 dist 会换掉正在看的 dist，留给下次专门一轮。

### 23. append 的落点参数 position（ADR 0005 的追加）— done 2026-10-16

- `content::AppendOp` 加 `position: i64`（`#[serde(default, skip_serializing_if = "is_zero")]`）；
  语义连同解析规则落在 `content::append_index`：`0` = 末尾；正数从尾往内数（`+1` = 最后一个之前，
  `+len` = 头部）；负数从头往内数（`-1` = 头部）；域外拒收，不夹取。方向刻意与"从头部锚定的
  插入"相反——索引一律从本操作自己的锚定端往内数。
- `ui_leptos::ctx` 的 append 臂由 `list.push` 改成按落点 `list.insert`，越界 warn 后丢弃该条目；
  id 唯一性拒收的顺序不变（先查重、再算落点）。
- 门槛：`cargo test -p content` 新增 4 例（落点表、域外拒收、线上字段写与不写、带位置解码）；
  `cargo test -p content -p accrete -p stage` 全绿，`cargo check -p ui_leptos` 绿。
- 起因：reflux 侧收敛操作集——位置插入不单设动作，`insert` 折进 append；需要整段重排时
  `set` 整体替换列表。schema 导出不受影响：它导的是 `accrete::Accrete` 的**节点**形状，
  帧信封（ops）从来不在其中（实测两份导出逐字节同长 40 574 B）。

### 24. 页面级上行把手：入口读 `data-path`／`data-codec`，发布 `window.fluxen.send` — done 2026-10-19

- `ui_leptos::mount()` 从 `#main` 多读两个属性：`data-path`（默认 `/channel`，必须以 `/` 开头，否则 warn 后保留默认）与 `data-codec`（默认 CBOR）；页面 URL 的 `?codec=` 仍然优先（更具体）。起因是宿主可以不是 stage：reflux 的通道叫 `/ws`，且先只走 JSON——而把 `?codec=json` 挂在用户看得见的地址栏上，会让「地址栏 == 路径标记」这条断言自带噪音。
- 挂载后发布 `window.fluxen = { send(event, data) }`：与 canvas 模块拿到的 `host.send` 是同一条腿（都落 `Ctx::send`），`id` 为 `None`（页面级事件没有节点身份），`data` 过一遍 `JSON.stringify` → `serde_json::Value`，不可序列化的值落成 `null`——不发一个假帧，也不静默丢掉调用。
- 连接还没打开时调用不会丢：`emit` 是 future，gloo 的 `Sink::poll_ready` 在 `readyState == 0` 时挂起、`open` 时唤醒（`websocket/futures.rs:287`），所以把手只负责 `spawn_local` 它，自己不排队。
- **装配必须留在 `mount_to` 的闭包里**：`Ctx::new` 的读循环走 `Executor::spawn_local`，把 transport 与 `Ctx` 提到 mount 之前会 panic（实测：页面白屏、`#main` 空着、控制台一行 `spawn_local before a global executor was initialized`）。
- trunk 的产物名带内容哈希（`ui_leptos-<hash>.js` / `…_bg.wasm`），glue 的默认 wasm 路径却写死 `ui_leptos_bg.wasm`——所以宿主页面必须显式传 `init({ module_or_path: "<hashed>.wasm" })`，否则 404。
- 门槛：reflux 侧 `tools/gate_m1_1.py`（探针页 + `App` 伺服 + CDP，5 条）——渲染器挂进 `#main`；`Network.webSocketCreated` 的 URL 是 `ws://…/ws?codec=json`；`fluxen.send("probe", {v: 1})` 到生产者是 `{event, data}` 且无 `id`；`init()` 返回即调的那一帧也到达；零 console error。`cargo check -p ui_leptos` 绿。

### 25. 渲染器发布产物：一份能直接引用的 UI bundle（tag `v*` → release 资产）— done 2026-10-20

`crates/ui_leptos/dist` 一直是本机构建物：谁要它就得 checkout 整个仓、装 trunk、编 wasm——reflux 那侧就是手工把四件拷进 `www/static/vendor/fluxen/` 并自己写一份 `versions.json`。这一节把「产物」变成可引用的东西。

- `scripts/ui_dist.py`：默认跑 `trunk build --release`（`--no-build` 可对已有 dist 装配）→ 挑出四件核心（trunk 的 `ui_leptos-<hash>.js` / `…_bg.wasm`、`assets/main.css`、`assets/custom.css`）→ 写 `manifest.json`（`renderer`／`tag`／`rev`／`entry`／`files{sha256,size}`）、`SHA256SUMS`、`fluxen-ui-<tag>.tar.gz`。tar 条目排序、`uid/gid=0`、`uname/gname` 空、mtime 取 `SOURCE_DATE_EPOCH`（缺省 0），gzip 头同一个值——同一份 dist 装配两次，tarball 逐字节相同。
- `.github/workflows/ui-release.yml`：tag `v*` 或手动派发 → stable rust + `wasm32-unknown-unknown` + trunk **0.21.14**（与开发同版本，钉住）→ 跑脚本 → `softprops/action-gh-release` 上传 tarball／manifest／SHA256SUMS／四件散装。散装是为了「直接引用」：消费者只取自己需要的 URL，不必解包。
- **不装演示媒体**：`assets/3dbrowser` 28 MB、`splatviewer` 48 MB、`monaco` 6.6 MB、`mermaid.min.js` 2.6 MB、`g2chart` 1.1 MB。这些是帧点名才加载的模块媒体，由部署侧自己组装（Dockerfile 就是那份组装）；整个 dist 打进 release 等于每个 tag 挂 ~86 MB，而其中 97% 的字节没有任何消费者请求。
- **一个文件一个名字**：不改 trunk 的哈希名，角色映射放在 `manifest.json` 的 `entry`（`js`／`wasm`）。不做 `fluxen.js` 这类稳定别名——那是同一份字节的第二个名字，还多拷 2.4 MB。
- **manifest 是 fluxen 形状，不是消费者形状**：reflux 的 `versions.json` 键带 `vendor/fluxen/` 前缀，那是它的目录布局，由它自己从本 manifest 派生；渲染器不猜消费者把文件放哪儿。
- 门槛（本地实测 2026-10-20）：脚本跑了两次——一次 `--no-build` 对 `0bf166d` 的既有 dist，一次走 `trunk build --release` 再装配。两轮四件 sha256 **与 reflux 钉的 `versions.json` 逐条相同**（js `7eee5b77…`、wasm `b09fde99…`、`assets/main.css` `201997bc…`、`assets/custom.css` `fd0d78dd…`），`rev` 亦相同；`fluxen-ui-v0.1.0.tar.gz` 723 514 B；`SHA256SUMS` 四行，`sha256sum -c` 可验；同一份 dist 装配两次，tarball 逐字节相同（`227e981d…`）。**界限说清**：那次 `trunk build --release` 里 cargo 复用了缓存（`Finished release profile in 0.18s`），trunk 只重做了分发——所以证明的是「分发幂等 + 装配可复现」，**不是**「冷编译逐字节可复现」（没做 clean build，别当已有证据）。工作流本身本地跑不了（无 `gh`、不自主推 tag），只按 `docker.yml` 的既有写法核对过结构。

### 26. Canvas 的模块选项 + forcegraph 变体（ADR 0017）— done 2026-10-23

用户 2026-10-23：「Canvas 再加一个 3d-force-graph 的变体，示例参考 k10r。边和节点的基础尺寸
之类的做成可配置选项。……这种属于变体的不要放在 accrete 中，在 canvas 的选项中添加一个
Value 动态类型把额外的选项打包传给它引用的 js，js 那边自己处理就可以了」。

- wire 面只加一个可选键：`Canvas { …, options: Option<Value> }`（缺省不出现）。核心零解释。
- 宿主：`mount(el, data, host, options)`，第四参是 JS 对象（wire Value → JSON 文本 → `JSON.parse`）；
  不走 CBOR——配置是一次性静态值，数据面仍是 CBOR。语义与 `url` 同级：挂载期静态，不进 update 腿。
- 新模块 `examples/modules/forcegraph/`（纯 JS，零构建，库在 `mount()` 里才注入）：载荷
  `{nodes, links}` 按 id 求差量、端点先解析、一次 update 一次 `graphData()`+reheat、`d3AlphaMin(0)`；
  尺寸 = 屏幕像素（`nodeSizePx/nodeSizeStepPx/nodeTiers/linkWidthPx/linkWidthStepPx/linkStrongAt`），
  按**本 canvas 高度**折世界单位；`autoFit`（默认开）每批数据后 `zoomToFit` + 尾随补一次取景，
  并用拟合后的相机距离重算像素基准。
- 载体一：`examples/yaml/19.canvas.forcegraph.yaml`（三帧整份快照）；载体二：
  `examples/walkthrough.py` 第 18–20 步（收尾一拍，同一个 canvas 节点 + `options` 袋）。
- 部署：`crates/ui_leptos/assets/forcegraph/`（gitignore）；Dockerfile 拷模块 + 从 unpkg 拉
  3d-force-graph **1.80.1** 并校验 sha256。

门槛（本地实测 2026-10-23）：

- `cargo test --workspace`：75 个用例 0 失败，含新增 `crates/accrete/tests/canvas_options.rs` 三例
  （往返保模块词汇、缺省不上 wire、`/options/...` 深层可 patch）。`cargo check -p accrete --all-features`
  与 `--target wasm32-unknown-unknown` 的 ui_leptos 均绿。
- 库文件的来源与字节：unpkg 的 `3d-force-graph@1.80.1/dist/3d-force-graph.min.js` 1 389 954 B，
  sha256 `13088340aad408156064fd120c3bc3a47c93a3cc45b8f132d6284b20e2223be9`——与 k10r 仓库里那份
  vendor 副本**逐字节相同**（两个路径各 `sha256sum` 过一次）。
- 第四参真的过了界（桩模块把入参抄进 dataset）：`{"nested":{"palette":["a","b"]},"nodeSizePx":3.5}`、
  `data` 13 B、`host.send` 是 function、resize 报 `200x120`——JSON 对象原样到达，键不被改写。
- 真模块（`19.canvas.forcegraph.yaml`，headless chromium + SwiftShader）：宿主 `data-module-mounted="true"`、
  宿主内 `canvas` 1384×460、模块状态 `nodes: 15 / links: 14`、`3d-force-graph.min.js` 在 resource timing 里、
  控制台零 warn 零异常。像素：裁宿主矩形后 `colors=227`、ink 0.44%（升到 4px 档后 0.52%），
  墨迹包围盒 286×272、居中于 1384×460——图在画，纵向基本吃满，横向留白是 3:1 画布的固有比例。
- 像素基准可核对：`pxPerUnit=0.4932`（camDist 1000、画布 460px 高）→ weight 9 的节点
  `nodeVal=1800` = 半径 12.16 世界单位 = 6px 屏幕半径（`nodeSizePx 3 + step 3 × 3 档`），
  即 options 里的 px 承诺在拟合后的画面上成立。
- walkthrough：`python3 examples/walkthrough.py --port 3102 --auto --delay 0.01` 20 步跑完，
  第 18/19/20 步（5 点 4 线 → 10 点 9 线 → 15 点 14 线）均 `delivered to 2 peer(s)`，
  收尾页面里关系图宿主 `canvas-5` 1369×460 `marker=true` 且画布有像素。
- **UMD 分支被页面劫持（本轮实测抓到并改掉）**：3d-force-graph 是 UMD，出口顺序
  CommonJS → AMD → 全局；Monaco 定义了 `define.amd`，于是库被注册成匿名 AMD 模块、
  `window.ForceGraph3D` 一个字节都不落 —— 症状是**编辑器挂载之后**关系图必然
  `mount rejected: window.ForceGraph3D is not a function`，同一页面在没有 Monaco 时
  一切正常（干净页复现不出、跑完 editor 那一步才复发，正因为如此第一版 `<script>`
  载法在演示里时好时坏）。改用 fetch 源码 + blob → `import()`，在遮蔽
  `module/exports/define` 的模块作用域里求值，UMD 只剩"挂全局"那条路；页面自己的
  `define` 全程不动（实测：修好之后同一页面 `define.amd` 仍是 true，宿主
  `canvas-3` `marker=true`、画布 1400×460、ink 3.1%、墨迹包围盒 817×386）。
  代价：CSP 需放行 `blob:`，库地址需可 fetch（同源或带 CORS）。
- **常显名字 + 悬停卡片改版（用户 2026-10-23：「节点和边要显示名字，悬停显示详细信息的菜单不要阴影了，现在显示的很难看」）**：
  节点/边的名字改为 HTML 覆盖层常显（每帧把世界坐标投影成屏幕坐标；标签重叠时后来的
  先藏，"位置一变就回来"），悬停卡片只留详情。卡片的"阴影感"根因是库自带的
  `.float-tooltip-kap` 外壳（bg + padding + radius）里又套了一层带边框的 div，缩小时像
  一层阴影；注入样式把外壳抹平，只剩单层深底卡片（实测 computed style：
  `background rgba(0,0,0,0)`、`box-shadow none`、`padding 0px`、`border-radius 0px`）。
  实测：29 个标签（15 点 + 14 边）全部生成、23 个在视口内显示、重叠对 0（改之前
  7 对）、宿主 1400×460 ink 3.5%、控制台零 warn。
- **悬停卡片改不透明底 + 压到标签层之上（用户 2026-10-24：「悬停菜单作成不透明的，现在
  能看到后面的字」）**：上一轮留下的卡底是 `rgba(15,23,42,.9)`，两个来源同时漏字——
  ① 10% 透明把浅色页面底与深色标签字透上来（实测卡片矩形里纯底色的像素占 0%，整张卡被
  染成 `rgb(39,46,63)`）；② 两层都是宿主里的绝对定位兄弟，标签层后插入、`z-index` 都是
  auto，卡片范围内实测 98 px 标签字（ui_leptos / stage / editor / mounted-by 四个标签）
  画在卡上。改成实色 `#0f172a` + `.float-tooltip-kap` `z-index 2` / `.fg-labels`
  `z-index 1`：同一矩形 85% 是纯 `#0f172a`、标签像素 0。

### 未修：模块实例在宿主重建/换布局后不进 unmount（2026-10-23 实测）

同一次 walkthrough 收尾读到的 `window.__fg`（临时调试钩子，未入源码）里有两个存活实例（各 15 点），
页面只有一个关系图宿主——一个成了孤儿。成因在宿主既有路径：`data-module-mounted` 标了 `true` 但
容器被原地重建抹空时，`canvas.rs` 清标记重挂，**没有先 `unmount` 旧实例**；换布局根（`create`）
同样没有对旧宿主补一次 `unmount`。对 WebGL 模块这不是无害残留（每实例一份 GL 上下文 + rAF）。
本节的改动不修它（属 canvas 宿主的生命周期问题，与 options 无关），记在这里等下一单元。

## Resolved

### trunk 0.22 拒绝了 copy-dir 的绝对 href —— `x start` 在装配阶段就死（fixed 2026-10-23）

`crates/ui_leptos/index.html` 里 `<link data-trunk rel="copy-dir" href="/assets" />` 是 trunk **0.21.14**
（Dockerfile 与 `.github/workflows/ui-release.yml` 钉的版本）吃得下的写法。本机的 trunk 是 0.22.0
（nixpkgs），它把 `href` 当文件系统路径去 canonicalize，于是 `x start` 在装配阶段报
`error taking canonical path of directory "/assets": No such file or directory`，wasm 都编完了却拿不到 dist。

改成相对路径 `href="assets"`（trunk 文档里的形态，0.21/0.22 都认），产物不变：`dist/` 仍是哈希命名的
`ui_leptos-<hash>.js` / `…_bg.wasm` + `dist/assets/**`。**没有**顺手把 pin 提到 0.22——`ui-release.yml`
出的 dist 是消费者按 sha256 钉住的东西（reflux 的 `versions.json`），换 trunk 版本等于换掉那四件的字节。

### A text node's second `.class()` call wiped the first — markdown body text had no typography rule (fixed 2026-10-12)

User report: 「文本没有行距」. Two layers of cause, both long-standing (identical in the
dioxus-era `ui_dixous` text widget and in the main.css carried over from fluxora):

- **Name mismatch.** `text_` emitted the class `text` while the element rule was `.txt`,
  so a text node matched no rule at all and inherited `body { line-height: 1 }`
  (main.css:67) — CJK prose sat at line box == font size. The emitted class is now `txt`.
- **The class list never landed on markdown nodes at all.** `div().class(base_css).class("markdown")`
  makes two `.class()` calls on one element and the second REPLACES the first: measured,
  every `.markdown` element's `className` was exactly `markdown` — `txt`/`col`, every
  `attrs.class` the producer wrote and the node's own id were all gone (which is also why
  `.--header` in custom.css had never matched anything). markdown now joins the same
  `css.join(" ")` and the element gets a single `.class()` call.

Measured after the fix (fresh `trunk build --release`, then stage + headless chromium;
the page's wasm hash was checked in `performance.getEntriesByType('resource')` so the
verdict is not about a stale build): `.txt` nodes 8 → 12, with `txt col markdown` and
`txt --header nogrow col markdown`; the `<p>` inside `.markdown` and `.txt` itself compute
line-height 30px at font-size 20px (was 20px); the menu tabs (text + `box`) also move
20px → 30px, i.e. single-line chrome built from text nodes grows 10px; `.txt accent`
computes `oklch(0.7 0.12 182.49)` (the style-layer `.accent.txt` combos are live again);
0 console errors, 0 leftover `.text`. Paragraph spacing (`p { margin: 0.2rem }`,
main.css:741) is a separate knob and was left alone. Only `text.rs` had the double-call
flaw — `fold.rs`, `case.rs` make their two calls on different elements/branches.

### The image build's glTF fetch died on `curl: (23)` — a gitignored directory is absent in CI (fixed 2026-10-12)

Builder step 10/15 failed with `curl: (23) client returned ERROR on write of 16375 bytes`.
Error 23 is a WRITE error, not a network one: curl opens `-o`'s file when the first body
byte arrives, so a successful connection plus a write error means the output path cannot
be opened. Measured with the same Khronos URL through the proxy: into an existing
directory 7,833,592 B / exit 0; into a NONEXISTENT directory the identical
`curl: (23) … on write of 16375 bytes` / exit 23; direct (no proxy)
`curl: (35) Recv failure: Connection reset by peer` — a different signature, not this bug.

Root cause: `crates/ui_leptos/assets/3dbrowser/` is gitignored as a WHOLE directory
(.gitignore:10), so a clean checkout — `actions/checkout` in .github/workflows/docker.yml,
whose buildx output matches the reported log shape — contains no such directory and
`COPY crates ./crates` cannot create one. The other fetched inputs are covered:
`assets/splatviewer/` carries tracked files and the g2chart RUN already had `mkdir -p`.
Fix: `mkdir -p crates/ui_leptos/assets/3dbrowser` before the first curl in that RUN (it
also gives the later `cp pkg/… assets/3dbrowser/` a landing directory).

Verified without a container daemon: `git archive HEAD | tar -x` IS the CI context — the
directory is absent, the old snippet reproduces exit 23, and with `mkdir -p` prepended both
glbs download and both `sha256sum -c` lines pass. The full image build was NOT run (docker
commands hit the session's approval gate); the steps after this one are unverified.

### The rack's auto-follow only fired on a list write — a row that grows internally never followed (fixed 2026-10-12)

User report: 「walkthrough 中，没有自动滚动」. Root cause: the injected follow JS lives
inside the rack's outer dynamic closure, whose only tracked read is the LIST slot, so
"jump to the newest" ran exactly once per row append. The walkthrough is one row for the
whole demo — after that row lands, every later change (typewriter patches, chart data,
module payloads, a row-inner rack append) writes a DATA slot, never the list slot, so
nothing followed; the row then grew from 650 px to 2168 px while `scrollTop` stayed at the
value the append had pinned. §17's own e2e never saw it because that probe appended row
after row — the one path that did trigger the follow.

Fixed in `rack.rs`'s injected JS, three parts. (a) Trigger surface: a `ResizeObserver` on
the container plus its element children (a `childList` `MutationObserver` re-observes new
children) and a subtree `MutationObserver` (`childList+subtree+characterData+attributes`),
so growth anywhere inside a row triggers the same pin. (b) The "only while at the bottom"
flag is still maintained by the scroll listener, but now RECOGNIZES OUR OWN scrolls —
landing at the bottom, or back at the `scrollTop` we last pinned, counts as following;
only a position clearly away from both is a user scroll-up. Live-value recomputation is
wrong here because growth is dispatched AFTER the scroll event, which misreads a follower
as a scroller (measured: the view froze mid-stream on the first attempt). (c) The catch-up
is a 40 ms `setTimeout` loop that stops after three stable-height ticks (flag "0" or a 2 s
cap also stop it): `requestAnimationFrame` was measured starved to the second scale on this
box's headless chromium (swiftshader + 7.9 MB wasm + six module hosts), so an rAF-driven
chase stuck at `chasing = true` with no tick running and lagged 480 px.

Measured (`walkthrough.py --auto --delay 0.01`, fresh stage + fresh headless chromium):
`top == max` at every sample through the whole run (0/0 → … → 1054/1054, terminal
1578/1578); scroll-up → append leaves `top` at 0; back to the bottom → append follows
(1718); a data-slot write that grows the row without touching the list slot follows
(max 1718 → 4554). 0 console errors; `cargo test --workspace` green (30 groups). Probes
kept as skill scripts `e2e_autoscroll.py` / `e2e_scroll_flags.py`.

### Module update leg was dead — G2 v5 has no `chart.update()` (fixed 2026-10-02)

`assets/g2chart/index.js` called `chart.update()` on every data change, but the
pinned G2 v5 build has no such method: measured on the chart instance,
`typeof chart.update === "undefined"` (the official `esm/api/runtime.js` shows
`Runtime` with `render`/`options`/`changeSize`/`forceFit` and no `update`;
`changeData` comes from the composition/mark base and does exist). The call
threw `TypeError: v.chart.update is not a function`, the host logged
`module host <id>: update threw: …`, and nothing repainted — a source-bound
chart held 1 instance / 1 render before and after a data patch, so ADR 0009's
"`chart.options(spec); chart.update()` 增量重绘" premise was false.

Fixed to `chart.options(spec)` + `return chart.render()`. Measured G2 semantics
behind that choice: `options()` alone paints nothing (render count 0); a
re-render makes the spec tree a diff (existing marks/compositions are reused by
key, dropped ones removed) and the canvas is repainted by G2's canvas plugin,
which disables the engine's own dirty-check/dirty-rect and keeps its own
dirty-object region repaint (`dirtyObjectNumThreshold` 500 /
`dirtyObjectRatioThreshold` 0.8, full-screen clear above). Incrementality only
holds while the instance lives — a re-render with an unchanged spec still
repaints (731 2D calls measured), and no scene element object survives a data
change (0/3), so destroy + rebuild throws the whole benefit away.

### A write into one row node rebuilt the row and remounted every module host (fixed for streamed values 2026-10-02)

Patching an inline-bound value inside a list row (the walkthrough's per-day and
per-token appends) rewrote the row's Accrete value, so the row's render closure
re-ran and every module host in that row was torn down and re-mounted: measured
4 G2 instances → 8 after a day append → 12 after a summary text append, 0 of the
4 tagged canvases (or their module-created hosts) surviving, while the row's
plain text nodes kept DOM identity (7/7 `.box` tags alive). Per-key slots and
the rack's per-row `Memo` isolate other keys and other rows; nothing isolates
siblings inside one row, and a remount renders each chart twice (mount +
ResizeObserver changeSize).

Streamed values no longer live in the layout plane: the radar example's line
chart and summary text bind `kind: source` (`trend` / `summary`) and the
walkthrough patches those slots. Measured after the change: a day append
re-renders only the line chart in place (instance count and canvas identity
unchanged, 0 destroys, the three radars untouched), a summary token re-renders
no chart at all, and the demo grows to 90 data rows (30 days × 3 departments).
The general case — any layout-plane write still remounts module hosts — is not
fixed, and the container-level memo §14 proposed for it was later measured
unreachable in this rendering stack (a `Memo` in the parent render scope is
vacuous, a synchronously built `keyed()` panics, and `keyed()` under an outer
dynamic closure tears down on every layout write — see §14), so keeping streamed
values out of the layout plane stays the only working mechanism.

### Emitted events carried no node id — the wrapper's `id` was always absent (fixed 2026-10-02)

ADR 0010 defines the wrapper's `id` as the emitting node's id, but every emit
site passed `None`: `use_target` (select, textarea), `input_`'s Enter path and
`form_`'s submit path. Only the canvas host uplink passed one. So a producer
could not tell which node fired, and — since the vals plane carries the same
wrapper — a slot subscriber could not tell which component wrote a value. All
three sites now pass the node's own id via `node_id` (an empty-string id is
normalized to absent, the rule row identity already uses).

Verified on the default CBOR page: a probe layout with an id-bearing input, an
id-less one and an empty-id one, Enter in each — the stage console echo shows
`id: probe-input` on the first uplink and no `id` field on the other two.
Unit tests: `node_id` in hooks.rs (own id / absent / empty).

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
whole-tree vDOM rebuild hid this. `textarea_` carried the same shape until
the ADR 0008 upgrade (2026-10-01): Enter now routes through the unified
`Ctx::emit` (Event uplinks, Local writes the value plane), clears with
signal-set + imperative `set_value("")` before awaiting, and skips empty
values / bind-less edits. It also exposed a second defect class fixed
same commit: emitter components read their initial value via
`use_source_untracked` — a tracked read inside the parent layout render
closure subscribes the parent to the very slot the emitter writes, so
every emit rebuilt the subtree and "restored" the sent content (DOM
identity probe: textarea node replaced, value back). Pure readers
(text/placeholder subscribers) keep tracked reads. e2e: Local path stays
cleared across two sends with subscriber updating draft-one→draft-two,
zero uplinks; Event path regression `<- note` intact.

### Notification fan-out (per-key slots) — done 2026-09

`data` / `list` were single whole-map signals: any `Set`/`Join` frame notified
every bound widget across all keys. Now the outer maps store lazily-created
per-key slots (`DataSlot` / `ListSlot`) and values publish through the inner
signal, so a write notifies only that key's subscribers. Row-level isolation
within one key comes from rack's per-row Owner + `Memo<Accrete>` (untouched rows
recompute to an equal value and never re-render).

Residual tail, not a defect: a frame touching key K still runs every row's
Memo for K's racks — O(rows) cheap recomputes (Arc clone), no DOM work. The
row lookup itself is a table: `rack_` builds one
`Memo<Arc<HashMap<key, position>>>` per list value, outside the outer dynamic
closure, and every row Memo resolves its row by key — O(rows) per write
instead of O(rows²); the key convention is shared with the keyed key set
(`row_key`), so id rows and `#{idx}` positional rows take the same path.

## Conventions (not code)

### Streaming rows should carry `id`

Rack keys rows by `Accrete::id`, falling back to position (`#{idx}`) for
id-less rows. Id-less rows render correctly but lose DOM identity whenever a
row is inserted before them (keys shift). Producers that stream (chat, logs)
must set `id`; composite identity is the producer's job (e.g.
`id "alice:m42"`) — the renderer deliberately has no key-template config,
since the Join merge contract (`cmp_id`) is pinned to `id` and a second key
axis would split row identity.

### Navigation (and any homogeneous group) goes through data; `case` is static skeleton only

A group of elements that differ one by one — a menu, a channel list, a page
dictionary — is data in this model: it lives in a list slot and is rendered by
a consuming component. `select` reads the `tabs` slot as its menu, `pages`
reads the `pages` slot as its page dictionary, `rack` reads any slot as a row
list (row identity = the data `id`, per-item differences come from the row's
`selector` picking an `item` template). `case` is left with static skeleton:
grouping and layout with no data source, no addressing, no streaming. The main
skeleton (00.main.yaml) is the shape in mind — menu rows and page rows are two
slots, not hand-written containers.

The judgement is where the multiplicity comes from, not whether per-element
styling is wanted: both primitives do per-element styling, `case` through each
child's `attrs.class` and `rack` through the row's `selector`. Hand-written
children pay off only when the structure is genuinely fixed — one place, never
added to or removed from, never patched or removed by address. A `case` child
is reachable only by its positional `/children/i` layout path, which dies when
the tree shifts; a rack row has keyed identity, can be streamed into place, and
can be addressed by `id`.

Neither primitive is redundant. `rack` needs a list slot (with no source it
renders an empty div) and its `item` templates are themselves `case`-shaped
nodes, so `case` is the substrate rack renders through. The mechanism gap is
not constant-sized — slot subscription, per-row `Owner` + `Memo` and template
selection versus rendering children directly — so merging them would only grow
two unrelated code paths inside one component.

### Streamed values live in a data slot, never in the layout plane

Token-level content (a growing summary, a per-day data point, chat tokens) is a
value that changes on its own schedule, so it belongs in a data slot with the
widget bound `kind: source`; the producer then patches the slot
(`{event: <slot>, path: …}`) and only that slot's subscriber re-renders.
Patching the layout node instead (`…/bind/value/default`) rewrites the container
node, so the container's render closure re-runs and every module host inside it
is destroyed and re-mounted — measured: one sibling text append re-created all
four charts of the row (4 → 8 → 12 G2 instances, canvases replaced) while the
row's own text nodes kept DOM identity. Two further reasons the slot is the
right home: a layout path is positional and dies when the tree shifts, while a
slot is addressed by name; and a slot-bound widget owns a subscription
independent of its parent, so it survives a parent re-render. The obvious
alternative — pruning container children so a layout write skips untouched
subtrees — was measured unreachable in this rendering stack (§14 实测否证), so
this is not a "cheaper later" choice but the only working mechanism.

### The graph's renderer is a scene layer, and its edges live in the scene — done 2026-10-26 (ADR 0018)

`frame::build` is now the seam between layout and graphics: a pure function
(camera + viewport + dpr + scene + style) that returns the viewer's matrices and
two instance batches. Five host tests pin it — one instance per node and per
link, node size converted to world units at its own depth, link width a world
length that still projects to the requested pixel width, focus dimming, and the
CSS-versus-device viewport split. The backend uploads those numbers and issues
two instanced draw calls; nothing decides anything there.

Edges stopped being clip-space quads. Each link is the unit quad mapped through
an affine matrix onto its world-space parallelogram: constant on-screen width,
real depth, occluded by nodes, and `u` along the quad is the link's own length
parameter — which is where flow, vines and trunks will attach. The old
hand-written WebGL2 renderer is deleted rather than kept as a second backend.

Cost, measured on the real component and not on a toy: wasm 2,348,320 →
2,739,031 B (+16.6%), gzip 709,984 → 851,259 B (+19.9%). The earlier estimate of
+41 KB gzip was three times optimistic because a real component also pulls the
PBR material, lighting, render targets and effect shaders.

One bug found by the migration, worth keeping: the collision radius used to be
the drawn pixel radius converted to world units through the *current* camera. With
a standing auto-fit that closes a loop — the cloud grows, the camera pulls back,
the world radius grows, the cloud grows — and it ran to 175k world units in
seconds. Collision is now derived from the layout's own scale, so the viewport
cannot change the shape of the graph.

Verified headless (34 nodes / 44 links, 220×473 viewport, SwiftShader): stable at
`d=565 r=204`, real-edge midpoints painted 72/80 against 32/80 for non-edges, 6888
coloured pixels with the DOM overlay hidden.

### 外部集成：ui-core / boot 拆分（ADR 0021）— 待做 2026-10-29

两种消费方式已经同时存在：reflux（Python 宿主 + 钉住的 bundle + `window.fluxen.send`）与 prism
（Rust 会场，前端装进自己的二进制）。现在的 `ui_leptos::mount()` 把 `#main` 解析、`transport-ws`
与页面级上行把手写死在渲染核心旁边，于是"核心 + 自己的 boot"这条路走不通，`fluxen` 门面也只做了
一个 feature re-export。适配方向此前被写反过：要求 fluxen 认宿主（prism）的 `{ev,args}` 帧——已否，
见 `docs/decisions/0021-external-integration.md`。

工作：

1. **拆 crate 并归位**：`ui_leptos` → 核心（`ctx` / `render` / `components` / `widgets`，不依赖
   `transport-ws`、不认识 `#main`，留 `crates/`）与 boot（`#main` 解析、`mount()`、
   `window.fluxen.send`、`main.rs`、`index.html` + trunk 配置）。boot 搬 `examples/`，
   `transport-ws` 一并搬（92 行，唯一消费者就是 boot）——判据见 ADR 0021「归属」节：`crates/` 只放
   外部按名字依赖的东西，`stage` 作为调试工具例外留 `crates/`。`stage` 的 `--dist` 默认值随 boot 的
   新路径改。核心与宏的最终包名（`ui-core` / `ui_leptos`，宏是否跟着改名）在动手时定，文档里先用语义名。
2. **门面归位**：`fluxen` 总是重导出 `ui-core`；`boot` 走 feature（默认不拉 `transport-ws`）。
3. **两个消费者对齐**：reflux 依赖 `boot`（它的 bundle 就是一次 boot 构建）；prism 依赖 `ui-core` +
   自己的 transport 与 boot。任何一方都不新增长在核心里的适配层。
4. **发布产物按挂载点构建**：reflux 用根相对 public URL，prism 用 `--public-url /assets/`；宿主
   不改写产物链接。
5. **验收**：`cargo run -p stage -- serve` 仍走原生 `Message`（dev 回归不破）；reflux 的
   `python3 -m reflux.demo_app` 在拆后照常运行；`ui-core` 的构建里没有 `transport-ws`。

说明见 `docs/external-integration.md`（en）与 `.zh-CN.md`（zh）。

### 外部集成：ui-core / boot 拆分 —— done 2026-10-29

ADR 0021 的实施。落地后的形状：

- **`crates/ui_core`**（包名 `ui-core`）：`ctx` / `dom` / `hooks` / `render` / `components` /
  `widgets`。不认识 `#main`，不依赖任何具体 transport（只认 `transport::Transport` 这个接缝）。
- **`examples/renderer`**（包名仍是 `ui_leptos`）：boot——`mount()`（读 `#main`、建 WS、挂响应式根）、
  `window.fluxen.send`、`main.rs`、`index.html`、`Trunk.toml`、`assets/`、`modules/`。
- **`examples/transport-ws`**：WS 实现，92 行，唯一消费者是 boot。
- **`crates/` 剩下的都是"外部按名字依赖"的东西**：`accrete`、`accrete_macro`、`content`、
  `transport`（trait）、`ui_core`、`ui_leptos_macro`、`forcegraph`、`ui-assets`、`fluxen`（门面，
  现在只重导出 `ui-core`，feature 已不需要）；`stage` 例外留 `crates/`（调试工具）。

包名 `ui_leptos` 故意没改：产物文件名是 `ui_leptos-<hash>.js/_bg.wasm`，reflux 在
`www/static/vendor/fluxen/` 里钉住的就是这两个名字——改名是一次跨仓 re-pin，不是路径编辑。

搬动的连带修正：`assets/{main,custom}.css` 原来是符号链接（`../../ui-assets/assets/...`），随目录
下移后指向失效，已改为 `../../../crates/ui-assets/assets/...`；`Dockerfile`、`x.nu`、
`scripts/ui_dist.py`、`stage` 的 `--dist` 默认值、两份 README、示例 yaml 与模块头注释里的路径全部
跟上。构建脚本里的 `stage` 装配命令随之变成 `cargo run -p stage -- serve --dist examples/renderer/dist`。

验证：`cargo check --workspace --all-targets` 与 `cargo test --workspace` 绿；
`examples/renderer` 的 `trunk build`（dev 与 `--release`）成功；`scripts/ui_dist.py` 装配出四件 + manifest。

工具事实（顺带撞上的）：本机 trunk 是 0.22.0，产物命名不再让 js 与 wasm 共用 stem（0.21.14 共用），
`ui_dist.py` 的 `entry_names` 已改成分辨各取一个，两种命名都吃。Docker 仍钉 0.21.14。

### 外部集成：信封归宿主（ADR 0021 决策 6）— 待做 2026-10-29

决定：`content::Message{ev, sender, created?, content}` 是宿主包装，不是线上模型——移进 `stage`；
prism 与 reflux 各自重新处理自己的信封；渲染器只认动作列表。落地之前下行仍是 `Message`，两份集成
文档已按「现状 + 决定」两段写明。

清单（按依赖顺序）：

1. `content`：`Message` 与 `EV_DRAW` 离开本 crate，`From<(Session, Content<T>)>` 一并走；留下
   `Content<T>` 与各 op、`Outflow`、`append_index`。`tests/codec.rs`、`tests/append_position.rs` 里
   用 `Message` 的断言改为直接测动作列表。
2. `stage`：`Message` 落到 `stage::proto`（构造点在 `proto.rs:48`，解析与转发在 `mirror.rs`）。
   验收：`stage serve` + `examples/yaml/00.main.yaml` 全链（布局、流式、上行）不回归。
3. `ui-core`：`Ctx` 的下行解码目标从 `Message<Accrete>` 改成动作列表；`dispatch_msg` 去掉信封参数，
   `ev` 闸删除（连同那条 `frame ev = …` debug 日志）；队列消费循环逐帧解出列表后直接应用。
4. `examples/renderer`：boot 只搬字节，不改代码；跑一遍确认。
5. 示例 yaml：不改——`examples/yaml/*` 的顶层一直是动作本身，信封只存在于传输层。
6. prism（跨仓）：渲染腿只发动作列表；`{ev,args}` 动词帧留在它自己的程序腿上，页面侧若要读动词，
   由 prism 自己的 boot 承担，不经渲染器。
7. reflux（跨仓）：`wire.py` 继续构自己的信封（它的业务形状），发给渲染器的那一层只留动作列表
   ——转换在它自己的宿主侧完成；`reflux.js` 若依赖 `ev` 需同步。

验收：`cargo test --workspace` 绿；本仓 `grep -r "content::Message"` 只剩 `stage`；ADR 0021 与两份
集成文档里「落地前」的措辞同时撤掉。
