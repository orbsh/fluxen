# ADR 0008: bind 本地路由——Local 事件落点（纯前端联动，不上行）

状态：已接受
日期：2026-10-01
仓库：fluxen

## 背景

需求：点击频道列表切频道、点菜单切页面——组件间联动全部发生在浏览器内，
不该走 transport 往返（上行→服务端→再下发，一个点击绕世界一圈）。

现状核对（2026-10-01）：
- **发射侧**三条路径全部硬编码上行：`use_target`（hooks.rs，button/select
  的事件闭包 → `ctx.send`）、input.rs 的 Enter 直发、form.rs 聚合 Field
  后按 Event 名 submit。Event 变体没有任何"落点"维度。
- **接收侧机制已存在且组件无关**：`kind: source` 读 Ctx 具名 data 槽
  （use_source_value：槽内节点 `bind.value.default` 优先，否则整节点
  JSON）——case/placeholder 渲染它、canvas 载荷读它、rack 订阅 list 槽。
  缺的只是把"事件的落点"从服务端改成本地信号。
- 死变体 `Target { target }`：全仓无消费者（ui 不读、stage/examples 不产）。
  用户裁决（2026-10-01）：不复活——`Target` 太模糊，删除；新增
  `Local { … }` 承载本地落点语义。

## 决策

### 1. wire 形态：BindVariant 增删两个变体

```rust
// 删除（死变体）：
Target { target: String }
// 新增：
Local { slot: String, path: Option<String> }  // 具名本地值槽（local 平面）
```

字段名取 `slot`：wire 是 AI 生产者与外部系统看的词汇表，"signal" 是
渲染层反应式原语的泄漏。载荷与展示形态分平面（用户裁决 2026-10-01）：
Ctx 有 `data`（Accrete 展示节点）与 `vals`（裸 Value）两个槽平面，
Local 落点/订阅走 `vals`——事件数据不是展示形态，硬包成
`text{bind.value.default}` 塞 data 槽是拿槽位当垃圾通道。

`Source` 变体增可选 `path: Option<String>`（JSON Pointer，进槽内节点
WIRE SHAPE 提取——复用 ADR 0005 的路径词法，只读版 `get_at`）。
订阅侧的 path 提取同样适用于 `Local`（进裸 Value 的 JSON Pointer，
形状归生产端，与 wire-shape 无关）。

发射语义：`Local` 的 emit 永远写**整值**（忽略自身 path——写必整体，
提取是读方的事）；一个 bind key 一个落点（要么上行要么本地），
两要都要 = 两个 key，不发明组合语法。

兜底语义（与既有 Source 槽行为一致）：local 槽未写入时回退自身
`bind[key].default`——渲染未喂先有初始显示。

### 2. 发射侧路由：收敛到一个咽喉点

三条 emit 路径改为共享一个落点助手（`Ctx::emit`，ctx.rs）：

```
emit(ctx, bind_variant, id?, payload):
  Event { event }        -> ctx.send(event, id, payload)        // 上行，现状不变
  Local { slot, path? }  -> ctx.slot_for_value(slot).set(payload)  // 本地整值，不上行
```

input.rs / form.rs / use_target 各自 match 到对应变体调用助手；form 的
submit 聚合路径同样支持 Local（整张表单记录进槽）。订阅侧在 `use_source`
统一读：`Source.path` 进 WIRE SHAPE（`get_at`），`Local.path` 进裸 Value
（serde_json Pointer）——凡取值走 use_source 的组件（text、select 的
value、placeholder、canvas/chart/diagram 载荷）天然可订阅，无需逐个改造。

Canvas 的 host.send 同样只到 `ctx.send`——将来若 Canvas 要事件绑定
（ADR 0007 后置项），落点机制对它无特判。本 ADR 不做 Canvas 事件字段。

### 3. wire 面同步（kind 集合改动的固定清单）

- `stage schema` 自动跟 serde（无需改）。
- kdl_parse：kind 节点名 `local`，首参映射 `slot`（同 `event` 惯例）；
  死 kind `target` 拒绝（serde 不认识即 parse 失败，无需特判）。
- 示例：`examples/kdl/14.local_routing.kdl` +
  `examples/yaml/14.local_routing.yaml`（菜单 select 发射进 `chan`
  值槽、header 订阅同槽——频道形态；path 提取注释见 kdl 帧内）。
- fluxora 存档不动。

### 4. 边沿与电平（约定，非机制）

值槽是电平语义：同值写两次，订阅方看到的值不变。瞬时事件（自动播放
tick、"重置"）要可靠触达，约定载荷自带区分字段（如 `seq` 或时间戳）——
写整值必然产生新值，订阅端比较即可判"真的发生了一次"。不建事件通道：
通道 = 第二套寻址，违背 ADR 0005 的操作层统一。

### 5. 明确不做

- 双向同步（地址栏/表单回流类）：需要 origin 标记或同值短路防写读环，
  且"环境原点"（popstate）比组件交互多一个入口——独立单元。
- 局部派生表达式（badge 计数、级联选项）：wire 里长出 mini-language
  违背闭词汇表哲学（与模板 `{% %}` 被砍同一判据）。派生归服务端：
  全 WS 下订阅方上行查询、服务端 push set/patch 就是正解。
- 组件内瞬态交互（hover/focus/drag）：无跨组件消费者，不进任何槽，
  归组件内部信号，wire 不认识它。
- 服务端联动表/路由 DSL：连线是布局帧生产端（AI）声明的（bind 里写
  slot 名即连线），UI 核心不预设任何业务联动。
- 事件名与本地槽的 fan-out/fan-in 语法：一 key 一落点，需要多落点时
  生产端在 emit 组件上多挂 bind key。
- list 槽的本地写入（append/remove 语义）：本 ADR 只写 local 值平面
  整值替换。流式列表联动（往 rack 追行）等真实需求出现再扩，语义上
  等价于把 Content::Append 的落点本地化，机制同族。

## 后果

- BindVariant kind 集合：`source | local | event | field | submit | default`
  （Target 消失）。破坏性 wire 变更：旧帧里若有 `kind: target` 会 decode
  失败——全仓示例已确认无生产者，外部生产者（fluxora 时代模板）本就
  不做兼容（ADR 0005 先例）。
- emit 助手让"上行 vs 本地"第一次成为布局帧里的显式声明，事件语义不再
  隐含"必有服务端"。
- 纯前端联动零 RTT；transport 只承载真正需要服务端参与的交互。
