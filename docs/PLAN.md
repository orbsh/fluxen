# PLAN

决策在 `docs/decisions/`（ADR 0003 … 0053）；本文件是索引——什么时候落了什么、什么还开着、宿主必须知道什么。

## 执行中的计划（2026-09 … 2026-11）

- [x] 0 — brick → accrete 更名 — ADR 0004 — 2026-09-26
- [x] 1 — `ev` 操作通道 — ADR 0003 — 2026-09-26
- [x] 2 — KDL 解析换到 kdl 6.7.1（真 KDL v2 语法）— 2026-09-26
- [x] 3 — YAML 载体并存，保留全表达力兜底 — 2026-09-26
- [x] 4 — 示例迁移：fluxora 的 `data/message/*.yaml` 进 `examples/yaml/` — 2026-09-26
- [x] 5 — nushell 发送工具 `examples/fluxen.nu` — 2026-09-26
- [x] 6 — 操作层：`Value` 与 `Patch` — ADR 0005 — 2026-09-30
- [x] 7 — Canvas 组件：外部渲染模块原位挂载 — ADR 0007 — 2026-10-01
- [x] 8 — 移除 minijinja，template 降为纯槽位替换 — ADR 0006 — 2026-10-01
- [x] 9 — bind 本地路由：Local 落点与值平面订阅 — ADR 0008 — 2026-10-01
- [x] 10 — Chart 渲染器：G2 spec 与模块通道 — ADR 0009 — 2026-10-01
- [x] 11 — Pages 组件与统一事件形状 — ADR 0010 — 2026-10-02
- [x] 12 — 远程 Vars 注入 — ADR 0011 — 2026-10-02；2026-11-24 撤除（`inject` 删除，ADR 0031 §2 由 ADR 0038 执行）
  - [ ] 页面状态快照/恢复 — 2026-10-02 暂缓（先不做）
