# ADR 0016: 代码编辑器组件——Monaco 走模块通道，保存即上行

状态：已接受（2026-10-15 实施；门槛全绿——见 `docs/PLAN.md` §22）
日期：2026-10-14
仓库：fluxen

## 背景

用户 2026-10-14：「实现 editor 组件（monaco，代码作为参数，保存时发送提交事件）」。

用途：编程 Agent 的前端里，让"Agent 产出的代码"原地可读可改——审核、微调、一键回传，
不必切到本地 IDE。这正是上一条 ADR 0015（"用到才加载"）立判据时点名的场景：Monaco 是
体积大户，绝不能在页面加载时拉下来。

本机实测（2026-10-14）：

- `monaco-editor` 0.55.1 的 npm tarball 13 677 219 B（sha1 `e74c6fe5a6bf985b817d2de3eb88d56afc494a1b`，
  与 registry 一致），解包后 `min/` 16 MB（`dev/` 28 MB、`esm/` 31 MB、其余 6 MB）。
- `min/vs`（AMD 构建）的组成：`loader.js` 39 848 B、`editor/editor.main.js` 79 052 B、
  `editor.api-*.js` 3 669 759 B、`editor/editor.main.css` 308 989 B、`nls.messages.js.js`
  104 KB、各语言 tokenizer（散在根目录的 4–20 KB 文件，如 `python-*.js` / `rust-*.js`）、
  `assets/{editor,css,html,json,ts}.worker-*.js` = 252 KB / 1.03 MB / 693 KB / 383 KB / 7.01 MB。
- 首屏真正要下的（以 basic-language 的 python 为例）：`loader` + `editor.main` + `editor.api`
  + `editor.main.css` + tokenizer + `nls.messages` + `editor.worker` ≈ 4.5 MB 原始。
- 网络（本机直连）：**jsdelivr / unpkg 不通**（`http_code 000`，与 Dockerfile 记的 unpkg
  一样只能走代理）、**esm.sh 通**（200）、**npm registry 通**（能下 tarball）。

## 决策

1. **组件形态**：新增 `Editor { id, url, attrs: SizeAttr, bind }`（wire tag `editor`，
   `has_id = "true"`）。attrs 走尺寸族（与 canvas/diagram 同——编辑器必须有确定高度）、
   `url` 带 serde default 指向自家封装模块（与 chart → g2chart 同例，换渲染实现/CDN 时显式覆盖）。
2. **数据区（"代码作为参数"）**：`bind["value"]`，载荷 = `{"code": <源码>, "lang": <Monaco 语言 id>}`
   （inline default 或 `kind: source` 槽，流式/改稿走既有 set/patch 指针）。
   **不新增节点级字段**：模块只拿得到数据区，`lang` 若放节点字段就传不进去——这是
   ADR 0007 契约的形状约束，不是取舍。判据沿用 `payload_is_empty`：`{}`/`Null` = 尚无数据
   → 不占位（ADR 0012），`{"code": ""}` 是"有空编辑器"（map 有条目即非空）。
3. **渲染**：Monaco **走 ADR 0007 的模块通道**（不是原生组件、不进应用包、不在 index.html
   预加载），且库下载在 `mount()` 里（ADR 0015：模块求值不拉库）——`ensureMonaco()` 门控。
4. **保存上行**：封装模块在编辑器上绑 Ctrl/Cmd+S，回调里 `host.send("submit", CBOR({code, lang}))`
   → 走既有上行（`ctx.send(ev, Some(node_id), value)`），与表单 `kind: submit` 同一条腿、
   零新机制。事件名固定 `submit`（与 button/form 同词），不引入 bind 事件字段。
