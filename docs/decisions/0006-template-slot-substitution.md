# ADR 0006: 移除 minijinja，template 降级为纯槽位替换

状态：已接受
日期：2026-10-01
仓库：fluxen

## 背景

minijinja 的引入先于 ADR 0005，当时刻意不预设立场（0005 第 98 行：
"另立 ADR（0006）"。现状：`Accrete::template { name, data }` 节点在
create/set/append 落地前经 `Accrete::expand` 渲染——模板本体是 Jinja
文本，`data` 是渲染上下文，产物是 Accrete JSON。

ADR 0005 之后重新审视，template 承担的两类活计已经分化：

1. **对已完成组件的局部修改**（token 流拼接、字段级编辑）——已被
   Patch 的 JSON Pointer 路径取代，模板不再是必要的寻址手段。
2. **结构复用**（一份骨架 + 每次不同的数据）——仍然需要，但真正被
   用到的只是 Jinja 的 `{{ var }}` 插值；控制流（`{% for %}` /
   `{% if %}`）只有 01.segment 一个示例在用，而那点逻辑本可由生产
   端在拼载荷时顺手做完。

为一个没人用的模板语言养一条 2.19 的依赖链（accrete 的 `template`
feature、ui_leptos、stage 的 feature 组合坑、wasm 体积），收益不成立。

## 决策

**移除 minijinja。template 语义收缩为纯槽位替换，wire 形态不变。**

### 1. 不变的部分（wire 面零改动）

- 注册：`Content::Tmpl { name, data: String }`（`action: tmpl`）——
  `data` 本来就是字符串，槽位文本直接放得下。
- 应用：`Accrete::template { name, data: Map<String, Value> }`——
  指明模板名和数据，与现有节点同形。
- KDL/YAML 载体、schema 导出、classify/ops derive 全部照旧。

### 2. 替换语义（expand 的全部新行为）

- 模板本体 = **带 `{{key}}` 槽位的 Accrete JSON 文本**（非严格 JSON，
  替换完成后才是）。
- 按值类型分派：
  - **字符串值 → 原样文本注入**（插值语义，槽位位于 JSON 字符串内部，
    同 Jinja `{{ x }}`——存量 `00.chat_layout` 示例的形状）。含引号的
    值会破坏 JSON：生产端的义务，替换层不转义。
  - **其余 JSON 值（对象/数组/数字/布尔）→ 紧凑序列化**注入，槽位占
    整个 JSON 节点位置（拼接子树/数组片段，`01.segment` 改造的形状）。
- 替换完成后 `serde_json::from_str::<Accrete>`；成功则替换节点，
  失败则**保留节点不动**。
- 不支持：`{% %}` 控制流、过滤器、默认值语法、`{{ a.b }}` 路径取值
  （值已经是完整 JSON，无此必要）。

### 3. 错误策略（修正存量缺陷）

现状 `expand` 渲染失败 `Err(_) => {}` 静默吞掉。新实现：未注册模板名、
替换后 parse 失败——`tracing::warn` + 节点保持 `template` 原形（渲染端
对未知变体的既有兜底展示）。对齐 ev 通道的 fail-loud 惯例。warn 打在
ui_leptos 调用侧（accrete 保持零日志依赖，`expand` 返回结果供调用方
决定）。

### 4. feature 门控删除

`template` cargo feature 的原有作用是隔离 minijinja 依赖成本。替换实现
零依赖，门控失去存在理由：`Accrete::template` 变体常开。连带删除
accrete 的 self-dev-dependency feature 组合、stage 的 `template` feature
列表项与对应 Pitfall（stage 需带 template 否则 KDL 拒绝模板节点——
常开后自然消失）。

### 5. 示例迁移

- `01.segment.tmpl.yaml`：`{% for %}` 构造 `children` 数组的逻辑上移
  ——body 改为 `{"type":"case","children":{{kids}},...}`，注册帧改写，
  `01.segment.render.yaml` 的 `data` 里由生产端给出完整 `kids` 数组。
- `00.chat_layout.yaml` / `12.placeholder.yaml`：`{{var}}` 插值同形，
  零改动。

## 后果

- 依赖删除：根 Cargo.toml、accrete（两处）、ui_leptos 的 minijinja；
  `crates/accrete/src/template.rs` 重写为 ~40 行纯 std + serde_json。
- `ui_leptos` 的 `static TMPL: RwLock<Environment>` 改
  `RwLock<HashMap<String, String>>`。
- 主包 wasm 体积下降（minijinja 不进 wasm 侧的程度未实测，方向确定）。
- 模板语言表面收窄是刻意的：fluxen 的哲学是封闭词汇表 + 生产端组装，
  模板引擎的控制流与这条原则相抵。将来若真需要循环复用，正解是 rack
  + data 槽（现成机制），不是给模板重新加语法。
- `stage schema` 不受影响（template 变体常开后 schema 反而更稳定，
  不再随 feature 组合变化）。
