# ADR 0003: 下行消息增加 ev 操作通道

状态：已接受
日期：2026-09-25
仓库：fluxen

## 背景

`Message<Brick>` 下行帧目前只有 `sender` / `content`。UI 的 `dispatch_msg`
对所有到达的帧一律执行渲染操作（create 重绘 layout、set/join 写数据槽），
没有"这条消息是给渲染器的还是给别的消费者的"这一维度。

两个约束促使加一个顶层操作通道字段：

1. 示例与广播消息需要固定归属一个操作类别。stage mirror 的 `/send`、
   examples 里的模板，本质都是"发给渲染器"；将来 gateway 转发时混流
   其他指令，渲染器不能照单全收。
2. 前端扩展点尚未定形（插件、快捷键、通知之类的消费方式没想好），但
   过滤机制可以先就位：不认识的操作通道一律不执行渲染，留日志即可。

命名上选择 `ev`（与 `Influx.event` 区分：后者是数据槽名，如 `login`、
`chat`，属于渲染操作内部的寻址；`ev` 是消息级的操作类别，两者平面不同）。
渲染操作类别的值定为 `draw`——比 `render`/`view` 短，且不与既有词汇冲突。

## 决策

`Message<T>` 顶层新增必填字段 `ev: String`，与 `sender`/`content` 平级：

```rust
pub struct Message<T> {
    pub ev: String, // 操作通道："draw" = 渲染类；其余值渲染器忽略
    pub sender: Session,
    pub created: Option<Created>,
    pub content: Vec<Content<T>>,
}
```

- UI `dispatch_msg` 先判 `ev`：非 `"draw"` 记 debug 日志后直接返回，
  不触碰 layout/data/list 槽。
- stage `parse_kdl_to_frame` 与一切生产消息的工具固定填 `ev: "draw"`。
- `Influx.event` 语义不变（数据槽名）。
- CBOR/JSON codec 无特判——serde 结构变更即线上格式变更，`ev` 为必填，
  旧帧（无 ev）解码直接报错，错误信息透传到 UI 的 decode failed 日志。

## 后果

- wire 契约破坏性变更：Message 构造点（stage/proto.rs、content/tests、
  将来 gateway）都要补 `ev`。当前无外部生产端依赖旧形状，改得起。
- 渲染操作内部（create/set/join 的寻址与 merge 语义）完全不动。
- 扩展通道从"改 Message 结构"变成"新增 ev 值 + 新消费者"，结构冻结。
