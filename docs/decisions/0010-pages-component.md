# ADR 0010: Pages 组件——id 字典 + 事件包装形状统一

状态：草案（待确认）
日期：2026-10-01
仓库：fluxen

## 背景

标签页/频道导航的真实数据形态是两份互异的数据：菜单项（短标签 + id）
与页面内容（整棵 Accrete 子树）。15.tabs.yaml 的初版把两者绑到同一个
local 值上，被用户判为无实用价值——页内容不可能等于被点的 id。

正解模型（用户定稿 2026-10-01）：select emit 一个事件对象（如
`{"event":"channel::select","data":"2"}`）落槽；Pages 订阅该槽、按
`path`（`/data`）提取键，在其内部以 id 为键维护字典（复用列表行
`id` 的既有纪律——行身份在数据里），命中哪个键就显示哪个页面。
查找发生在 Pages 私有状态里，不引入任何新的寻址语法：path 是静态的，
提出来的字符串就是字典的键。

载荷形状的统一规则（同轮裁决，修订 ADR 0008 的 Local 分支）：
**事件数据的形状由事件定义，不由落点决定**。上行帧的载荷历来是
包装对象（`Outflow {event, id?, data}`）；若允许 local 落槽写裸值，
同一个事件从 `kind: event` 改绑 `kind: local` 就得换形状，订阅方
（path、Pages 键）跟着碎。所以 Local 与 Event 携带同一个包装对象，
路由只决定它去哪。path 的存在意义由此坐实：载荷是结构，消费方按
字段取件。

`display` 选项来自真实需求：页面切换有"丢状态"与"保状态"两种语义——
chat 页的滚动位置、表单草稿、canvas 的 GL 上下文，隐藏 DOM 全保，
卸载重建全丢。

## 决策

### 1. emit 载荷形状统一（修订 ADR 0008 §2）

`Ctx::emit` 的 Local 分支不再写裸 payload，改写与上行同形的包装对象：

```json
{"event": "<事件名>", "id": "<发射节点 id，可缺>", "data": <载荷>}
```

事件名来源：`Event { event }` 用它的事件名；`Local` 变体增可选字段
`event: Option<String>`，缺省 = 槽名（槽即频道）。这是本 ADR 唯一的
wire 字段追加（`Local` 从 `{slot, path?}` 变 `{slot, path?, event?}`，
path 仍是订阅侧字段、发射侧忽略）。Event/Field/Submit 行为不变。
示例 select 帧：`kind: local, slot: page` emit 出
`{"event":"page","data":"2"}`；显式 `event: "channel::select"` 则
`{"event":"channel::select","data":"2"}`。

订阅侧零新机制：`kind: local, slot: page, path: /data` 已是现成语义
（tracked 读槽 + 静态指针提取），Pages/标题等读方直接用。

### 2. 新 Accrete 变体 Pages

```rust
Pages {
    id: Option<String>,
    attrs: Option<ClassAttr>,
    bind: Option<HashMap<String, Bind>>,
    #[serde(default)]
    display: PagesDisplay,     // Render（默认）| Dom
}
```

wire tag `pages`，derive 门控照 rack 抄。bind 固定两个键：

- `"value"` → `kind: source`（list 平面）：页面行的来源。行 = 任意
  Accrete 节点，`id` 即字典键（复用 rack 的键纪律：无 id 行退位置
  `#{idx}`）。服务端用现有 append/set 逐页喂，patch 可只动一页的子树。
- `"select"` → `kind: local`（或 `source`）+ `path`：选中信号订阅。
  tracked 读槽、按 path 提键；path 缺省 = 整值（提取的 Value 转字符串
  作键；非字符串 = warn + 不切换）。

未命中键 = warn + 空白（对齐 miss-warn-drop）；信号未写入 = 空白。

### 3. display 语义

- `render`（默认）：只挂载命中行，其余不 mount。切页省内存；
  子树状态（滚动/输入/GL）随卸载丢失。
- `dom`：所有行常驻 DOM，未命中行容器加 class `hide`
  （`display:none`）。状态全保——chat 页滚回原处、canvas 不重建；
  代价是全部行的渲染与内存常驻，生产端自选。

字典在渲染层维护（`RwSignal<HashMap<String, Accrete>>` 派生自 list
槽，非 wire 概念），与 Ctx 槽平面无关——Ctx 平面仍是 data/list/vals
三个，不再增多。

### 4. 示例与验证

- 15.tabs.yaml 重写为诚实形态：菜单 = list 槽的短标签行；页面 =
  另一批内容互异的行（首页=文本块、文档=表单、关于=chat 样例行）；
  select `kind: local, slot: page`；Pages `bind.value→source: pages`、
  `bind.select→{kind: local, slot: page, path: /data}`；标题用同 path
  演示"同事件两处取件"。
- 测试：emit 包装形状单测（Local 带/不带 event、id 透传）；Pages
  键提取与未命中 warn 单测；display 两模式的 DOM 取证（dom 模式：
  切走再切回，滚动位置探针存活）。
- 三门槛 + e2e（stage 直驱）。

### 5. 明确不做

- 路由/历史同步（地址栏）：仍是 ADR 0008 §5 的独立后置单元。
- 键模板/动态 path：查找在 Pages 私有字典里，不需要寻址语法变复杂。
- Pages 多信号源/优先级：一 value 一 select，两键定死。
- rack 的定位/滚动 API（scroll-to 某行）：无真实需求前不加。

## 后果

- ADR 0008 的 Local 落槽形状变为包装对象（wire 兼容注意：已发布的
  14.local_routing 示例与 e2e 记录里 local 槽是裸值——本 ADR 落地时
  同步改示例；旧 wire 帧 `kind: local` 的 decode 不受影响，变的是
  运行时槽内容形状，订阅方 `path` 语义因此才有了稳定对象可指）。
- `Local` 增可选 `event` 字段（追加式、向后兼容）。
- 闭词汇表 +1 变体（pages），schema/serde 单一事实源自动跟。
- "事件 → 槽 → 按键取件"成为组件间联动的全形：菜单/走马灯/详情面板
  都是 Pages 的换皮或子集，不再有"同一文本既当菜单又当内容"的假示例。
