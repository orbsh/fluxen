# ADR 0002: 渲染后端只保留 Leptos，移除 Dioxus

状态：已接受
日期：2026-09-25
仓库：fluxen

## 背景

fluxen 从 fluxora 拆出时同时带了两个渲染后端：`ui_dixous`（Dioxus）与
`ui_leptos`（Leptos），由 `fluxen` facade 按 feature 选择。两后端共享同一
数据模型：Accrete DSL（闭合类型 enum + Bind 系统）、流式 merge
（Replace/Concat/Delete）、CBOR/JSON codec、`Transport` trait。

移除触发点是一次构建失败：workspace 联合构建下，Dioxus feature 会把
`dioxus_signals`（内含 `generational_box::UnsyncStorage`，RefCell 实现，
非 `Sync`）拉进 `accrete`，而 Leptos 的 `RwSignal` 要求槽位值 `Send + Sync`。
两边在 accrete 这个交汇点上数学上不可能同时成立，`ui_leptos` 编译报 18 错。
单独构建各自通过，联合必炸——双后端在 feature 层面就不自洽。

## 决策

删除 `ui_dixous` / `ui_dixous_macro`，Leptos 为唯一渲染后端。同时清除
accrete DSL 中一切 Dioxus 痕迹：`feature = "dioxus"`、`derive(Props)`、
`BindVariant::Field/Submit` 内嵌的 `Signal<Value>` 字段。

## 理由

1. 本项目的渲染模式是全动态数据驱动：没有编译期 UI 树，`gen_dispatch!`
   把 Accrete 纯数据在运行时展开成视图，结构本身就是下行帧的数据。在这种
   模式下框架被用到的特性只剩三样——信号、订阅重跑的闭包、事件 handler。
   Dioxus 的相对优势（组件模型、rsx 模板、VNode diff）恰好全部不参与：
   组件树每次都是从数据现搭的，diff 收益趋近于零；细粒度订阅（Leptos）
   反而与流式 merge 语义天然对齐（一个 token 的 Concat 只重跑被触及的
   文本节点）。
2. `accrete` 携带 `Option<Signal<Value>>`（Dioxus 运行时句柄）是分层违例：
   共享 DSL 被渲染框架的运行时类型污染。这不是移植瑕疵，是结构性的——
   它同时是 1 中构建冲突的根因。移除后 accrete 回归纯数据，表单信号改由
   `Ctx` 沿渲染克隆链传递（view 层内部机制）。
3. 双后端没有消费方：上游（stage mirror / 未来的 Prism / Aura realm）只
   发 Accrete 操作，不感知渲染框架。"可换后端"从未被行使，维护成本却按
   两倍计（两套 widget、两套 hooks、feature 矩阵）。

## Why Not

- 为什么不"修好"双后端共存（如 accrete 拆出无框架 feature 的纯数据核 +
  框架绑定层）？——为一个从未被行使的可选轴增加一层 crate 边界与类型
  转换，违背奥卡姆；且见理由 1，即便共存成立，Dioxus 在此模式下也没有
  结构性收益可保。
- 为什么不选 Dioxus 弃 Leptos？——Leptos 的信号要求 `Send + Sync`，恰与
  "共享 DSL 不得携带非线程安全句柄"同一判据，从类型上逼出正确分层；
  Dioxus 的 `Signal`（UnsyncStorage）则允许并实际发生了污染。细粒度更新
  对流式渲染的契合（理由 1）同样单向有利于 Leptos。
- 保留 facade crate 做什么？——唯一后端后 facade 近乎空壳，暂保留作为
  装配别名与未来 wasm 入口聚合点；若持续无内容，删除是后手。

## 后果

- workspace 联合构建恢复绿色；`cargo test --workspace` 与 wasm 目标构建
  均通过（2026-09-25 验证）。
- `BindVariant::Field` 不再携带信号，表单归属经 `Ctx.form` 克隆链传递
  （见 ui_leptos hooks/ctx 实现）。
- ADR 0001 中 "`transport-ws` 是两个 UI crate 的默认依赖" 的表述就此过时，
  历史文本保留不改，以本文为准。
- 若未来真要引入第二后端，前提是其渲染模型能落在同一"零编译期结构"约束
  下且不再要求 DSL 携带运行时句柄——按本 ADR 的判据重新评审。
