# ADR 0017: Canvas 的模块选项——一个动态袋，语义归模块

状态：已接受（2026-10-23 用户裁决后实施）
日期：2026-10-23
仓库：fluxen

## 背景

用户 2026-10-23：「Canvas 再加一个 3d-force-graph 的变体，示例参考 k10r。边和节点
的基础尺寸之类的做成可配置选项。……这种属于变体的不要放在 accrete 中，在 canvas
的选项中添加一个 Value 动态类型把额外的选项打包传给它引用的 js，js 那边自己处理
就可以了」。

两件事被同时定下来：

1. 需要一个力导向关系图的渲染器（k10r 的 `docs/kg-demo-3d.html` 是实测过的参考实现）。
2. 这类渲染器自己的配置项（节点/边的基础尺寸、力参数、配色……）**不进 DSL**。

ADR 0007 的 Canvas 只有 `id / url / attrs / bind`：`attrs` 是尺寸与 CSS 钩子，
`bind["value"]` 是**数据区**（流式载荷）。渲染器的静态配置没有落点。三条候选：

- **每个渲染器一个 Accrete 变体**（`Graph { nodeSize, linkWidth, … }`）——否决。DSL 的
  封闭词汇表会被外部库的词表逐个污染：换一个图库就改一次 wire，核心还得理解它不认识
  的键。用户裁决的原话就是这个意思（"这种属于变体的不要放在 accrete 中"）。
- **capability/registry 抽象**——ADR 0007 已否决过（为想象中的插件市场预付成本），
  不复活。
- **塞进数据区**（`bind["value"].default.options`）——否决。数据区是流：配置会跟着
  每一帧载荷重推，槽写入也会把配置一起改掉；配置与数据是两条生命周期不同的腿。

## 决策

### 1. `options` 是 Accrete 里唯一的动态袋

```rust
Canvas {
    id: Option<String>,
    url: String,
    attrs: Option<SizeAttr>,
    options: Option<Value>,        // wire tag `options`，缺省不出现
    bind: Option<HashMap<String, Bind>>,
}
```

核心对它零解释：不认键、不校验、不导出 schema。缺省/`Null` 时宿主传 `{}`，模块
自己套默认值。形状上的判据只有一条——**它是挂载期配置，不是数据**。

### 2. 宿主契约扩成四参：`mount(el, data, host, options)`

- `options` 以 **JS 对象**过界：wire 上的 `Value` 由宿主序列化成 JSON 文本，在
  mount 那一跳 `JSON.parse` 成对象。
- **不套 CBOR 信封**。数据面（`data`）用 CBOR 是因为它是流、模块必须防御式解码；
  配置是一次性静态值，模块不该为它再背一份解码器。ADR 0007 的"边界传数据不传
  对象"约束的是跨模块句柄与运行时对象（`host` 那种回调），一个纯 JSON 对象不是
  句柄——它没有身份、没有方法、不能反向调用宿主。
- 契约的其余三参不动；`update/resize/unmount` 都拿不到 options（它们不需要）。

### 3. 语义与 `url` 同级：挂载期静态

options 与 url 一样是**挂载期**配置：改它们不热生效，要重新挂载（换行 / 换节点）。
这不是新增限制，而是照抄 url 的既有规则——ADR 0007 写的就是"url 变更 = 重新挂载
（不做热替换）"。把配置做进 update 腿会引出"模块如何热重配"这套新机制，收益不抵。

### 4. 示例模块：forcegraph（3d-force-graph v1.80.1）

`examples/modules/forcegraph/index.js`，纯 JS、零构建，考据自 k10r 的实测实现：

- 库是 UMD（自带 three.js，暴露全局 `ForceGraph3D`），只能 `<script>` 注入——与
  g2chart 同一条先例。**库在 `mount()` 里才注入**（ADR 0015），模块求值期零下载。
  同源 `./3d-force-graph.min.js` 优先（Dockerfile 钉版本 + sha256），同源缺件才回落
  钉版本的 unpkg；`options.libUrl` 可显式覆盖。
