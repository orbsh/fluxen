# 0046 — 地址的意见归宿主：boot 不读页面 URL

状态：Accepted（`lib.rs` 已按此走：boot 只拼装宿主交进来的地址参数，不再读 `document.location` 的查询串。`settings.rs` 的模块注释与测试仍停在旧形状——`cargo check -p ui_leptos --tests` 报 21 个错，待补）

## 问题

`crates/ui_leptos` 的 boot 从 `document.location` 读 `?token=` 与 `?codec=`（`query_of`，
`lib.rs:187`），并按"opts 优先、地址兜底"合并（`settings.rs:56`、`:89`）。两条推论都不成立：

- 它把**宿主的策略**放进了渲染器：token 从哪来（地址、存储、还是服务端签发时塞进页面）、
  codec 用哪个，是宿主与它的服务端之间的约定，渲染器对此没有意见可表；
- 它给了一个**半个实现**：读了却不存在回、不认 `ui:token`（`transport-ws` 无存储代码），
  于是"刷新还是同一台设备"这件事看起来像渲染器管、实际没人管。

## 决定

boot 只认 `mount(el, opts)`：

- `codec`、`token` 由宿主显式传入，缺省仍是 `cbor` / 无 token；
- 新增 `query: Record<string, string>`：宿主把要带在 WS 地址上的参数交进来（`path` 之外的那些），
  boot 只做拼接与转义，不解释键名；
- `query_of` / 对 `location` 的读取整体移除。页面地址从此是**宿主的**事实。

## 代价

- 宿主必须自己写那几行（读地址/存储 → 传参）。reflux 已经有这层（`www/static/reflux.js`），
  prism 的部署页面也要补——它本来就没有，所以这不是新增负担，只是把隐含的依赖显式化。
- 兼容性：`?token=` 在地址上不再生效。发布说明里要写这一条，并给一个版本窗口。

## 为什么不是"渲染器也管存储"

存储是宿主的会话策略（localStorage、cookie、内存、没有）。渲染器一旦碰它，就要回答过期、
多标签、跨域，而这些问题它没有上下文可答。**渲染器提供口子，宿主提供意见**——与 ADR-0040
（transport 是一个运行时钩子）、ADR-0045（页面用 mount 启动我们）同一条线。
