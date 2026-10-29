# ADR 0022: 槽位在线上叫 `slot`——`event` 只留给事件名

状态：已接受（2026-10-29 用户裁决后实施）
日期：2026-10-29
仓库：fluxen

## 背景

`content` 的五个动作里，树内的**地址**一直叫 `event`：

- `Influx.event`（`create`／`set`）——数据槽名；`create` 恒写 `""`
- `AppendOp.event`、`RemoveOp.event`——列表槽名
- `PatchOp.event`——布局根、某一行、或数据槽（`""` = 根）
- `InjectOp.slot` 例外：它早就是 `slot`，另有一个可选的 `event` 是"被模拟的那个本地事件名"

同一个词在上行那一侧是另一件事：`Outflow.event` 是**事件名**，节点 `bind.<key>.event` 也是事件名
（`{kind: event, event: "row.remove"}`）。于是同一帧的两半都用 `event`，而含义相反：下行它是地址，
上行它是名字。用户 2026-10-29 的原话是「免得后端处理的时候名字撞车，太乱」——处理器一边读
`frame["event"]`（上行事件），一边写 `{"action": "set", "event": "summary"}`（下行槽），读到同一个词
却要换脑子。

## 决策

**树内的地址叫 `slot`；`event` 只出现在事件上。**

1. `Influx<T> { slot, data }`：`set` 写具名数据槽；`create` 用 `slot: ""` 指向布局根——与 `patch` 的
   根约定同一个写法（`slot: ""` = 根），不再有两套。
2. `AppendOp<T> { slot, position, data }`、`RemoveOp { slot, id }`、`PatchOp { slot, id?, path, op, value }`。
3. `InjectOp { slot, event?, id?, data }` **不变**：它的 `slot` 是 vals 平面的通道，`event` 才是被模拟
   的事件名（形状与 `Outflow` 一致，ADR 0011）。这正是"一个词一个意思"的反面证据——它本来就没混。
4. 破坏性变更，无别名：旧帧里的 `"event": "<槽名>"` 现在解不出来（`content` 的 op 不 deny 未知字段，
   所以旧键会被忽略、槽名缺失即 `slot` 缺失 → 解码失败）。生产端（stage 的 YAML、reflux 的
   `wire.py` 与 SDK、prism）必须同批换。
5. `Outflow.event`、`BindVariant::Event { event }`、`RenderEvent` 之类**不动**——它们本来就是事件名。

## 代价

- 换名触及所有示例 YAML（35 个文件里 31 个的顶层 `event:`）与两个 SDK 的帧层，属于一次机械但
  面广的破坏性改动；`append`／`patch` 的语义一点没动，只是地址的字段名。
- 旧生产者不会得到"友好提示"——它拿到的是解码失败（`missing field \`slot\``）。这一点与 ADR 0021
  决策 6 的处理不同（那边旧形状被点名拒绝，因为帧的**层级**变了，能一眼认出来）；同层换名认不出来，
  所以这条只写在文档与 ADR 里，不做运行时兼容层。

## Why Not

- **保留 `event` 并在文档里解释"下行 event 是槽、上行 event 是名"**：那就是今天的状态，也是用户判
  "太乱"的那一条。文档不能修掉一个同词异义。
- **改成 `name`／`key`／`address`**：`name` 在 `tmpl`（模板名）与 `id`（行身份）旁边照样含混；`key` 与
  `bind` 的键撞；`address` 太长且没说明"哪些东西有地址"。`slot` 是这个词在本仓已有的含义
  （`ctx.slot_for_data`、`reflux` 的"槽"、DESIGN 的"四个平面"），换名是收敛不是发明。
- **给 `create` 另设一个不带地址的变体**：`slot: ""` 已经是根的统一写法（`patch` 早就在用），多一个
  变体只是多一条要学的规则。