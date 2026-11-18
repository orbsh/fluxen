# ADR 0027: A frame is one node — "will this be filled?" is a binding, not a type

状态：已接受
日期：2026-11-18
仓库：fluxen

## 背景

组件目录里有两个东西住在同一个文件（`components/case.rs`），却分属两条轴：

    case         case.rs:9    结构容器：children 写死，bind 只是个名字（class/id）
    placeholder  case.rs:52   数据区域：绑 Source 时画槽里的节点树并淡入，否则画 children

`placeholder` 的实现是 `case` 的超集——它的 else 分支就是 `case` 干的事。唯一
的差别是"绑了 source 时读不读槽"。

把它们分成两个组件，等于要求作者在写布局时预判这块将来会不会被数据填。而答案
会变：今天静态的容器，明天要变成某个区域。类型不能后加，绑定能。

这个代价在 prism 的外框上真实发生过：pager 的行用了 `case` + `source`，看起来
完全正确，渲染出来却永远是空盒子——因为 `case` 把 binding 当名字，不画槽里的东西。
一个组件如果是"画 children 的盒子"，另一个是"画槽的盒子"，那这条规则就该由
**绑定**表达，而不是由节点类型表达；否则同一个盒子在两种用法之间切换要换类型，
而写帧的人并不知道自己六个月后会不会需要它。

## 决定

1. `case` 改名 `frame`（结构体 `Case` → `Frame`，`CaseAttr` → `FrameAttr`）。
2. 删 `placeholder`。它的行为并入 `frame`：

       bind.value.kind = source   槽有值 → 画槽里的树（一次性淡入淡出）
                                  槽空   → 画 children（默认内容）
       无 bind                    画 children

3. `attrs` 统一用 `FrameAttr`（`class`/`selector`/`horizontal`/`grid`），所以原来
   `placeholder` 撑不起的 grid 布局现在一条路都通。
4. 淡入淡出改由 CSS 一次性动画承担（`main.css` 里的 `.fade-in-and-out`）：元素随
   槽值重建，动画自己重播，不再需要 JS 去找 id 加类再删类。

## 迁移

    {"type": "case"}         → {"type": "frame"}
    {"type": "placeholder"}  → {"type": "frame"}   （保留 bind，行为不变）

## 后果

- 一个盒子只有一种写法；"静态"与"被数据填"是同一个节点的两种状态。
- **隐藏耦合要记住**：`use_common_css`（`hooks.rs:12`）按类型名决定默认是否加
  `col`，表里原本写的是 `"Case"`。改名时这张字符串表必须同步，否则所有 frame
  都不再默认纵排——布局会整体散掉，而编译器不会提醒。
- DOM 里的 class 从 `case`/`placeholder` 变成 `frame`。`main.css` 里没有这两
  个类的样式（`.f`/`.g`/`.col` 管布局），所以没有样式回退。
