# ADR 0029: The native forcegraph is gone — the 3D renderer is a url

状态：已接受
日期：2026-11-18
仓库：fluxen
取代：ADR 0018（原生 `forcegraph` 节点）

## 背景

同一张三维力导向图曾经有两个实现：

    crates/forcegraph            4884 行 Rust + three-d 0.19，wasm 里渲染、拾取、布局
    assets/forcegraph/index.js   3d-force-graph（vasturiano），ADR 0007 模块契约

ADR 0025 立的尺子在这里是能用的：**一个种类住在数据里，条件是框架既不为它选实现，
也不替它定义词表。** 原生节点两条都犯了——`accrete` 里长出了 `ForceGraphStyle` 的
二十几个字段（`node_size_ratio`、`link_shape`、`highlight_reach`……），而它选的实现
是"wasm 里那套 three-d"。加一个可视化因此要改 fluxen、重新编 wasm；换一种画法不可
能只换一个文件。

## 决定

删掉原生实现：`crates/forcegraph`（整个 crate）、`accrete` 的 `ForceGraph` /
`ForceGraphStyle` / `LinkShape` / `Theme` 与 enum 变体、`ui_core` 的组件与 dispatch
臂、workspace 依赖。只留 `assets/forcegraph/index.js`，用 `canvas` 节点走 url：

```json
{"type": "canvas", "url": "/assets/forcegraph/index.js",
 "attrs": {"width": "100%", "height": "100%"},
 "options": {"nodeSizePx": 4, "linkWidthPx": 2.5, "linkOpacity": 0.85, "fitPaddingPx": 56},
 "bind": {"value": {"kind": "source", "source": "scene"}}}
```

## 代价（照实记）

Rust 实现有两项能力模块没有，**不是重命名，是消失**：

- `highlight_reach` / `reach_hops`：外部高亮的"可达路径"模式。模块只有按 kind 配色。
- `link_shape`：`axes` / `tetra` 的节点朝向。模块的边一律直连。

另外原生实现把拾取与上行事件放在 wasm 里；现在它们归模块的 `options` 与模块自己的
事件定义。这两项若回来，走的是模块加键，不是框架加字段——那就是 ADR 0025 想要的方向。

## 后果

- 示例 `20.forcegraph.yaml`（整篇在讲"原生 vs canvas 的结构区别"）删除；`00.main.yaml`
  的左栏改用 canvas。
- 三维场景的渲染细节（标签层、配色、粒子、UMD 求值绕开 AMD 的坑）都留在模块注释里，
  框架侧不再持有任何一份。
