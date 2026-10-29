# ADR 0023: 上行目标叫 `dest`——UI 侧不再出现 `event`

状态：已接受（2026-10-29 用户裁决）
日期：2026-10-29
仓库：fluxen

## 背景

上行帧原来是 `Outflow { event, id?, data }`，而 `event` 这个词在同一个系统里有四个所指：

1. **DOM 事件**——页面本身到处是 `event`（`addEventListener`、`event.target`）；
2. **后端的事件**——aura 的 event/stream、prism 的 `{ev, args}`；
3. **信道的帧类别**——ADR 0003 的 `ev`（已随信封归宿主，退场）；
4. **上行落点**——页面把这次交互送到后端的哪个位置。

用户 2026-10-29 的裁决：「UI这边就不要出现event这个名字了，因为后端（例如Aura）本身就有event，
前端传来的数据里又有event就乱了，并且前端本身html中也有event。」

同日的第二条裁决否掉了"上下行共用一个词"的做法：「上下行的关系，包括数据形状都是不对称的，确实
不应该用同一个名字。」下行是往页面里的某个位置写（`slot`，ADR 0022）；上行是调起后端的一个处理器，
形状也不同（`{…, id?, data}` 封套 vs 各个 op 自己的字段）。既然不对称，就不该收敛成一个词。

## 决策

**上行落点叫 `dest`（destination）。**

1. `content::Outflow { dest, id?, data }`——一帧上行 = 送到宿主上的某个目标。
2. `accrete::BindVariant::Dest { dest }`（`kind: dest`）——控件的上行目标。
3. 与既有 `bind` 词法同一个模子：`kind` 说的是"这个字符串是什么名字"，`{kind: field, field: …}`、
   `{kind: source, source: …}`，所以 `{kind: dest, dest: …}` 不是新句式。
4. `inject` / `local` 里"另起的名字"叫 `name?`（缺省 = 槽名）：那里 `slot` 已经是通道，
   同一个对象里不能有两个 `slot`。
5. `event` 在 UI 侧彻底退场：bind 里没有、帧里没有、`ctx.emit` 的分支名也没有。

## 为什么是 `dest` 而不是别的

- 它只占一个含义：**发出去的东西落在哪儿**。与 `slot`（页面里的位置）并列而不重叠。
- 4 个字母，和 `slot`、`kind` 一样长，帧的字段列对齐。
- 被否掉的：`intent`（不像计算机领域的词，且一次未确认的自造改名）、`request`/`req`（暗示回执，
  我们的上行不保证有回应）、`command`（`name.change` 这类通知不是命令）、`topic`（订阅语义，
  bind 里没有订阅）、`func`/`call`（描述"调什么"，不如"送到哪"贴合）、`addr`（更像内存/套接字
  地址）、`sink`（和 `kind: source` 凑对漂亮，但 `local` 夹在中间破坏对称）、`path`（撞
  `Local.path`）、`hook`（reflux 的 `app.hooks` 已被页面钩子占）、`name`（`name?` 骑手已用）。

## 后果

- 后端读到的一帧里 `dest` 就是派发目标，与它自己的领域事件（aura 的 event）不同名。
- 破坏性变更，无别名：`kind: event`/`kind: uplink` 的布局与 `{"event": …}`/`{"slot": …}` 的上行帧
  一律失效（缺 `dest` 即缺必填字段，解码报错）。
- reflux 的注册表跟着改名：`app.dests`、`reflux/dests.py`、CLI `reflux dests`——它收的就是上行
  `dest`。命名先例是 `app.routes`（一张按键索引的表），不是 `app.uplinks`（那个词说的是帧的类别）。