- [x] 13 — 流式内容走订阅槽，并修好模块 update 通道 — 2026-10-02
- [x] 14 — 容器子节点级 Memo — 实测否证 2026-10-12：流式只走数据槽
- [x] 15 — 空载荷的模块宿主不占位 — ADR 0012 — 2026-10-10
- [x] 16 — Float overlay：背景模糊、不可操作 — ADR 0013，被 ADR 0014 取代 — 2026-10-10
- [x] 17 — 聊天框自动滚动：只在已经位于底部时跟随 — 2026-10-10
- [x] 18 — Canvas 模块的加载过渡 — 2026-10-10
- [x] 19 — custom CSS 层排在最前 — 2026-10-10 已修
- [x] 20 — layer + portal：模态层渲染到哪里 — ADR 0014 — 2026-10-12
- [x] 21 — 组件库按需加载：宿主 import 时机、模块求值、mermaid 预加载 — ADR 0015 — 2026-10-14
- [x] 22 — editor 组件：Monaco 走模块通道，保存走上行 — ADR 0016 — 2026-10-15
- [x] 23 — `append` 的落点参数 position — ADR 0005 — 2026-10-16
- [x] 24 — 页面级上行把手：入口读 `data-path` / `data-codec`，对外发 `window.fluxen.send` — 2026-10-19
- [x] 25 — 渲染器的发布产物：一份页面能直接引用的 UI bundle（tag `v*` → release 资产）— 2026-10-20
- [x] 26 — Canvas 的模块选项与 forcegraph 变体 — ADR 0017 — 2026-10-23
- [x] 27 — 删 `inject`：op / 消费臂 / 同值守卫 / 测试 / 示例帧 / wire 文档行 — ADR 0031 §2、0038 §1 — 2026-11-24
- [x] 28 — 页面选项就是槽：挂载选项的 `config` 指定槽名（默认 `__CONFIG__`），选项对象是节点的载荷（`source.default`）；`sink` 带 `path` 写进 local 面并按成员合并，读侧按成员回落；`config` op / `Ctx.config` / `set_config` / 每个选项一个 signal 全部退役 — ADR 0038 §2 — 2026-11-24
- [x] 29 — 皮肤：`config.skin` 镜像到 `<html data-*>` + roughr 实时画边（frame / panel / loom；帧自己声明 `is-border` 才挂）— ADR 0038 §3-4 — 2026-11-24
- [x] 30 — 开关落在控件本身：`bool` 字段读自己写的槽、change 即出口（文本字段仍以 Enter 为出口），复选框读 `checked()` 而非 `value()`；示例把它放进图的右下角（`panel place: anchor` 浮层，卡片向上开，占图的空边角），shell 不加任何栏 — ADR 0030、0038 §2、§5 — 2026-11-24
- [x] 31 — 字段没有自己的上行：无 tag 的 sink 只剩本地落地（remote/both 报警告），清空改由 loom 的 `reset: keep|clear|default` 决定；`sends: n` 每发一次减一、为 0 锁定（字段 disabled、拒发），缺省/-1 无限 — ADR 0032 §2、§6、§7 — 2026-11-24
- [x] 32 — 字段的 `sink` 整个删掉（shape 也不再接受，`Input`/`Select` deny unknown keys）：字段只剩 `source` + `kind`（number/bool/text/password，缺省 text，元素类型与解析同一处声明）；本地落地归 loom（`sink: {scope: local, path: /}` 把打包对象并进槽根，`write_local_at` 的根路径按对象合并）；例子里的 13 处字段 sink 全部改成 loom — ADR 0032 §2 — 2026-11-24
- [x] 34 — 嵌套 loom 的打包键只认 `tag`：`id` 是身份（按它匹配答复）、`dest` 是它自己发送的去向，两者都不是名字，`loom_key` 不再从 `id` 后备；无 `tag` 的内层 loom 不进外层的包（它的字段留在自己包里，自己有 `dest` 就自己发）；顶层 `tag` 不读 — ADR 0032 §4 — 2026-11-24
- [x] 33 — 读和写分成两个类型：`Source {slot, scope, path, default}` / `Sink {dest, scope, path}`（`dest` 与上行帧同名，`emit` 不再翻译；sink 没有 `default`，两个类型各自 deny unknown keys，`sink: {slot: …}` 解码即错）；例子/文档 29 处 `sink: {slot: …}` 改 `dest`，两条死 `default` 一并删掉 — ADR 0039（0031 §2 加 amend）— 2026-11-24
- [x] 32 — 皮肤的边角：`picker` 的层是盒子的子节点、行在它里面重建（换选项不重画）；`button` 的 `attrs.class` 接进来了（原来写死类名）；`input` 是 void 元素装不下子节点，所以画在上一层：`span.rough-host` 平时 `display: contents`、进皮肤后承载盒子和层。宿主是布局的一部分，两处曾错：它必须是**块级**的（inline 宿主会收缩包裹子节点，画出的盒子跟着子元素宽度走，于是"输入框宽度是固定的"），以及**开关不吃它被给的宽度**（`is-checkbox`：`flex: 0 0 auto` + `align-self: center` + 子元素 `1.15em`；此前 `bool` 字段带着所有字段共有的 `flex` 类，复选框长成整行宽、对勾浮在中间）。对勾也用同一支笔画（`path.check`，CSS 那份填色字形在皮肤下 `content: none`）— ADR 0038 §4 — 2026-11-25
- [x] 35 — `move` 动作：把一个**已经在**列表里的行挪到另一个位置，行本身不重建 — ADR 0005 加一节 — 2026-11-24
- [x] 36 — 节点改名 `bundle` → `loom`：`bundle` 只说"打包"，对"收集输入"与"字段按 `tag` 命名"两层都沉默，读起来又像是 `frame`/`panel`/`tiles` 那一类"装东西的盒子"；`loom` 一次说出三件事——线=字段、织法=`tag`→键、布=送出的对象，且织机天然可嵌套（ADR 0032 §4）。`bus` 否（Prism 自己就是总线、共享通路不属于哪一组、总线不嵌套）、`trunk` 否（本仓构建工具、git 主干、方向朝根而载荷离树） — ADR 0032 §1 — 2026-11-25
- [x] 37 — 字段的时钟：`input.emit` = `{debounce: ms}` | `{throttle: ms}`，闭枚举互斥，缺省 `{debounce: 250}`；`{debounce: 0}` 回到"每键一帧"。被定时的只有"通知 loom"，字段自己的显示信号照旧每键同步写，所以打字不等窗口；`submit` 先取消挂起的窗口再打包（Enter／按钮永远送屏幕上的值），`reset` 也取消（不让旧值复活） — ADR 0032 §8（§3 加"两个 owner"段）— 2026-11-25
- [x] 38 — 传输钩子 + 活着的 socket：`window.fluxenTransport` 在 mount 前读一次（形状不对 = 挂载错误，不回退），默认路径换成带心跳与重连的 `LiveWs`，整条策略是挂载调用里的一个 `policy` 对象 — ADR 0040 §3、§3.1、ADR 0044 — 2026-11-25
- [x] 39 — 一个门：页面用 `mount(element, options)` 启动，`data-*` 那套属性全部退役，随之消失的是"两个来源打架时谁赢"的整台机器；调用静默覆盖地址，值不可用就拒绝挂载、绝不降级；默认目标改成**我们的** id `#fluxen`（`#main` 是宿主的地标，不再当魔法 id）；`transport` 作为选项按值传，与 `window.fluxenTransport` 同一个 `hook::wrap`；产物从 bin 变成 lib（cdylib），`src/main.rs` 删除。真浏览器五页验证通过 — ADR 0045 — 2026-11-25
- [x] 40 — 一张 hooks 表、四个口子：`mount(el, {hooks: {open, close, create, slots, dests}})`；函数住 `thread_local` 表、`Ctx` 只拿 id（`js_sys::Function` 不是 `Send + Sync`）— ADR 0047 — 2026-11-25（单测待补）
- [x] 41 — `disc`：`oklch` 平面上的一个盘，角度=色相、半径=彩度，亮度由 `attrs.light` 钉住；值是 `"L C H"` 三元组，所以成员校验与宿主侧的 `oklch()` 合成都不知道有盘参与 — ADR 0048 — 2026-11-25
- [x] 42 — `picker` 的三种形状：`expand`（折起来）、`dir`（一排/一列）、`multiple`（打包成键的列表）；开合状态是渲染器自己的信号，不上线 — ADR 0049 — 2026-11-25
- [x] 43 — 两种字段节点的两种减词法：`input` 是一种行为的六种形式（`InputKind`，原样，不含区间），`picker` 是三种组件共用一个名字（`PickerKind`：菜单 / 滑块 `min`+`max`+`step?` / 开关），`kind` 缺席 = 菜单所以旧帧不动；区间只挂在滑块上，`min`/`max` 不可省、`step` 可空（空 = 连续）。数字框与滑块空着都打包 `null`，不是 0。`select` 更名 `picker`（`selector` 224 处 vs 节点名 8 处）。类型层已落；未落：滑块与开关的画法、`input` 内联 ±、`input.dir`、`picker` 的 `on`/`off`（2026-11-26 复核：`picker.rs` 里仍没有 range / switch 分支）— ADR 0050 — 2026-11-25，2026-11-26 落类型层
- [x] 44 — 图的图例是画出来的：G2 内置图例关掉，自绘 HTML 图例项（色块 + 名字）；名字点击隐藏该序列、色块点击出该序列的选项（形态 interval/line/area/point + 颜色 L/C/H）；模块把「一个 mark + color 编码」归一层成 `view` + 每序列一个 child，形态即 child 的 `type`、颜色即其 `style.fill`/`style.stroke` 定值（全都是柱时整层一个 `type`，见 46 条）；不加齿轮，坐标改由右上角一个显示当前值的图标按钮循环切换（名单**只来自** `options.coordinate`，帧不声明就没有按钮——同日改，见 46 条） — ADR 0051 — 2026-11-26；实测：两序列图例/形态/颜色/隐藏/坐标循环全通，雷达四图仍出画（11682→11420），`00.radar.yaml` + `08.g2chart.series.yaml` 两轮 e2e
- [x] 45 — 颜色字段改成方片 + 三滑块面板：`disc` 改名 `color`，行内是一块 16px 方片加可复制的 `oklch()` 字面量，悬浮/聚焦弹出包含头部在内的整块面板，`L`/`C`/`H` 各一滑块加一个数字输入框两向同步，拖动走字段自己的 `emit` 窗口；`attrs.light` 删除；值仍是 `"L C H"` 三元组 — ADR 0052 — 2026-11-26；实测：面板/双向同步/Esc/上行 `accent: 0.90 0.18 33` 全通，`cargo test --workspace` 绿
- [x] 46 — 多序列的柱子折回一层：全部可见序列都是 `interval` 且至少两个时并回**一个** interval child（`dodgeX` 分组 / `stackY` 堆叠，`scale.color` 带各自选过的颜色，隐藏仍是拿掉行），混合图（柱 + 线）不折；分组/堆叠是图级的（右上角第二个按钮，图标即当前态），拐角是序列级的（`line` / `area` 的 折角 / 平滑 → `style.shape = "smooth"`）；坐标每次显式写（`options()` 是 patch，删掉节点等于没改）；G2 内置图例每次 render 显式 `false` — ADR 0053、0051 §1/§3/§5/§6 — 2026-11-26；实测：坐标三轮三个画面、分组/堆叠/平滑三态的像素对照见「已修」
- [x] 47 — 聊天流的 tile 撑满列：`dir-col wrap` 的行按内容算宽，`ac-start` 让它不吃剩余宽度，于是 `as-stretch` 的 tile 撑的是那一行（730px 列里 120px 的图卡）；三份布局 yaml 改 `ac-stretch` — ADR 0024 补记 — 2026-11-26
- [x] 48 — 把"面"提升为框架原语：`.is-card`（`--surface` 填充 + 圆角 + `--card-pad` 内边距）进 `utilities`，皮肤下改由笔描边——正常模式靠色阶、线框靠笔，两套皮肤各自用自己有的工具分层；`border_layer` 认 `is-card`，`position: relative` 列表也补上（否则绝对定位的层落到页面上，三张卡被画成一条横线，实测过）— ADR 0038 §3-4 — 2026-11-25；`.round` 仍只在 `.is-border` 旁生效，暂不解绑
- [x] 49 — boot 不读页面 URL — ADR 0046：`codec` 与地址参数由宿主显式传入，`?token=` 在地址上不再生效（2026-11-26 复核：`settings.rs` 已按单门走、测试重写于 `174ed92`，`cargo check -p ui_leptos --tests` 0 错——这条原先挂着的 21 个编译错已经不存在）
- [x] 50 — 颜色只在 token 层：六个调色板名字（accent/primary/secondary 说"是什么"，disabled/warn/error 说"什么意思"）＋两个模式端点（fg/bg）是仅有的源；`--surface` 改成 `color-mix` 派生、`--wash`/`--ring` 取代三处裸灰、12 处 `var(--secondary)` 归位成 `--border-color`/`--wash`（`--border-style` 保留：写它的是宿主，`gate_settings` 第 8 步就是量它）；`--ink`/`--gloss` 作为两个绝对量（影子与高光不随主题）；图表模块 6 处行内 `rgba()` 换成注入的四个类规则读 token — ADR 0055 — 2026-11-26；`crates/ui_core/tests/tokens.rs` 把规则变成测试（切掉 token 两层后逐字扫描，`--bar` 的三条内容渐变按名放行）；随后这一面按主人切成两层：`@layer options` 排层序最前（十三个叶子 = 六色 + 形式四 + 字三，宿主 `@layer options { :root { … } }` 追加即胜），`@layer tokens` 只放端点与派生
- [x] 51 — 布局工具的类名就是属性加值：`ac`/`ai`/`as`/`jc`/`ji`/`js`（align-/justify- × content/items/self）后面跟 CSS 值自己的词 —— `jc-between`、`ac-stretch`、`as-baseline`；唯一的压缩是 `space-*` 去掉 `space`，因为没有别的值以 between / around / evenly 结尾。这六个类原先是 `jc-s-between` 这种缩写套缩写，其中两个还把 `evenly` 拼成 `evently`（类名错、声明的值对），全仓无一处使用所以一直没人发现 — 2026-11-26；`crates/ui_core/tests/utilities.rs` 把类名钉到属性与值上（把 `jc-between` 改回 `jc-s-between` 立刻红）

