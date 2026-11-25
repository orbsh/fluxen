# ADR 0025: A renderer is a url — the framework has no visualization kinds

状态：已接受
日期：2026-11-18
仓库：fluxen

## 背景

`accrete` 的节点表里曾经有五个可视化节点：`canvas`、`chart`、`editor`、`diagram`、
`forcegraph`。它们的能力分布是这样的：

| 节点 | url | 走模块契约 | 自家词表 |
|---|---|---|---|
| `canvas` | 必填 | 是 | — |
| `chart` | `#[serde(default)]` → `/assets/g2chart/index.js` | 是 | — |
| `editor` | `#[serde(default)]` → `/assets/editor/index.js` | 是 | — |
| `diagram` | 无字段 | 否（`dom::eval` 注入 mermaid） | — |
| `forcegraph` | 无字段 | 否（wasm-bindgen 直连） | `ForceGraphStyle`（17 个类型化字段） |

`chart`/`editor` 的实现只是 `canvas` 加一个默认 url；`diagram` 自己注入脚本；
`forcegraph` 的旋钮进了 schema 而 `canvas.options` 的旋钮不进——同一个角色
（外部渲染器），四种形态。

代价不是代码重复，是**种类住在框架里**：新增一种可视化要改 `accrete`（加
struct 与 enum 变体）、改 `ui_core`（加组件函数）、重新编译 wasm；而可视化
本来是最该在部署侧扩展的东西。默认 url 更直接：「`chart` 这个词汇」就等于
「用 g2chart 画 G2 spec」，换渲染实现要么改 url 要么改节点，两种都对，所以
这个词汇自己没有说清它是什么。

## 判据

**种类住在数据里，框架就只需要契约；框架一旦替某个种类设了默认值或定义了
词表，种类就住在框架里了。**

`canvas` 通过：url 必填（框架替你选不了）、`options` 是不解释的袋（ADR 0017）、
载荷语义归模块。其余四个不过。

## 决定

1. 可视化只有 `canvas` 一个节点：`url` + `source`（载荷，CBOR）+ `options`
   + `attrs`。`url` 必填——没有 url 的 canvas 是坏帧，解码失败，不是"合法的默认"。
2. 模块契约不变（ADR 0007）：ES module 导出 `mount/update/resize/unmount`。
3. 种类全部落到 assets：`g2chart`、`editor`、`diagram`、`forcegraph`（JS 版）
   各自一个 `index.js`，由部署装配。换渲染器是换 url。
4. 已删除：`Chart`/`Editor`/`Diagram` 三个 struct 与 enum 变体、
   `default_chart_url`/`default_editor_url`、组件 `chart_`/`editor_`/`diagram_`
   与 `components/diagram.rs`（内联 mermaid 加载器）。
5. 本仓新增 `assets/modules/diagram/index.js`：mermaid 作为模块实现契约
   （v10 `run()` / v9 `init()` 都支持，因为库版本归部署）。

## 迁移

    {"type":"chart"}   → {"type":"canvas","url":"/assets/g2chart/index.js"}
    {"type":"editor"}  → {"type":"canvas","url":"/assets/editor/index.js"}
    {"type":"diagram"} → {"type":"canvas","url":"/assets/diagram/index.js"}

`examples/yaml/08.chart.yaml`、`08.g2chart.yaml`、`08.diagram.yaml` 已按此改写。

## 后果

- 新增一种可视化不改 fluxen：写一个导出四个函数的模块，放在它能被取到的
  地方，`url` 指向它。
- 一次 `cargo check` 就能看出框架里还有没有种类：`enum Accrete` 里不该再有
  跟渲染器同名的变体。
- `forcegraph` 已单独裁决（ADR 0029）：保留 wasm 里的 `three-d` 实现，删掉 JS 模块
  那一版。它是本判据的**自觉例外**——同帧拾取、可达路径高亮、边的朝向几何在模块边界
  之外，不能靠 url 换实现。其余可视化仍走 url。
