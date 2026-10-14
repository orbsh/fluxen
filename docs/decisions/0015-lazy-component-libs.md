# ADR 0015: 组件库按需加载——用到才加载

状态：已接受（2026-10-14 用户裁决后实施）
日期：2026-10-14
仓库：fluxen

## 背景

用户 2026-10-14 的规则：「js 的库要惰性加载，没有用到 editor 组件的时候不要加载」
「canvas 库好像也没有惰性加载？比如加载 splat 播放器的时候才加载 three.js」。

当时实测的三条加载路径（每条都有判据）：

1. **应用包本身是干净的**：`dist/index.html` + `ui_leptos-*.js` 里 `rg 'esm\.sh|three'`
   命中 0——模块的库从不进应用包，`assets/` 被 copy-dir 拷进 dist 不等于被加载。
2. **但惰性只到"节点"这一层**：宿主在 `NodeRef on_load` 就 `import(url)`，与载荷无关
   （ADR 0012 的"空载荷不占位"只把容器设成 `display:none`）；模块自己又在**模块顶层**
   拉库——splatviewer 顶层 `await import(three@0.170.0)` + `await import(GS3D)`，
   3dbrowser 顶层 `const runtime = init()` 立刻开始拉 wasm。于是"绑了槽、载荷还没到"
   的 canvas 宿主已经把 three.js 拉下来了（`examples/walkthrough.py` 正是
   "先 append 行、后 set 载荷"）。
3. **mermaid 是唯一真正跟页面走的库**：`index.html` 一行
   `<script src="/assets/mermaid.min.js">`，2666850 B 原始 / 771617 B gzip（本机
   `gzip -c` 实测），每次开页都下载并解析，哪怕整棵树里没有 diagram 节点。

ADR 0012 的"明确不做的替代方案"里正好有这一条（**"首次非空载荷才 mount"**），当时的
理由是"收益为零、风险不对等"——收益只算作"不占位"，而 resize 那条腿已经免费拿到。
本 ADR 改判的正是这个账：当"用到才加载"成为一条**独立诉求**（字节量、首屏、受限网络
下不要白拉 MB 级依赖、将来还要接代码编辑器这类更重的库），收益不再是零，而代价就是
当年点出的那件事（把挂载时机从 on_load 挪进载荷订阅）——本 ADR 把它做掉。

## 决策

**三层同一判据：库在"组件真的要用它"之前不下载。**

1. **宿主层**（canvas/chart 共用的 `mount_module`）：`import` + 契约检查 + `mount`
   从 `NodeRef on_load` 改由**首次非空载荷**触发。空载荷 = 零 import、零下载——
   ADR 0012 的"不占位"从容器样式延伸到模块加载。
   挂载标记仍是**唯一状态源**（写在宿主元素属性上，rebuild 重跑 on_load 不会重复
   发起）：无标记 = 从未发起过 → 发起；`"pending"` = 在途；`"true"` 且容器里还有
   模块写进去的内容 = 活着。
   - 护栏语义一处收紧：旧实现只挡 `"true"`，所以"进行中的挂载不会被 rebuild 重复
     发起"那句注释当时并不成立——`"pending"` 会被再挂一次。现在按注释的语义收：
     在途不重发。
   - 旧的"挂载成功但内容被原地重建抹掉（tachys 按视图重写宿主子节点）"路径保留：
     清掉标记 = 当作没挂过，看守会用当前载荷重挂。
2. **模块层**（3dbrowser / splatviewer）：库的下载从**模块求值**挪到 `mount()`——
   splatviewer 的顶层 `await import` 收进 `ensureLibs()`、3dbrowser 的顶层 `init()`
   收进 `ensure()`。模块契约不变（仍是 mount/update/resize/unmount 四个导出），变的
   只是"本文件被 import ≠ 库被下载"（宿主那一层已经把 import 推到载荷非空）。
3. **diagram 的 mermaid**：`index.html` 的 `<script>` 预加载退役，改成首次挂载时注入
   script——全局 promise 门控（多实例共享一次下载，浏览器按 URL 缓存），形状与
   g2chart 模块的 `ensureG2` 一致。mermaid 是 UMD 全局、走不了 0007 的模块通道
   （ADR 0009 §4 记的"独立事项"即此），注入 script 是它的等价物。

不新造构件、不在 wire 上加字段：判据（"空"是什么）沿用 ADR 0012 的 `payload_is_empty`
（`Null` 或零条目 map），显示形态仍是内联 `display: none`。

## 明确不做的替代方案

- **给 canvas/chart 加 `lazy`/`eager` 开关**：同一语义表达两遍，而且"要不要预加载"
  没有第二种正当答案——与封闭词汇表的取向相反。
- **只在模块侧收（顶层改 mount）**：不解决问题——"节点存在"仍然等于"import 模块"，
  空载荷的宿主照样把模块拉下来。
- **只在宿主侧收（模块顶层不动）**：宿主不 import 了，但只要 import 发生，库就在模块
  求值时被拉走；两道合起来才是"用到才加载"。
- **把 mermaid 包成模块走 0007 通道**：diagram 是原生组件、不是模块宿主；为一个 UMD
  全局引入一个模块包装，构件多于收益。
- **prefetch / `<link rel=modulepreload>`**：那改变的是"何时开始下"，不改变"要不要下"。

## 后果

- 首屏不再有 2.7MB 的 mermaid 无条件下载；出现 diagram 节点时仍是同一份文件、同一次
  下载（多实例共享）。
- `examples/walkthrough.py`：splat canvas 行到达时**零请求**；`intro_car` 载荷 set
  到达才开始 splatviewer → three.js + GS3D → truck.ksplat。3dbrowser 同源。
- chart 同源但时序有变化：带子仍在行到达时就位（尺寸来自 CSS，ADR 0009），**G2 库的
  下载推到 spec 到达**。这是本 ADR 有意接受的时序变化——"用到才加载"优先于"行到达即
  起下载"。
- 载荷永远是空 map 的宿主：永不下载、永不挂载，容器保持 `display:none`（ADR 0012）——
  这正是想要的语义；想拿"固定尺寸的空盒子"的生产者照旧发非空载荷（如
  `{"assets": {}}`）。
- **给未来组件立先例**：代码编辑器这类库（Monaco 等）必须骑 0007 的模块通道
  （`url` → ES module），不得进应用包、不得在 `index.html` 里 `<script>` 预加载；
  宿主那一层已经保证"节点存在 ≠ 库下载"，模块那一层保证"被 import ≠ 库下载"。
- 文档连带：ADR 0012 挂 Update 指向本 ADR（决策 3 的挂载路径与"明确不做"第 1 条被
  取代）；ADR 0009 §4 的 mermaid 条目结案。ADR 0007 正文按历史存档保留。
- 落地与门槛见 `docs/PLAN.md` §21。