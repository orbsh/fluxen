# ADR 0013: Float 的 overlay 选项——背景模糊 + 不可操作

状态：已接受（2026-10-10 用户确认后实施）
日期：2026-10-10
仓库：fluxen

## 背景

Float 是个纯定位容器：`PositionAttr` 给 left/right/top/bottom 偏移，CSS 里
`.float` 只有 `position: absolute`；Popup 负责 hover 展开，两者组合出悬浮卡片
（examples/yaml/00.chat.yaml 的购物车浮层就是这么搭的）。

缺的是**模态形态**：登录框这类浮层要"把后面的页面挡住、并模糊它"，现在拿
PositionAttr 摆位置做不到——背景既没被遮，交互也没被挡住。用户 2026-10-10：
「浮动层添加选项，背景模糊不可操作，用于登录模态框」。

## 决策

**Float 加 `overlay` 选项：铺满视口、背景模糊、背后页面不可操作；`dismiss`
选项让它点模糊区关闭。**

1. **wire 形状**：`PositionAttr` 加两个普通（非 flatten）可选字段
   `overlay: bool`、`dismiss: bool`，线上写 `attrs: {overlay: true,
   dismiss: true}`。PositionAttr 只被 Float 引用（accrete/src/lib.rs 唯一使用
   点），所以爆炸半径是这一个组件；新字段不进 PosH/PosV 那对 flatten enum，
   不引入信封歧义，省略时旧行为逐字不变。
2. **overlay 的形式**：`float.rs` 在 overlay 模式加 `overlay` 类，几何（`position:
   fixed; inset: 0; z-index: 100` + flex 居中）**内联**在宿主 div 上；CSS 只负责
   `::before` 的模糊与色调、以及把内容压在模糊层之上。几何必须内联的原因是一条
   实测的层级缺陷：custom 层实际排在最前——main.css 第 1 行的
   `@import url("custom.css") layer(custom)` 先于第 3 行的 `@layer theme, base,
   layout, element, custom, style;` 出现 `custom`，而层序由首次出现决定，于是写在
   custom 层里的 `position` 会输给 element 层的 `.float{position:absolute}`（同一
   缺陷正是老坑"`.hide` 打不过 `.f`"的根因；PLAN §19 记录，并于 2026-10-10 按用户
   指示修掉——层序声明移到 import 之前，`custom` 归位到 element 与 style 之间）。
   内联无此问题，与 ADR 0012 的 display 内联同款，故保留。h/v 偏移在 overlay 模式下
   没有意义，随之忽略。
3. **"不可操作"靠铺满视口**：点模糊区就是点在浮层自身（空白填充区），事件不会
   落到背后页面；模态内容照常可交互。因此不需要 pointer-events 小把戏，也不需要
   给背后页面加 inert。
4. **dismiss 是本页瞬时状态，不上线协议**：点浮层自身（`e.target === el`，即模糊
   区而非模态内容）时本地隐藏（内联 `display: none`）。这与 Popup 的 hover 展开
   同类——组件内部瞬时状态按既定纪律不进 wire（Pitfalls）；应用要真正移除节点仍
   走常规 `remove` 帧。
5. **stacking**：overlay 的 z-index 取应用面板之上，具体值写在 custom.css 那条
   规则的注释里，免得被 sidebar / tooltip 一类盖住（数值依据是现有面板的层级）。

## 后果

- 新增 `examples/yaml/NN.login.yaml` 作为回归样本：overlay float + 登录表单、
  `dismiss: true`。
- schema 自动跟随（PositionAttr 已 derive JsonSchema），`stage schema` 不需要
  手改。
- 向后兼容：不带新字段的旧帧解出的 Float 与今天行为一致。两类形状（带/不带新
  字段）都要过真实 codec 的往返测试——这是 Binding/Attr 加字段后的既定门槛。