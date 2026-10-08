# ADR 0011: 远程 Vars 注入——vals 平面的受控写入口

状态：已接受（2026-10-02 实施完毕，三门槛 + e2e 通过）
日期：2026-10-02
仓库：fluxen

## 背景

`vals`（local 平面，裸 Value 槽）自 ADR 0008 分出来之后只有一个写者：浏览器内的 `Ctx::emit` Local 分支（ctx.rs）。layout / data / list 三个平面都有远程写入口（create / set / append / remove / patch 按平面寻址），vals 没有。

两个真实需求把它逼出来了：

- 强制视图状态：聊天应用里发送了重要消息，希望用户立刻看到——生产端把用户所在频道（`page` 槽）切过去。本质是"以用户的名义合成一次交互"，用户明确判定这种模式不鼓励。
- 页面状态快照/恢复：val 槽装的是页面内部状态（当前页、过滤器等），导出再回灌是"整页保存恢复"的必要一半（另一半是渲染器私有的 DOM 状态，如容器滚动）。

ADR 0008 §5 把"双向同步"列为独立后置单元，理由是"需要 origin 标记或同值短路防写读环"。本 ADR 把那条路走完，但不引入 origin 标记：写读环用同值短路正面挡。

## 决策

### 1. 新 Content 变体 Inject（wire `action: inject`）

```rust
pub struct InjectOp {
    pub slot: String,                 // vals 平面键（= 频道名）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,        // 包装对象的事件名，缺省 = slot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub data: Value,                  // 载荷（裸 JSON）
}
```

- 载荷就是 ADR 0010 的 `Outflow {event, id?, data}`（content/src/lib.rs:52），逐字段落在 op 上，不套 `value` 外壳：`slot` 选键，其余三字段就是包装对象的字段。订阅方（`kind: local, slot, path`）看到的形状与浏览器自己 emit 写入的完全一致——"这次切换是谁写的"在订阅方不可分辨，这正是要的性质。
- `event` 缺省 = slot 名，与 `BindVariant::Local` 的缺省一致（槽即频道）。
- 该变体不泛型：`Content<T>` 的其它变体携带 T，而 vals 平面的货币是裸 `serde_json::Value`（ADR 0008：事件数据不是展示形态），所以 `InjectOp.data` 是 `Value` 而非 T。两个平面的差异第一次落在类型上。
- 不新增平面、不新增寻址语法、不新增开关：一个操作、dispatch 一个分支，写 `slot_for_value(slot)`（ctx.rs:196）。

### 2. 写语义：整值覆盖 + 同值短路

- 写必整值（与 emit 一致）：载荷即槽的新值，不做指针级增量——那是 data 平面的语义。
- 同值短路：槽当前值与来值相等时跳过写入（不 set、不通知）。这是写读环的正面解法——生产端把收到的值回显回来时，环在第一跳终止。与 ADR 0008 §4"值槽是电平语义"一致（同值两次写订阅方看到的值不变），区别是这次连通知也不发。

### 3. 可观测性（不鼓励的模式必须显形）

- 每次 inject 打一条日志（槽名 + 值）：状态变更必须可追溯。
- 操作名 `inject` 自带成本信号：帧流与 stage console 里一眼可辨"这是强制注入"，不需要 flag、开关或权限位——选词优于加约束。
- 文档口径：vals 的常态写者是浏览器；远程写只用于恢复与强制视图状态，数据展示一律走 data 平面。

### 4. wire 面同步（新增操作的固定清单）

- content crate：`Content` 闭词汇 +1 变体，唯一改动点；`Message<T>` 与 decode 自动跟。
- accrete 与 `stage schema` 零改动：载荷是裸 Value，不进 Accrete 词汇表。
- YAML/JSON 载体自动可用（`Content` 是 `action` 内部标记的自洽枚举，stage 的 `parse_yaml_to_frame` 不需要特判）。
- 老 UI 收到 `action: inject` 会 decode 失败并 loud 报错（ADR 0003 fail-loud），不静默丢帧。

### 5. 示例与验证

- 示例 `examples/yaml/00.inject.yaml`：在 00.main.yaml 骨架下发一帧 inject，把 main 布局的当前页切到 home（骨架本地无人写 `page` 槽，注入前按"信号未写入 = 空白"回落，注入后首页出现）。
- 单测 `crates/content/tests/inject_op.rs`：只带 slot+data 的解码、显式 event/id 透传、缺省 event = slot、缺 event/id 时 wire 上省略、结构型载荷不被重塑、以及"不携带 T"（`Content<u8>` 也能装）。
- 单测 `crates/ui_leptos/src/ctx.rs`（同值短路的纯函数 `inject_writes`）：空槽写、同值不写、异值写。真写路径要反应式运行时，故抽纯函数测判定。
- e2e（stage 直驱，headless chromium 读 DOM）：00.main.yaml 后 pages 空白（`page` 槽未写，符合"信号未写入 = 空白"）；00.inject.yaml 一帧后 visible = home；同值帧重发无变化（短路，DOM 看不出"没通知"，那部分由单测覆盖）；改值 inject `page=about` → 切到 about（textarea 行，innerText 为空，以容器 class 判定）；未命中的键 → 回落空白。
- 三门槛：workspace build/test、wasm（trunk build）全过。

## Why Not（排除的通道形态）

- 与 data 合并、统一更新：两个平面的货币（Accrete 展示节点 / 裸 Value）与写语义（指针增量 / 整值覆盖）结构性不同，合并只有两条路——让生产端在外部包一层 Accrete 信封（形状不再由事件定义，正是 ADR 0008/0010 拒绝的做法），或把 data 平面放宽成 Value（"这个值是展示节点还是裸通道"变成每个消费方都要分支的形状歧义，且快照再也无法判断哪些值该恢复）。用户 2026-10-02 的判据同此：合并后"没法区分事件还是视图"。
- 只做 dump/restore 一对专用操作（不做通用写）：强制视图状态是真实第二场景，且与 restore 同形（都是往 vals 灌一份包装对象），分成两条只会多一套语法。用户裁决是走专用通道，但通道本身是一种成本，故不加宽。
- 另开 forced 子通道（组件显式 opt-in 才接受远程写）：订阅方就要区分"谁写的"，形状一致性丢失；"不鼓励"变成逐组件重复声明，成本更高。
- 加 origin / epoch 标记：同值短路已经挡住最常见的回显环，交替值环（A→B→A）不是机制能解的，标记只是新增一个 wire 字段而把责任挪个位置。

## 明确不做

- 地址栏/历史同步（popstate 等环境原点）：仍属独立单元（ADR 0008 §5）。
- list 槽的远程本地写（本地 append/remove）：等真实需求；语义同族，到时按同一纪律扩，不预先加。
- 快照/恢复本体（dump 载荷形状、滚动的身份锚点与读写方、可恢复 vs 瞬态的判据）：下一个单元，骑同一条通道；本 ADR 只定通道。
- stage 侧 CLI 子命令：stage 只做 YAML → 帧的转发，注入是生产端的事。

## 后果

- 新增操作 = `dispatch_msg` 一个分支，写入 `slot_for_value(slot)`；三个既有平面与订阅侧零改动。
- 强制视图状态从此在帧流里可见、可审计；数据展示仍走 data 平面，两个平面的边界不因这次扩展而模糊。
- 快照/恢复的地基就位：恢复 = 若干 inject（+ 滚动回填），导出 = 一条上行载荷；后者需要渲染器读 DOM 私有状态，属下一个单元。
- 交替值环是已知残余风险，靠文档标注与生产端自律，不靠机制。