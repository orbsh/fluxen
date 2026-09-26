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

## Resolved

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
