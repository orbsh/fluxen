# ADR 0008: 本地落点——纯前端联动，不上行

状态：已接受
日期：2026-10-01
仓库：fluxen

## 背景

需求：点击频道列表切频道、点菜单切页面——组件间的联动全部发生在浏览器内，不该走 transport 往返（上行 → 服务端 → 再下发，一个点击绕世界一圈）。

当时的现状（2026-10-01）是：发射侧三条路径全部硬编码上行——`use_target`（hooks.rs，button/picker 的事件闭包 → `ctx.send`）、input 的 Enter 直发、form 聚合字段后按事件名 submit，落点没有任何维度；接收侧的机制却已经存在且组件无关——`source` 读一个具名槽，文本渲染它、canvas 的载荷读它、`tiles` 订阅它的列表槽。缺的只是把"事件的落点"从服务端改成本地信号。

一个死变体 `Target { target }` 当时全仓无消费者（ui 不读、stage/examples 不产）。用户裁决（2026-10-01）：不复活——`Target` 太模糊，删除，改用本地落点。

## 决策

### 1. 落点是 `scope`，不是一种新类型

`Scope` 是闭枚举（ADR 0031 §1）：`remote`（缺省，wire 上不写）、`local`、`both`。本地落点就是 `sink` 带 `scope: local`：

```yaml
sink: { dest: chan,  scope: local }   # 写本页的值槽，不上行
sink: { dest: gate.form }             # 缺省 remote：调宿主上的处理器
sink: { dest: chan,  scope: both }    # 先写本页，再上行——同一个出口两个去处
```

`dest` 是那个值槽的名字。取 `dest` 而不是 `path`：槽位是"持有值的那个东西"，没人沿它走；也不是 `id`——节点已经有一个 id。一个 `sink` 一个落点，两处都要时不是再加一个出口，而是 `scope: both`。

**两个槽平面。** `Ctx` 有数据平面（`slot_for_data`，装 Accrete 展示节点）与值平面（`slot_for_value`，装裸 `Value`）。本地落点与事件订阅走值平面——事件数据不是展示形态，硬包成一个 `text` 节点塞进数据平面是拿槽位当垃圾通道。

`Source` 带可选 `path: Option<String>`（JSON Pointer，进槽内节点的 WIRE SHAPE 提取，复用 ADR 0005 的路径词法，只读版 `get_at`）。对裸 `Value` 的订阅同样吃 path，形状归生产端，与 wire-shape 无关。

发射语义：本地写永远是**整值**——写必整体，提取是读方的事。唯一例外是 sink 自己带 `path` 时按"名下成员"合并写（ADR 0031 §2），这样控件只改它拥有的那个成员，不必知道值的其余部分。

### 2. 发射侧收敛到一个咽喉点

三条发射路径共享 `Ctx::emit(&Sink, id, payload)`（ctx.rs）：

```
dest 缺席            -> false，无处可去，什么也不发
scope 含 local, 有 path  -> write_local_at(dest, path, payload)   // 合并写那一个成员
scope 含 local, 无 path  -> 整个 Outflow 存进以 dest 命名的值槽     // 本地整值
scope 含 remote          -> ctx.send(dest, id, payload)            // 上线（ADR 0023）
```

`use_target`（hooks.rs，button/picker 等）、input 的 Enter、`loom` 的 submit 打包都只是这个助手的入口。本地那条分支写进去的是同一个 `content::Outflow { dest, id?, data }`——所以一次本地 emit 与一个上行帧是同一种形状（ADR 0010），整条链没有 socket 也能演练。

订阅侧统一在 `use_source_value` 读：`source.path` 走 WIRE SHAPE 提取。凡取值走它的组件（`text`、`picker` 的值、`tiles` 的列表、canvas/chart/diagram 的载荷）天然可订阅，无需逐个改造。

Canvas 的 `host.send` 同样只到 `ctx.send`——将来 Canvas 要事件绑定（ADR 0007 的后置项），落点机制对它没有特判。本 ADR 不做 Canvas 事件字段。

### 3. wire 面同步（改一个通道时的固定清单）

- `stage schema` 跟着 serde 走，导出侧不需手改（ADR 0037 起每个 arm 自带 `$ref`）。
- 下游客端从快照重生成（reflux 的 `tools/sync_schema.py --write` → `gen_sdk.py`）。
- 示例：`examples/yaml/14.local_routing.yaml`（菜单 picker 发射进 `chan` 值槽、header 订阅同槽）。
- fluxora 存档不动。

### 4. 边沿与电平（约定，非机制）

值槽是电平语义：同值写两次，订阅方看到的值不变。瞬时事件（自动播放 tick、"重置"）要可靠触达，约定载荷自带区分字段（如 `seq` 或时间戳）——写整值必然产生新值，订阅端比较即可判"真的发生了一次"。不建事件通道：通道 = 第二套寻址，违背 ADR 0005 的操作层统一。

### 5. 明确不做

- 双向同步（地址栏/表单回流类）：需要 origin 标记或同值短路来防读写成环，且"环境原点"（popstate）比组件交互多一个入口——独立单元。
- 局部派生表达式（badge 计数、级联选项）：wire 里长出 mini-language 违背闭词汇表哲学（与模板 `{% %}` 被砍同一判据）。派生归服务端：全 WS 下订阅方上行查询、服务端 push `set`/`patch` 就是正解。
- 组件内瞬态交互（hover/focus/drag）：无跨组件消费者，不进任何槽，归组件内部信号，wire 不认识它。
- 服务端联动表/路由 DSL：连线是布局帧生产端声明的（`source`/`sink` 里写槽名即连线），UI 核心不预设任何业务联动。
- 事件名与本地槽的 fan-out/fan-in 语法：一个出口一个落点，要多落点就在布局里多一个节点。
- 列表槽的本地写入（`append`/`remove` 语义）：本 ADR 只写值平面的整值替换。流式列表联动（往 `tiles` 追行）等真实需求出现再扩，语义上等价于把 `Content::Append` 的落点本地化，机制同族。

## 后果

- 出口的类型是 `Sink { dest, scope, path }`，入口的类型是 `Source { slot, scope, path, default }`（ADR 0039：读与写不是一种类型）。`target` 这个 kind 消失，旧帧里若有它会 decode 失败——全仓示例确认无生产者，外部生产者（fluxora 时代模板）本就不做兼容（ADR 0005 先例）。
- `emit` 助手让"上行 vs 本地"第一次成为布局帧里的显式声明，事件语义不再隐含"必有服务端"。
- 纯前端联动零 RTT；transport 只承载真正需要服务端参与的交互。