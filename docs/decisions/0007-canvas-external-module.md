# ADR 0007: Canvas 组件——外部渲染模块的原位挂载

状态：草案（待确认）
日期：2026-09-30
仓库：fluxen

## 背景

fluxen 核心不允许集成固定渲染逻辑（用户裁决 2026-09-30）。3D 模型浏览器这类
能力（自带 GL context、自带事件循环、动态拉资产）不可能塞进 DSL 封闭词汇表，
也不能复制 eval 注入模式（chart.rs 现状的实现缺陷；它的整改属于 Chart 自身的
渲染器演进——换成 GoG spec + 别的渲染实现——与本组件无关，两件事不混）。

对比过 three-d / Bevy / three.js / deck.gl（见 wiki
`3d-rendering-library-selection.md`）：three-d 进主包净增 +341KB raw /
+94KB gz，且"模型浏览器"本质是独立小应用，不该由主包供养。结论是按需下载。

组件分工（用户定位 2026-09-30）：**Chart 是数据可视化专用组件**，不变量是
"数据 + 定义 → 图"——载荷格式会变（现在 ApexCharts spec，后续换图形语法类
spec）、渲染实现会变（eval 注入 → 其它），组件语义不变，Canvas 不取代它。
**Canvas 是自由度容器**，用于核心不理解其内容的能力（3D 浏览器、交互小应用），
自定义数据区的语义完全交给模块。

两条已核实的机制事实决定形态：

1. **wasm32-unknown-unknown 的 cdylib 之间不能互相链接**——每个模块有自己的
   线性内存和 allocator，跨模块只能传数据不能传指针。
2. **动态下载单元是 ES module**，不是裸 wasm：wasm-bindgen 产物 = glue `.js`
   （ES module）+ `.wasm` 字节码，`import(url)` 加载前者、后者由它内部 fetch。
   纯 JS 模块（three.js）走同一个 `import(url)`。因此接口对 JS/wasm 统一，
   组件名不带"wasm"字样——定名 `Canvas`（基底：canvas 元素 + 外部模块 +
   尺寸归 attrs）。

## 决策

### 1. 新增一个 Accrete 变体

```rust
Canvas {
    id: Option<String>,
    url: String,                          // CDN 上的 ES module 地址
    attrs: Option<ClassAttr>,             // 尺寸/样式走现有 attrs
    bind: Option<HashMap<String, Bind>>,   // "value" = 载荷槽（Chart 既有惯例）
}
```

wire tag `canvas`。不引入 `Custom { capability }` 抽象（用户否决：为想象中的
插件市场预付成本；数据可视化仍走 chart 类组件，与此无关）。

**data 区不新增寻址机制**：模块载荷 = `bind["value"]` 槽的内容（create 时的
default + 后续写入）。流式喂数据完全走 ADR 0005 现成协议：

```
set/patch { "event": "model-data", "path": "/assets/apple",
            "op": "append", "value": "https://cdn/…/apple.glb" }
```

命名槽 → JSON Pointer 进槽内 Value——路由、原子回滚、miss-warn-drop 全部
复用，Canvas 对操作层是零新增概念。模块在浏览器内解释 `data` 的语义
（`assets: {apple, banana}` 是 3dbrowser 自己的约定，核心不理解、不校验）。

### 2. 模块契约（宿主与模块唯一的相遇点）

URL 指向的 ES module 必须导出四个函数：

```js
mount(el: HTMLElement, data: Uint8Array, host) → ctx
   // el 是组件自己的 canvas 容器；data 是 bind["value"] 的 CBOR；
   // host = { send(event, bytes) } ——模块上行走现有动作帧协议（本期只留通道）
update(ctx, data: Uint8Array)   // 槽内容变化 → 宿主重序列化推入
resize(ctx, w, h)               // 尺寸归宿主：ResizeObserver 驱动
unmount(ctx)                    // leptos 视图销毁时必调；模块释放 GL/循环
```

- **边界传数据不传对象**：payload 是 CBOR（与帧链路同一 codec），模块用
  自己的 serde/JS 解，不存在跨模块类型 → 双内存硬约束天然满足。
- **多实例共享已加载模块**：浏览器模块表按 URL 缓存 `import()`，免费得到，
  无需 registry。url 变更 = 重新挂载（不做热替换）。
- **加载失败/导出缺失**：console.warn + 空容器——对齐 ev 通道的 fail-loud
  惯例，不猜、不降级。

### 3. 安全边界

- 动态 import 的模块在宿主传入 canvas/GL 句柄前接触不到渲染面（wasm 侧尤其：
  能力由宿主 import 中介）。JS 模块权限本就等同现状——Canvas 不引入新的
  攻击面类别，也不负责整改 Chart 的 eval 注入（那是 Chart 渲染器演进的课题）。
- 部署准则：模块与 UI 同源（stage 自家静态服务），URL 带版本号；跨源 CDN
  需 CSP `script-src` 白名单，可选 SRI 完整性校验（url 字段可扩展为携带
  integrity）。供应链风险由版本固定与自家镜像控制，不在组件层发明机制。

### 4. 本 ADR 明确不做

- **事件绑定**：`host.send` 通道留好，但 Canvas 暂不加 `bind` 事件映射字段
  ——还没想清楚 3D 场景会发什么事件（pick/hover/drop），只开门不设语义。
  后续打通 = 复用 bind slot 既有机制，一次小 ADR。
- **schema 导出**：`data` 归模块自定义，`stage schema` 不为 Canvas 导出
  payload schema（capability schema 汇总随 registry 方案一并否决）。
- **数据可视化按需 wasm**：图表差异是不同 spec（纯数据、本来就在帧里），
  不是不同代码；继续 chart/diagram 组件族，与本组件无关。

## 后果

- 主包体积不变；按需模块的成本 = 自带胶水基线（实测 526KB raw / 118KB gz）
  + 模块自身（three-d 全渲染面 +341KB raw / +94KB gz）。胶水按模块重复付费，
  所以模块按能力粒度切，宁少勿多。
- 渲染核心新增一个 `ui_leptos` 挂载实现：NodeRef on_load → import(url) →
  约定导出检查 → mount；bind["value"] 信号订阅 → update；ResizeObserver →
  resize；Owner drop → unmount。核心不含任何 three-d/glTF/ApexCharts 知识。
- KDL/stage 侧新变体经 serde 推导自动可用（`canvas { url "…" … }`）。
- 与 Chart 的关系：并存，各管各的载荷。Chart 的渲染器演进（ApexCharts eval →
  GoG spec + 别的实现）是独立议题，将来另立 ADR；本组件不吞并、不退役它。
