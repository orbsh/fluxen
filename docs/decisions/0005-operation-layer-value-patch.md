# ADR 0005: 操作层重构——Value 与 Patch 两种数据形态

状态：已接受
日期：2026-09-29
仓库：fluxen

## 背景

下行 `Content` 现有四个动作（create/tmpl/set/join），更新机制依赖 join +
`Method`（Replace/Concat/Delete）的**位置合并**：`merge.rs` 用 `zip_longest`
按索引逐层配对 children，bind default 按策略合并。复盘发现三个问题：

1. **粒度太粗**。要改深层一个值，帧必须携带从根到目标的完整前缀结构
   （zip 需要逐层占位）；token 流每帧重发整段前缀，O(树) 带宽。
2. **位置寻址脆弱**。children 靠索引配对，布局一旦增删兄弟节点，旧帧
   全部错位——结构性缺陷，不是实现问题。
3. **合并覆盖不全**。`merge` 只处理 bind default 与 children，**attrs 完全
   不参与合并**：改一个 class 只能整棵重发。

同时数据平面的两条寻址通道已被验证良好：Set 的事件名（具名槽整体替换）、
join 的行 id（同 id 合并进同一行）。粗的只有"树内部定位"这一段。

fluxora 生产端已存档，本协议无外部消费方，允许一次性切断，不设兼容层。

## 决策

数据分两种形态，动作按形态重划为五个：

```rust
#[serde(tag = "action")]
enum Content<T> {
    Create(T),                      // 整体替换 layout 根
    Set(Influx<T>),                 // 整体替换具名槽（清空=替换为空值）
    Append { event, data },         // 向列表槽追加行（原 join，去掉合并职能）
    Remove { event, id },           // 从列表槽删行
    Patch(PatchOp),                 // 按路径定点更新
    Empty,
}

struct PatchOp {
    event: String,                  // 槽名；"" = layout 根
    id: Option<String>,             // 列表行的锚点
    path: String,                   // JSON Pointer (RFC 6901)，相对锚点
    op: PatchKind,                  // Replace | Append
    value: serde_json::Value,
}
```

- `PatchKind::Replace` 替换指针命中的值；`Append` 追加（字符串拼接 / 数组
  push）——承接 Concat 的 token 流职能，但作用在显式路径上，不再递归全树。
- 平面由 `(event, id)` 判定：`id` 存在 → 列表行锚点；`event == ""` → layout
  根；其余 → 具名数据槽。
- `Method` 字段与 `merge.rs` 的位置合并随 join 的合并职能一并删除。
- 路径未命中：`tracing::warn` 后丢弃该条目（帧可能先于其 create 到达，
  丢弃是合法降级，日志保证可观测）。
- Append 的 id 唯一性防御：列表槽内已存在同 id 行时拒收该帧并
  `tracing::warn`（原 join 的"同 id 合并"职能已转 patch，重复追加会产生
  两行同 id，keyed 渲染身份冲突；拒收把错误挡在边界而非留给渲染层）。
- 字段级寻址在 Value 域实现：`apply_patch` 对锚定子树做 `Accrete ↔
  serde_json::Value` 往返——指针直接作用在 Value 上，补丁写回后重新
  反序列化，失败（如 `attrs/class` 塞进非数组）则整条补丁回滚 + warn，
  节点保持原值（补丁的原子性由类型系统兜底，不手写逐字段 setter）。
  指针词汇与线上帧字段名天然一致（同一 serde 形状），无第二套路径语法。

## 理由

- **帧大小与树大小解耦**：patch 帧恒为 路径+值，与目标所处深度、兄弟数量
  无关；token 流回到 O(1) 每帧。
- **寻址走已验证的两条通道**：具名槽与行 id 不动，只把"树内部"从位置
  配对换成显式指针；JSON Pointer 是标准格式，AI 可直接生成，无需私有条语法。
- **attrs 更新从不可能变为一条 patch**：`path: "/attrs/class"` + replace。
- **动作即语义**：append/remove 只对列表存在，patch 只对定点存在，每个
  action variant 单独可审计，符合封闭 enum 门卫的既有立场。

## Why Not

- **保留 join+Method（位置合并）**：它把"追加行"和"树内合并"两件事捆在
  一个动作里，捆住的部分恰好是三个问题的根源。fluxora 已存档，兼容没有
  收益，只有沉没成本。
- **CRDT / diff 式自动合并**：每个槽有单一权威生产者（gateway 顺序分发），
  不存在并发写场景；引入向量时钟是为不存在的约束付税。
- **完整 RFC 6902 JSON Patch（add/remove/move/copy/test）**：词表必须封闭
  且 AI 可校验，Replace/Append 两个 op 覆盖全部已知场景（定点改值、token
  流），其余 op 是为"别人会用"而非"我们会用"预留。
- **全局路径寻址（不用 id 锚点）**：列表行已有 id 通道，绝对指针穿过列表
  索引会随流式追加漂移（第 N 行随时在变）；id 锚定使行内路径与行序无关。

## 缓解

- 生产者对指针有效性负责：布局重构必须同步重构帧生成器；建议优先用
  id 锚点 + 行内短路径，避免长绝对指针。
- 未命中不静默：warn 日志带 event/id/path，可在 mirror 侧观察。
- 迁移是本仓库内闭环（examples + stage + ui），无外部协调成本。

## 后续

`Content::Tmpl` 与 minijinja 的删除、"json 组件"（固定骨架 + 微型插值）
的引入是独立决策，另立 ADR（0006），本 ADR 不预设立场。