- [x] 52 — 静态源并成一棵树：`crates/ui-assets/assets/{core,custom}.css` 与 `loading.js` 移到仓库根 `assets/`（模块源码 `assets/modules/**` 旁边），服务目录 `crates/ui_leptos/assets/` 保持不变、这三处改指 `../../../assets/…` —— 服务布局与源码布局不是恒等映射（`/assets/g2chart/index.js` 来自 `assets/modules/g2chart/index.js`），所以不能整目录链接；`loading.js` 的服务 URL 仍是 `/assets/loading.js`，帧里存的地址一个都不用改。`favicon.ico`/`header.svg` 是零引用孤儿，直接删。`crates/ui-assets` 这个 crate 随之退场（它的 `manganis` 登记表自 dioxus 后端退场后就没有读者），两组样式表测试搬到 `crates/ui_core/tests/`。**不显然的耦合**：`Dockerfile` 的 `COPY assets/modules ./assets/modules` 必须改成 `COPY assets ./assets`，否则镜像里符号链接悬空、dist 静默没有 CSS — 2026-11-26

## 开着

- [ ] 图里的 canvas 不跟随暗色：坐标轴、图例标签的颜色来自 G2 的默认主题（浅色），页面切到暗色时轴上的字几乎看不见（2026-11-26 实测）。模块外的界面已经全部走 token（ADR 0055），画布里的这一层还没接——它要么读同一个 `--fg`，要么由帧指定主题，二选一，未定
- [ ] 模块实例在宿主重建或换布局后不进 `unmount` — 2026-10-23 实测，未修；2026-11-25 复核：`on_cleanup` 已按实例登记 `unmount(cid)`（`components/canvas.rs`），宿主重建时是否真的触发仍未测

