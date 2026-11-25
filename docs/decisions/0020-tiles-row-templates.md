# ADR 0020: tiles 的行模板——selector 选壳，children 被行替换

状态：追记（代码既有行为，补文档；本条不改代码）。**节点名已从 `rack` 改成 `tiles`、模板字段已从 `item` 改成 `children`**（ADR 0031 §4：`window` 说显示几行、`step` 说谁在推进）；`fold`／`pages`／`table` 也都不存在了。
日期：2026-10-28
仓库：fluxen

## 背景

`tiles` 的行模板是全仓唯一的「一份版式给所有行复用」的机制，但它没有写过文档——只
存在于 `crates/ui_core/src/components/tiles.rs` 和示例 yaml 里。ADR 0019（grab／dock）
把它当成关键前提（模板不能承载每行的数据），于是先把它单独记下来，免得下一个人靠猜：
它到底是「一种模板类型」，还是「可以带骨架的壳」，还是「行插进槽里」。

按代码现状，答案是第二条的半截：**壳，且只有壳**。

## 决策（机制，按代码现状逐段）

1. **形状**：`tiles.children: Option<Vec<Accrete>>`——一串**普通节点**，不是特殊类型。它和
   `template` 那个根变体（`Accrete::template { name, data }`，ADR 0006 的纯槽位替换）
   没有关系，两个「模板」不要混。

2. **装载**（`impl From<Vec<Accrete>> for ItemContainer`）：遍历这串节点，带
   `attrs.selector` 的进索引（键就是那个 selector）；不带 selector 的当**默认**，多个
   默认**后者覆盖前者**（前面那个被丢掉，不报错）。空或字段缺席 = 没有模板。

3. **选行**（`ItemContainer::select`）：行节点用**自己**的 `attrs.selector`
   （`Classify::get_selector` → `attrs.selector`）去命中索引；没命中用默认；默认也没有
   → **不套模板**，直接把行节点交给 `render_accrete`。

4. **替换**：命中的模板先**克隆**（`select` 返回 clone，逐行一份，行与行之间不共享可变
   状态），再 `template.set_children(vec![row])`，最后渲染模板。于是模板**除 `children`
   之外**的一切都留着——类型、`attrs`（class／selector）、以及该类型自己的非 children
   字段（例如 `window`／`step`）——而 `children` 被整段换成**恰好一个**行节点。

5. **深度在前面，不在模板**：行节点可以是任意深的子树；要在一行里放几件事（文字、grab、
   图标），它们得在**行节点内部**（行自己是个 `frame`）。模板层不承载版式内部结构。

6. **行身份是行自己的**：`tiles` 的 keyed diff 用行节点的 `id`（无 id 退 `#{idx}`），与模板
   无关——模板上写 `id` 不会成为行身份，而且模板在模板数组里只有一份，本来也不该带身份。

7. **坑：叶子类型当模板会静默吞掉行**。`set_children` 由宏按结构体字段生成
   （`accrete_macro/src/ops.rs`）：只有带 `children` 字段的类型才真的写
   `self.children = Some(...)`；`text`／`input`／`button`／`image`／`path` 这类
   没有该字段，方法在它们是**空实现**。所以把叶子类型当模板，渲染结果是「壳出来了、
   里面空白」，不报错、不 warn。模板必须是带 `children` 的容器。

8. **别的容器不吃这一套**：`fold` 的折叠头、`pages` 的 id 字典（ADR 0010）、`table` 一族
   的行结构当时各有各的做法；`fold`／`pages`／`table` 今天都不存在（ADR 0030、0031 §4），
   行结构只剩 `tiles` + `entry` 这一条路。

实例：`examples/yaml/01.chat_layout.yaml` 的 `tiles` 有两个模板——带
`selector: ask` 的 `frame`（靠右、accent）和不带 selector 的默认 `frame`（靠左）；行节点用自己
的 selector 决定落在哪个气泡里。

## 后果

- 「模板能带自己的版式」不成立：想在壳里放一个固定标题、让行接在它后面，这个机制做不到
  ——壳的 `children` 会被整段替换。正解要么让行节点自己带这层，要么把那个固定部分放到
  `tiles` 的兄弟节点上。
- 「模板写错成叶子类型」没有任何反馈，这是本条最该补一道门的地方：断言每个行模板的
  类型有 `children` 字段（宏生成的信息在编译期已知，运行时也可按 `borrow_children()`
  是否为 `Some` 判）。
- 复用只在壳这一层：每行的内部结构随行送一遍。对信息流这种「每行内容本来就不同」的场景
  没有额外成本；对「一列结构完全相同的简单行」则比理想形状肥一点。

## 明确不做的替代方案

判据，不是历史记录（当时没有留下取舍过程）：

- **模板里留一个占位符，行填进占位符**：等于给「行插在哪」再发明一套寻址语法（哪个占位、
  能不能有多个槽、槽和 `source` 的槽怎么区分），而现状的规则只有一条——模板的
  `children` 就是行。少一条语法，多一个约定。
- **模板可以带 `children`，行追加在后面**：壳能有自己的固定内容，但「哪些是固定、哪些是
  行」立刻失去判据：要么全当追加（那模板只能是纯壳），要么得加标记（回到上一条）。
- **给行模板定义一个专门的类型**：accrete 的一个新变体只为 `tiles` 服务，而它要能装的
  东西就是「任意容器」——等于把每一个容器类型复制一份。