5. **交付形态：vendored AMD 子集**（推荐）。理由：本机 jsdelivr/unpkg 直连不通（用户真机
   也许走代理，但 fluxen 的 e2e 与容器镜像都不该依赖一条不通的路）；AMD 构建 + 同源 worker
   零配置（`editor.worker` 与 tokenizer 同源取，无 CDN worker 的 blob/CSP 折腾）；
   与 g2chart（`g2.min.js` 版本固定 + sha256 + 镜像装配）同一条既有先例。
   落地：`crates/ui_leptos/assets/monaco/`（gitignore，配方写进封装模块头注释 + Dockerfile
   从 npm tarball 解出并 **sha1** 校验）；本机实测 tarball 13 677 219 B /
   sha1 `e74c6fe5a6bf985b817d2de3eb88d56afc494a1b`（与 registry 一致），解出的
   `package/min/vs` 与本地 assets 树 `diff -r` 逐字节相同——本 ADR 记的数字就是装配件本身。
   **剪枝 = 只删四个 worker 文件** `vs/assets/{css,html,json,ts}.worker-*.js`
   （1.03 + 0.69 + 0.38 + 7.01 ≈ 9.12 MB；剩 ≈ 6.3 MB）：它们只在把 lang 设成
   typescript/css/html/json 时才会被请求，评审场景要的是 tokenizer 高亮。
   2026-10-15 实测修正原配方的另外半句：`vs/language/**` 那四个 contribution 与根下的
   `monaco.contribution-<hash>.js` 四块**删不得**——`editor.main.js` 的 define 头把
   `../monaco.contribution-*`（四块）连同 `../basic-languages/monaco.contribution`、
   `../workers-*.js` 写成了 eager 依赖，删掉即 editor.main 加载失败；它们本身只是
   2–5 KB 的默认值/config 块，重的是 worker 本体。留 `editor.worker`（核心 worker，
   plaintext/basic 语言走它）+ 全部 basic-language tokenizer。
   剪枝后的实测（2026-10-15，剪枝树 + 全新 profile 的 headless chromium，见 §22 门槛）：
   高亮照旧由 basic-languages 的 monarch 提供（`monaco.editor.tokenize` 出 `keyword.ts` /
   `attribute.name.css` / `tag.html` / `string.key.json` 等类型，DOM 里 `.mtk*` span 着色）；
   语言服务如预期不可用（A/B 同文档：worker 在时 ts/css/json 各出 2 条诊断 marker，剪掉后
   6s 内 0 条，而 `getTypeScriptWorker()` 仍 resolve——失败不经 API 暴露）。失败面**不在
   网络层**：worker 由 blob 壳 `workers-*.js` 内部 `importScripts` 拉取，页面资源表与 CDP
   Network 都是零 404；代价是每个剪掉的语言一条 `Uncaught NetworkError: … importScripts …
   failed to load` 加 Monaco 的一条 `Could not create web worker(s). Falling back to
   loading web worker code in main thread` 警告。还原：从 `monaco-editor-0.55.1.tgz` 解出
   `package/min/vs/assets/*.worker-*.js` 覆盖回 `assets/monaco/vs/assets/`。

## 明确不做的替代方案

- **esm.sh CDN 直拉 Monaco**（先例 = splatviewer 的 three.js）：镜像零字节、无构建，但
  ① Monaco 的 ESM 构建要自己接 worker（`new Worker(url,{type:'module'})` + esm.sh 转译
  的 worker 入口），配置面比 AMD 大一截；② 把渲染能力押在一条本机直连不通的 CDN 上，
  与 fluxen 容器镜像"自包含可复现"的取向相反。备选保留：`url` 是节点字段，换 CDN 只是
  改一条帧。
- **原生组件 + eval 注入 Monaco**：04-02 已把 chart 从 eval 注入迁到模块通道（ADR 0009），
  编辑器更重，没有回头的理由；而且模块通道天然满足 ADR 0015 的加载判据。
- **`code`/`lang` 做成节点级字段，由宿主合成载荷**：等于在 `mount_module` 的载荷闭包里
  加一层变换机制（槽值也要包），为新组件破一次"载荷 = 数据区原样"的不变式。
- **保存走新的 bind 事件字段**（`bind.submit.event`）：button/form 的 `submit` 已经证明
  固定词够用；加字段就是同一语义表达两遍。
- **diff 视图（side-by-side）**：本次不做（用户只点编辑器 + 保存）；渲染器是 Monaco，
  将来加 `bind.diff` 之类是新决定，不预留字段。

## 后果

- wire 面：新增一个变体（`type: "editor"`）+ 一个 crate 结构体；旧帧不受影响（serde
  未知 tag 本来就会拒，这与"加字段"不同）。
- 加载面（承 ADR 0015）：没有 editor 节点 → 一字节不拉；有节点但载荷未到（绑槽、空载荷）
  → 宿主零 import；载荷到达 → 才 import 封装模块，模块 mount 时才拉 Monaco。实测链路
  （2026-10-15，python 与 rust 两条路径）：`loader` → `editor.main` → 四块
  `monaco.contribution-*` + `basic-languages/monaco.contribution` → `editor.api-*` →
  `workers-*`（worker 壳）→ `editor.main.css` → tokenizer；**`editor.worker-*.js` 两条路径
  都没被请求**（核心 worker 只在用到它的功能时才拉）。
- 镜像面：`assets/monaco/` 由 Dockerfile 从 npm tarball 装配（版本 pin + sha1 校验），
  与 `assets/g2chart/g2.min.js` 同模式；镜像增大 ≈ 6.3 MB（剪枝四个 worker 后，
  实测 15 700 863 B − 9 118 165 B），浏览器只取用到的部分。装配步按同一口径剪（`cp -r min/vs`
  之后删这四个文件，并断言只剩一个 `*.worker-*.js`——glob 不再命中即构建失败），所以镜像里的树
  与本机剪过的 `assets/monaco/vs` 逐字节相同。
- 上行面：stage 控制台能看到 `<-` 块里 `event: submit` + 节点 id + `{code, lang}`；
  生产者（prism/gravity 之类）据此回写文件或提交评审。
- 与既有组件的关系：`mount_module` 共用（canvas/chart/editor 三个宿主一条路径）；
  editor 与 canvas 的差异只在 url 默认值与封装模块（同 chart 与 canvas 的关系）。
- 落地与门槛见 `docs/PLAN.md` §22。