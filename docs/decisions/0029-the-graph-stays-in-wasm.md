# ADR 0029: The three-dimensional graph stays in wasm

状态：已接受
日期：2026-11-18
仓库：fluxen
修订：ADR 0025 的例外

## 背景

同一张三维关系图有过两个实现：

    crates/forcegraph            4884 行 Rust + three-d 0.19，wasm 里渲染、拾取、布局
    examples/modules/forcegraph 3d-force-graph（vasturiano）的 JS 模块，走 ADR 0007 契约

把原生实现删掉、只留 JS 模块的路走过一次，被推翻：那次删除按"种类住在数据里"（ADR
0025）处理，但这份图不是"一种可视化"，是这台机器的**观察面**——它的能力清单里有一半
是模块给不了的：

- 拾取与上行事件在 wasm 里，和渲染同一帧，不需要跨 JS 边界回传
- `highlight_reach` / `reach_hops`：从选中点向外 n 条边的可达路径高亮
- `link_shape`：`axes` / `tetra` 的节点朝向（边从节点的哪个面离开）
- 布局与相机在同一份代码里：`auto_fit` 只认第一批，之后的追加不重排、不飞相机

这三项不是配置差异，是"图作为界面"与"图画"的差异。JS 模块是后者。

## 决定

保留原生的 `forcegraph` 节点与 `crates/forcegraph`，删掉 JS 模块那一版
（`examples/modules/forcegraph/`、`examples/yaml/19.canvas.forcegraph.yaml`、
Dockerfile 里 3d-force-graph 的装配）。

## 与 ADR 0025 的关系

0025 说：种类住在数据里，条件是框架既不为它选实现、也不替它定义词表。`forcegraph`
两条都不满足——它就住在框架里。

这是**自觉的例外，不是疏忽**：框架内值得有一个需要 wasm 内拾取与布局的图，因为它
要当界面用；其余可视化（chart、editor、diagram）继续走 url。判据因此补一句：一个种类
可以住在框架里，前提是它提供的能力在模块边界之外（同帧拾取、可达高亮、朝向几何），
而不是"画得出来"。

## 后果

- `enum Accrete` 里有一个跟渲染器同名的变体，这是唯一一个，且它带着类型化 `style`
  （可被 schema / patch 校验）——新增另一种图仍然应该是 canvas 模块。
- 三维场景的渲染细节（标签层、配色、布局稳定规则）留在 `crates/forcegraph` 与
  ADR 0018 里。