- 载荷 `{nodes:[{id,name,kind,weight,color}], links:[{source,target,kind,strength,color}]}`，
  按节点 id 做增量（整份快照重复送达幂等）。三条硬约束照抄 k10r：`d3AlphaMin(0)`
  （物理不许自动停）、端点先解析成节点对象再交引擎（否则物理 tick 抛 `TypeError`、
  画布冻死）、一次 update 一次 `graphData()` + reheat。
- 尺寸按**屏幕像素**配置（`nodeSizePx / nodeSizeStepPx / nodeTiers / linkWidthPx /
  linkWidthStepPx / linkStrongAt`），模块按**本 canvas 自己的高度** + 相机距离 + fov
  折成世界单位（用视口高度折算会把 2px 变成 4px——canvas 只是页面里的一块）。
- 取景：`autoFit`（默认 true）在每次新增节点后 `zoomToFit` 到整张图，并用**拟合后**
  的相机距离重算像素基准（重造节点球），所以 px 尺寸承诺在拟合后的画面上仍然成立；
  `fitPaddingPx` / `fitDebounceMs` 调留白与节流。`autoFit: false` 退回 k10r 的固定
  机位（`cameraDistance` + `cameraDirection`）。
- 其余可配置项：`linkOpacity`、`arrowPx`、`particles`、`charge`、`linkDistance`、
  `background`、`palette`、`libUrl`。
- 文字两层，分工写明：**常显标签**（`labels`，默认开）节点显示 `name`（缺省 id）、
  边显示 `kind`，画在 canvas 之上的 HTML 覆盖层里（`pointer-events:none`，每帧把世界
  坐标投影成屏幕坐标），字号/颜色/抬升距离可配（`nodeLabelPx`、`linkLabelPx`、
  `labelColor`、`linkLabelColor`、`labelOffsetPx`）；同屏重叠时后来的标签先藏、位置
  一变自己回来（`labelDedupe`），节点数超 `labelMaxNodes`（默认 150）整体只画不标。
  **悬停卡片**给详情（类型/权重/强度），单层、无边框、无阴影、**底不透明**——库自带的
  `.float-tooltip-kap` 外壳（内边距+圆角+底色）被模块注入的样式抹平，避免"套娃"观感。
  卡片底必须实色：页面底是浅色、标签是深色字，90% 透明会让它们透上来（实测整张卡被染成
  `rgb(39,46,63)` 而不是 `#0f172a`），读数字的地方不能有第二层字。层级上卡片压在常显
  标签层之上（`z-index` 2 / 1）：两层都是宿主里的绝对定位兄弟，标签层后插入，不给卡片
  更高的层级它就会被标签盖住（实测同一位置 98 px 标签字盖在卡上）。
- 示例帧 `examples/yaml/19.canvas.forcegraph.yaml`：canvas 绑槽 → 三帧整份快照
  逐步长出 crate / component / module 三层。

## 后果

- wire 只多一个可选键，**向后兼容**：旧帧没有 options 就是空配置，任何模块照旧工作。
- 加一个新渲染器 = 新模块 + 新 `url`/`options`，DSL 形状不变；核心永远不知道图。
- 代价一：options **没有模式校验**。键名打错只有模块的 `console.warn` 与"这项用了
  默认值"的结果，不是错误帧。这是把语义归模块的必然后果——模式若要在核心侧校验，
  就等于核心开始认识每个渲染器的词表。
- 代价二：`patch` 能写 `/options/...`，但需要重新挂载才生效（§3）。写进去了不生效
  是容易误判的形态，所以这一条必须留在文档里；它不是缺陷，而是与 url 同一规则。
- `stage`/KDL 侧零改动：新字段经 serde 推导自动可用（`canvas { url "…" options { … } }`）。