## 约定（非代码）

- 流式行要带 `id`。
- 导航（以及任何同质分组）走数据；`frame` 只是静态骨架。
- 流式值只落在数据槽，永不进布局平面。
- 图的渲染器是场景层，边住在场景里 — ADR 0018 — 2026-10-26。

## 外部集成（2026-10-29）

- [x] `ui-core` / `boot` 拆分 — ADR 0021
- [x] 信封归宿主 — ADR 0021 决策 6
- [x] 线上槽位的词是 `slot` — ADR 0022
- [x] 上行目标叫 `dest` — ADR 0023
- [x] 传输是一个运行时钩子：`{ emit(bytes), subscribe(push) }`，挂载前读一次（`window.fluxenTransport`，或作为 `mount` 的 `transport` 选项按值传）、只搬字节（codec 仍归 Rust）、形状不对即挂载报错且不静默回退、心跳与重连归钩子（没有 `close`：socket 是钩子的，不是我们的）；默认路径仍是内置的 `LiveWs`，reflux/prism 不装钩子 — ADR 0040，ADR 0045 §2 — 2026-11-25

## 已修 — 这条路付过的代价

- [x] 聊天流的 tile 撑不满列：`tiles` 是 `dir-col wrap`，换行后的**行**按内容算宽，`ac-start` 让它不吃剩余宽度，于是 `as-stretch` 的 tile 撑的是那一行——730px 的列里挂出一张 98px 的图卡（两份都没写错，叠起来是错的）— 2026-11-26 修：聊天流的 `tiles` 改 `ac-stretch`（`00.main` / `01.chat_layout` / `01.default_layout`），实测图卡 120→710px、画布 98→688px，两个序列的柱并排出现（ADR 0024 补记）
- [x] 图表模块四件事一起坏：内置图例一直画着（`legend: true` 从未显式关过）、切回平面直角坐标不重绘（`options()` 是 patch，删掉 `coordinate` 节点等于没改）、两个序列都画成柱时叠成一根（分组与堆叠是层的 `transform`，每序列一个 child 里没有那个通道）、折线只有直段 — 2026-11-26 修（ADR 0053）；实测：坐标三轮切换三个不同画面且第三轮与初始逐像素相同，分组/堆叠画布哈希互异且切回一致，平滑与折角同像素数不同几何，`legend:false` 的 A/B 让画布顶部那块墨整块消失（9 行起密集色块 → 16 行起一条 6px 竖线）
- [x] 镜像构建死在 truck 场景那步，`curl: (23)`：`crates/ui_leptos/assets/` 自 `c57e5b8` 起整棵被 gitignore，干净检出里没有 `splatviewer/` 目录，`curl -o` 写不进去（同类坑 3dbrowser 那步已修过、这步漏了）；顺带补 `assets/modules/splatviewer/index.js` 的拷贝，原先镜像里只有场景、`/assets/splatviewer/index.js` 是 404 — 2026-11-26 修；实测：`git archive HEAD` 出的干净树 + 改好的 Dockerfile，`docker build` 全程通过（`cargo build -p stage` 48s、`trunk build` 出 dist），镜像 `0cbc86d`；CI 上 27 次全红（`0b5caae` 之后）应随之转绿
- [x] 颜色面板的三条腿一起坏：悬浮打开时 Esc 无效（keydown 挂在面板自己身上，而悬浮不聚焦），点外部关不掉（遮罩的 click 冒泡到方片的开关，关掉后立刻被重新钉上），`H` 拖到末端跳回起点（`rem_euclid(360)` 把 `360` 折成 `0`，值经 loom 回来又折一次）— 2026-11-26 修；实测无焦点 Esc 关、点外部关、点面板内部不关，`H` 停在 `360`，`gate_settings` 13/13（ADR 0052 §3/§6/§7）
- [x] trunk 0.22 拒绝 copy-dir 的绝对 href，`x start` 在装配阶段就死 — 2026-10-23 修
- [x] 文本节点第二次 `.class()` 抹掉第一次；markdown 正文没有排版规则 — 2026-10-12 修
- [x] 镜像构建的 glTF 拉取死在 `curl: (23)`：被 gitignore 的目录在 CI 里不存在 — 2026-10-12 修
- [x] rack 的自动跟随只在列表写入时触发，内部变长的一行永远不跟 — 2026-10-12 修
- [x] 模块 update 通道是死的：G2 v5 没有 `chart.update()` — 2026-10-02 修
- [x] 写一行会重建那一行并重挂所有模块宿主 — 对流式值 2026-10-02 已修
- [x] 发出的事件不带节点 id：wrapper 的 `id` 一直是空的 — 2026-10-02 修
- [x] Enter 什么也不做：console 把 CBOR 上行丢了 — 2026-09-30 修
- [x] 输入框回车即清空（dioxus 移植遗留）— 2026-09
- [x] 通知分发，走 per-key 槽 — 2026-09