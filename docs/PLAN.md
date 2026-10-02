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
  流式更新 = 指针 patch 进 `/data` + update 重绘（ADR 0005 零新机制）。
- 去 eval：薄封装 ES module（mount/update/resize/unmount 契约，内部
  动态 import g2、CBOR→spec 解码），chart.rs 复用 ADR 0007 的
  import(url) 通道；`Chart { url }` 字段 serde default 向后兼容。
- 资产：g2.min.js（321KB gz）进 assets/ 照 3dbrowser 的 gitignore+
  重建配方模式；ApexCharts 资产与 index.html script 标签退役。
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
  ApexCharts 资产/index.html 标签/trunk.toml 代理/08.apexchart.yaml
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
- 新增 16.chat_in_pages.yaml：Pages 结构为基础 main 布局，00.chat_layout
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
- 示例 `examples/yaml/16.inject.yaml`（骨架下 inject `page` = home，把 main
  的当前页切过去）；单测（缺省 event = slot、id 透传、同值短路、载荷形状与
  emit 逐字段一致）；e2e（inject `page` → pages 切到目标行；随后点一次同名
  交互，槽值不变）；三门槛 + e2e。
- 实施（2026-10-02）：content crate 加 `Inject(InjectOp)` 与
  `InjectOp::outflow()`（"缺省 event = slot"因此进了 content，可单测）；
  ctx.rs 的 `dispatch_msg` 加 inject 分支——写 `slot_for_value(slot)`，
  同值短路抽成纯函数 `inject_writes`；info 日志经浏览器 tracing_wasm
  （main.rs 装的是 INFO 级）可见。
- 验证：workspace build/test 全过（content 新增 6 条、ui_leptos 新增 3 条）；
  trunk build 出 dist；`stage tojson 16.inject.yaml` 确认 YAML 载体认识新操作
  （event/id 缺省在 wire 上省略）；e2e 实测 pages 受控切换——注入前空白、
  一帧 inject `page` = home 后首页出现、同值重发无变化、改值切到 about、
  未命中的键回落空白。

快照/恢复（暂缓：用户 2026-10-02 决定先不做，方案留档；骑同一条通道，尚未定案）：

- 导出（dump）= 一条上行载荷，装 layout / data / list / vals 四个平面 +
  可滚动容器的 scrollTop；回灌（restore）= 若干 inject + 滚动回填。
- 滚动的锚点复用现有寻址元组 `(event, id?, path)`（rack 某行内的容器 =
  `{event: "chat", id: "scoreboard", path: "/children/1"}`），不引入第二种
  路径语法；滚动是渲染器私有的 DOM 状态，读写与回填都归渲染器。
- 未定：快照的判定标准——收 Vars 全量还是只收可恢复的那部分（决定要不要
  给槽或值加持久性判据）；dump 的载荷形状；恢复的时机与幂等要求。落地前
  单独写 ADR。

## Resolved

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
