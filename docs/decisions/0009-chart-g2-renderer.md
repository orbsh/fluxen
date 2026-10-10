# ADR 0009: Chart 渲染器演进——G2 spec + 模块通道（去 eval 注入）

状态：已接受
日期：2026-10-01
仓库：fluxen

## 背景

chart.rs 的现状是两处债：载荷是 ApexCharts spec（类型式 API，不是图形
语法）；渲染是 `dom::eval("new ApexCharts(...)")` 字符串注入（ADR 0007
背景里点名"实现缺陷"，整改归属本 ADR）。选型约束（用户裁决
2026-10-01）：JS 库、支持 GoG 语法、体积不能太大、交互效果好。
poloto（纯 Rust SVG）后延——视觉质量不满意；vega/vega-lite 同轮否决。

候选实测（min+gzip，tarball/bundlephobia 双渠道核对）：

| 库 | gz | GoG | 交互 |
|---|---|---|---|
| uPlot 1.6 | 21 KB | ✗ 时序专用 | 缩放/brush 强 |
| Chart.js 4.5 | 67 KB | ✗ 类型式 | 插件生态成熟 |
| Observable Plot 0.6 | 125 KB | ✓ GoG | interaction 是文档明示弱项 |
| ApexCharts 4.5（现状基线） | 150 KB | ✗ | 良好 |
| **@antv/g2 5.4** | **321 KB** | **✓✓ GoG 正统重写** | **内置最齐（tooltip/slider/brush/zoom/legend）** |
| ECharts 6.1 | 359 KB | ✗ 自有 option | 全家桶 |

G2 v5 的 API 本体即图形语法（data/transform/scale/coordinate/mark/encode
的 spec 对象），spec 是纯 JSON——与 Accrete "纯数据 + 闭词汇表" 哲学同构；
AI 生产者的训练语料熟悉度也高（中文生态）。体积代价由 ADR 0007 的
按需加载模式消化：不进主包、静态服务/CDN、浏览器缓存一次即免费。

渲染验证（headless chromium 实测）：g2.min.js 从静态服务加载，
interval + stackY + color encode + tooltip/elementHighlight interaction
的 spec 渲染成功——`#c` 容器出现 canvas 元素、无 JS 异常。
实施阶段补充取证（2026-10-01，全部实测）：
- 契约与载荷层（node）：g2chart 四导出齐、宿主形态 CBOR（uint8/uint16
  头、UTF-8 长文本、空 map `{}`）经封装 cbor.js roundtrip 通过。
- 全链渲染（headful chromium + CDP，iframe 挂真实 UI wasm、/cli 广播
  chart 帧）：inline default 与 source 槽两条路径都出 canvas
  （CHART-OK 1280x960）——其间抓到并修复一个真缺陷：UMD `<script>` 的
  相对 src 按文档 base 解析（页面在 / 时 `./g2.min.js`→404），必须
  `new URL(..., import.meta.url)` 按模块自身解析。
- 交互目检（CDP Input.dispatchMouseEvent hover）：tooltip 出现，
  内容 `{b, y, 5}` 正确命中数据行——本 ADR 的"效果"证据链闭合。

## 决策

### 1. chart 的载荷 = G2 spec（纯数据）

`bind["value"]` 槽的 default/set/patch 内容直接就是 G2 spec 对象
（`{type, data, encode, transform, scale, coordinate, interaction, ...}`）。
核心不解析、不校验内部（同 Canvas：spec 是模块私有词汇）；流式更新
走 ADR 0005 指针（`patch path:/data/... op:append` 追数据行，
G2 侧 `chart.options(spec)` + `chart.render()` 差量重绘；v5 没有
`chart.update()`，实测见 docs/PLAN.md §13）。

组件分工不变（用户 2026-09-30 定位）：chart 仍是数据可视化专用组件，
"数据 + 定义 → 图"的语义不变——变的是 spec 词汇（ApexCharts → GoG）
和渲染实现（eval → 模块通道）。Canvas 不取代它。

### 2. 去 eval：chart 挂载复用 0007 模块通道

新增薄封装 ES module `assets/g2chart/index.js`（约 40 行）：导出
mount/update/resize/unmount 契约，内部动态 import g2 库、
`new G2.Chart({container})` → `chart.options(spec)` → render（update 也是
options + render，v5 没有 `chart.update()`）；
spec 载荷 = 宿主推来的 CBOR（解码后即 spec 对象）。chart.rs 的挂载
实现改为与 canvas.rs 同一条 import(url) 通道——`chart` 变体的默认
url 指向 `/assets/g2chart/index.js`（`Chart { url }` 字段可覆盖，
与 Canvas 同形，允许换渲染实现/版本固定）。

结果：dom::eval 的 JS 字符串注入路径从 chart 消失；宿主对 G2 零知识
（知识全在封装模块里，供应链上它就是一份版本固定的静态资产）。

### 3. 资产与体积

- `g2.min.js`（1058 KB raw / 321 KB gz，`@antv/g2` **5.4.8**——版本原先只写
  "固定"没写具体号，2026-10-10 按本机文件 sha256 反查补记）+ 封装模块进
  `crates/ui_leptos/assets/`，不入 git 的构建产物模式照 3dbrowser
  （.gitignore + 重建配方注释；容器镜像照该配方自动装配）；版本固定 + 同源部署，
  跨源 CDN 走 CSP/SRI 准则（ADR 0007 §3 同文）。
- 主包 wasm 体积不变（G2 是 JS 资产，不进 wasm）。
- 二期减重（不在本 ADR 范围）：G2 官方 tree-shaking 用法
  （define custom 只打包用到的 mark/transform）可把 spec 常用面子集
  压到 ~250KB 级——文档说法未实测；`g2.lite` 实测只省 29KB，无用。

### 4. 退役项

- ApexCharts 的 spec 形态 chart 载荷：全仓示例（08.apexchart.yaml）
  改写为 G2 spec；`assets/apexcharts.min.js`/`.css` 与 index.html 的
  全局 script 标签删除。08.chart.yaml（通用形态）保留为 e2e 驱动帧。
- mermaid 不在本 ADR（diagram 组件的同类整改将来同通道进行，独立事项）。

## 后果

- chart/diagram 的渲染实现从"eval 字符串注入"迁到"0007 模块契约"，
  核心依旧零渲染知识；chart 与 Canvas 在机制上同族、语义上分工
  （封闭 GoG spec vs 自由载荷），符合组件分工裁决。
- 流式 token 更新图表 = patch 指针进 spec 的 data 数组 + 同实例 options
  + render 重绘（模块 update 腿），与既有操作层零冲突。内容须落在数据槽
  上（写布局节点会重渲染整行、重挂模块宿主），见 docs/PLAN.md §13/§Conventions。
- wire 面：`Chart` 增加 `url` 字段（默认值 serde default 保持向后兼容
  ——旧帧不带 url 也能 decode）；spec 内容本就是 opaque Value，无 schema
  变化。
- KDL/stage 侧经 derive 自动可用；示例改写 08.apexchart → 08.g2。
