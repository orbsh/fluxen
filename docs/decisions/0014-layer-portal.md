# ADR 0014: 模态层的渲染位置——layer 容器 + portal 传送

状态：已接受（2026-10-10 用户确认后实施）
日期：2026-10-10
仓库：fluxen

## 背景

ADR 0013 给 Float 长出 `overlay`/`dismiss`，用来做登录模态框，但遮罩仍然**声明在消息
行里**：DOM 上它挂在气泡子树内，全靠 `position: fixed` 逃出去，祖先里一旦出现
transform / filter / contain 就会被困住；而"打开模态的是某条消息里的按钮、遮罩却属于
应用外壳"这个错配是本质的。用户 2026-10-10：「模态背景确实超出了当前组件的范围」
「可以考虑类似 vue 的 transport 组件，挂一个全局的遮罩层，有一个 slot（和 case
差不多，加一些控制逻辑），然后通过 Local 变量控制其显示/隐藏」。

## 决策

**渲染位置与内容分离：应用显式声明一个全屏 `layer` 作挂载点，`portal` 把 children
渲染到它里面；显隐由 layer 的载荷槽决定（空 = 不显示）。**

1. **wire**：两个新变体，省略即旧行为（不新增时零影响），schema 自动跟随。
   - `layer`：`{ id, attrs: { class?, selector?, dismiss? }, bind: { value: <Local /
     Source 槽> }, children }`。attrs 只留 `dismiss`；遮罩（铺满视口、模糊背景、
     挡住背后）是 layer 的默认形态，不做开关。
   - `portal`：`{ attrs: { to: <selector> }, children }`。只负责渲染位置，别的语义
     一概没有——不碰显隐、不碰遮罩。
2. **挂载点显式声明**（用户 2026-10-10 选定）：layer 由应用写在骨架里（通常根级，
   与 pages 平级），portal 用 `to` 指过去。渲染器不新增任何"固定逻辑"，守住
   ADR 0007 的立场（core 里零固定渲染逻辑）。目标元素不存在时 portal 不渲染 +
   warn（fail-loud）；布局每次变更重新解析目标——挂载点可能比 portal 晚出现。
3. **显隐 = 载荷空否**（复用 ADR 0012 的"空即无"）：槽空 → layer 不显示、不占位；
   非空 → 铺满视口 + 遮罩，内容 = children。不新造布尔开关，"清空槽 = 关掉"白送。
4. **几何内联、遮罩走 CSS**：几何（`position: fixed; inset: 0; height: 100vh;
   z-index`）内联在 layer 宿主上（同 ADR 0012 的 display 内联；本机 headless
   chromium 上 `fixed` + `inset: 0` 的高度不撑开，见 PLAN §16）；模糊/色调走
   custom.css 的 `::before`——层级定序缺陷已于 2026-10-10 修掉（PLAN §19），
   CSS 这条路现在真的通。
5. **dismiss 是本页瞬时状态**：点 layer 自身（`e.target === e.currentTarget`，即
   模糊区而非模态内容）时**清空所绑的槽**——显示与否本来就是"槽空否"一条规则，
   清空即关，不新造第二个掩蔽状态；槽空了，下次写入必是新值，"同值不写"
   （ADR 0011）不会误挡重开。只写 local/data 平面，不上线协议。故 dismiss 需要
   槽绑定：inline default 的层无槽可清，只 warn。判等逻辑从 float.rs 搬过来。
6. **切页不带走模态**：layer 与 pages 平级，页面切换不销毁它——这正是"挂在消息行里"
   给不了的（现状切页即丢）。

## 后果

- Float 的 `overlay`/`dismiss` **撤掉**（用户 2026-10-10：「撤」）；ADR 0013 标记为
  被本 ADR 取代。登录示例（`examples/yaml/17.login.yaml`）改为 layer 作挂载点 +
  触发行写槽打开（输入框的 Enter——按钮只认 `kind: submit`，不做普通发光），模态内容
  写在触发行里、由 portal 传送进 layer——传送这条由该例正面覆盖（就地不渲染、内容
  落在 layer 里），不再另开 portal 示例；实测见 docs/PLAN.md §20 第 ④ 条。
- 内容两种写法任选：直接写在 layer 的 children 里（最省），或写在触发处附近、用
  portal 传送过去（内容与触发点耦合时，登录示例即此形）。
- 两类形状（带 / 不带新字段）都过真实 codec 的往返测试——wire 是契约。门槛见
  docs/PLAN.md §20。