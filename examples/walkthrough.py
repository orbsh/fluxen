#!/usr/bin/env python3
"""Fluxen walkthrough demo: ONE chat box, every element streamed into a data slot.

Steps (each waits for one Enter; --auto runs unattended, --delay paces the text):
  1  send 00.main.yaml      the two-column shell (left graph, right stream)
  3  inject `page`=chat     switch the visible page remotely (ADR 0011)
  4  append the ONE message row (9 children, every child bound to a slot) and
                            seed the four text slots
  5  TYPE the analytics intro
  6  send the chart data    the two charts draw into their bands (line chart +
                            the three radars — 00.radar.yaml's set frames)
  7  one patch per day      the line chart grows to 30 days
  8  TYPE the summary
  9  append the review table + form into the row's rack -> the form appears
 10  TYPE the car paragraph, then set the splat payload
 11  TYPE the shoe paragraph, then set the shoe payload
 12  TYPE the code-review paragraph, then set the editor's {code, lang} payload
     (Monaco is pulled at THAT moment — ADR 0015) and press Ctrl/Cmd+S to watch
     the uplink land on the stage console
 13  TYPE the graph paragraph, then grow the relationship graph in three
     snapshots — the same canvas host pointed at the forcegraph module, its
     renderer options (sizes, physics, palette) riding Canvas.options (ADR 0017)

The box in child order (what the reader watches fill in):

    intro text | line chart | radar group | summary text | review form |
    car text | splat canvas | shoe text | shoe canvas | patch text | code editor |
    graph text | graph canvas

Frames go to the mirror's POST /send, so no WS handshake is needed and the stage
console echoes each one. The typewriter is a per-token PATCH stream (3 chars per
frame, paced by --delay) — a paragraph costs no Enter per token, and a model or a
chart appears the moment its payload lands.

Implementation choices worth naming:

- ONE row, so ONE bubble. Every child is bound to a DATA slot — that is what lets
  a streamed write re-render only that child; patching the row itself would
  re-render the whole box and remount every module host inside it (docs/PLAN.md).
  An empty data area occupies NO space (ADR 0012), which is why the box opens
  holding nothing but the first paragraph and grows segment by segment.
- A chart has no size attrs (ADR 0009 — a chart's size always comes from its
  container), so each chart sits in a borderless container whose definite size
  comes from `attrs.grid`, the same shape `00.radar.yaml` uses. That band is part
  of the row from the start: unlike a canvas (where the size is on the node and
  ADR 0012 can collapse it), a chart's band cannot be derived from its data area,
  so it reads as an empty band until the spec lands and draws into it.
- The charts are slot-bound (`trend` / `radar_rd` / `radar_mkt` / `radar_ops`).
  `examples/yaml/00.radar.yaml` stays the single source of that chart data: this
  demo posts only its SET frames, because posting the whole file would open a
  SECOND box with the example's own row.
- The review table + form is static layout, not a module host, so it cannot be
  payload-gated: it rides in a rack (slot `review`) and is appended when its turn
  comes — an empty rack renders nothing and takes no space.
- Models land AFTER the paragraph above them; the trend keeps growing one day per
  frame (the walkthrough's original "chart grows" beat).
- The code editor is a module host like the canvases: it is in the row from the
  start, bound to its own slot, so it costs ZERO requests until the code payload
  lands (ADR 0015) — Monaco is ~4.5 MB and must never ride the page load. Its save
  is Ctrl/Cmd+S in the editor, which uplinks `{code, lang}` over the same leg the
  review form submits through (ADR 0016).
- The graph is the LEFT COLUMN, not a message (ADR 0018): a native `forcegraph`
  component with its own WebGL2 renderer — no JS asset, no `url`, no module
  contract, no three.js. Its knobs are typed DSL (`style`), not an opaque options
  bag, because the renderer is ours. Cumulative snapshots (not per-node patches)
  because one update = one physics push; it diffs by node id, so re-sending what
  is already there is a no-op, and an append never re-frames the camera. The
  final frame is a `focus` directive — the equivalent of "a search landed".

Prerequisites: crates/ui_leptos/assets/3dbrowser/ (with MaterialsVariantsShoe.glb),
crates/ui_leptos/assets/splatviewer/ (with truck.ksplat), and crates/ui_leptos/assets/editor/
+ crates/ui_leptos/assets/monaco/ (the editor wrapper + Monaco's AMD subset; recipes in
crates/ui_leptos/modules/editor/index.js and the Dockerfile). The graph needs no assets:
its renderer is compiled into the wasm bundle.
Run from the repo root (paths below are relative to it).

Usage:
  python3 examples/walkthrough.py [--host 127.0.0.1] [--port 3002] [--auto]

Ctrl-C quits.
"""

import argparse
import json
import sys
import time
import urllib.error
import urllib.request

TREND = "trend"                # data slot: the line chart's spec
RADARS = ("radar_rd", "radar_mkt", "radar_ops")   # data slots: the three radars
SUMMARY = "summary"            # data slot: the summary paragraph
REVIEW = "review"              # data slot: the rack the review form is appended to
ANALYTICS = "analytics_intro"  # data slot: the opening paragraph
SAY_CAR = "say_car"            # data slot: the splat paragraph
SAY_SHOE = "say_shoe"          # data slot: the shoe paragraph
SAY_PATCH = "say_patch"        # data slot: the code-review paragraph
CAR_SLOT = "intro_car"         # data slot: the splat scene's payload
SHOE_SLOT = "intro_shoe"       # data slot: the mesh model's payload
CODE_SLOT = "patch_code"       # data slot: the editor's payload {code, lang}
SAY_GRAPH = "say_graph"        # data slot: the relationship-graph paragraph
GRAPH_SLOT = "graph"           # data slot: the force graph's {nodes, links, focus?}

TREND_DATA = "/bind/value/default/data"     # pointer into the trend slot node
TEXT_VALUE = "/bind/value/default"          # pointer into a text slot node
RADAR_FILE = "examples/yaml/00.radar.yaml"  # the single source of the chart data

THREED_URL = "/assets/3dbrowser/index.js"
SPLAT_URL = "/assets/splatviewer/index.js"
SHOE_URL = "/assets/3dbrowser/MaterialsVariantsShoe.glb"
TRUCK_URL = "/assets/splatviewer/truck.ksplat"

# The two model areas inside the box. The node carries the size, and an empty
# data area takes no space (ADR 0012), so the area appears with its payload.
SPLAT_SIZE = ("720px", "460px")
SHOE_SIZE = ("640px", "420px")


# The editor area (ADR 0016). An editor is a module host with a definite size like a
# canvas, and its payload is {code, lang} — "the code as a parameter".
CODE_SIZE = ("100%", "280px")
PATCH_CODE = """pub fn render(nodes: &[Node]) -> Frame {
    // 改两笔再按 Ctrl/Cmd+S：整段代码连同语言 id 会从同一条上行通道回上去
    nodes.iter().map(Node::draw).collect()
}
"""

# The closing graph: the renderer's own pieces, i.e. what the reader has been
# watching work. Same 15 nodes / 14 links as examples/yaml/19.canvas.forcegraph.yaml
# (that file is the standalone carrier; this table keeps the demo free of a YAML
# parser). `weight` drives node size, `strength` drives edge width — both in the
# module, both configurable through options.
GRAPH_NODES = [
    {"id": "accrete", "name": "accrete", "kind": "crate", "weight": 9},
    {"id": "ui_leptos", "name": "ui_leptos", "kind": "crate", "weight": 8},
    {"id": "content", "name": "content", "kind": "crate", "weight": 7},
    {"id": "transport", "name": "transport", "kind": "crate", "weight": 5},
    {"id": "stage", "name": "stage", "kind": "crate", "weight": 6},
    {"id": "canvas", "name": "canvas", "kind": "component", "weight": 7},
    {"id": "chart", "name": "chart", "kind": "component", "weight": 6},
    {"id": "editor", "name": "editor", "kind": "component", "weight": 6},
    {"id": "rack", "name": "rack", "kind": "component", "weight": 6},
    {"id": "pages", "name": "pages", "kind": "component", "weight": 5},
    {"id": "forcegraph", "name": "forcegraph", "kind": "native", "weight": 7},
    {"id": "browser3d", "name": "3dbrowser", "kind": "module", "weight": 5},
    {"id": "splatviewer", "name": "splatviewer", "kind": "module", "weight": 5},
    {"id": "g2chart", "name": "g2chart", "kind": "module", "weight": 4},
    {"id": "monaco", "name": "monaco", "kind": "lib", "weight": 7},
]

GRAPH_LINKS = [
    {"source": "ui_leptos", "target": "accrete", "kind": "renders", "strength": 5},
    {"source": "stage", "target": "content", "kind": "encodes", "strength": 4},
    {"source": "stage", "target": "ui_leptos", "kind": "serves", "strength": 3},
    {"source": "transport", "target": "content", "kind": "carries", "strength": 3},
    {"source": "canvas", "target": "accrete", "kind": "node", "strength": 2},
    {"source": "rack", "target": "pages", "kind": "rows", "strength": 3},
    {"source": "canvas", "target": "ui_leptos", "kind": "mounted-by", "strength": 3},
    {"source": "chart", "target": "ui_leptos", "kind": "mounted-by", "strength": 3},
    {"source": "editor", "target": "ui_leptos", "kind": "mounted-by", "strength": 3},
    {"source": "forcegraph", "target": "accrete", "kind": "node", "strength": 3},
    {"source": "browser3d", "target": "canvas", "kind": "mounts", "strength": 4},
    {"source": "splatviewer", "target": "canvas", "kind": "mounts", "strength": 4},
    {"source": "g2chart", "target": "chart", "kind": "mounts", "strength": 3},
    {"source": "monaco", "target": "editor", "kind": "mounts", "strength": 5},
]

# (label, node count, link count) — cumulative snapshots for the LEFT column. The
# crate layer is already on screen (00.main.yaml seeds it), so the demo only grows
# it: components, then external modules. One frame = one update = one physics
# push; the component diffs by id, so a resent node is a no-op and an append never
# re-frames the camera.
GRAPH_BEATS = (
    ("组件层", 10, 9),
    ("外部模块层", 15, 14),
)

# The two chart bands. A chart is sized by its container (ADR 0009), so the band
# is a borderless container with a definite track height given through `attrs.grid`
# — the same mechanism 00.radar.yaml uses.
TREND_BAND = {"grid-template-rows": "260px"}
RADAR_BAND = {"grid-template-columns": "repeat(3, minmax(0, 1fr))", "grid-template-rows": "340px"}

# 3DGS scenes carry their own coordinate convention; the camera triple is the one
# from GaussianSplats3D's own demo page for this scene (demo/truck.html),
# otherwise the viewer's default ([0,10,15] looking at the origin, up=[0,1,0])
# frames the scene crooked.
TRUCK_CAMERA = {
    "up": [0, -1, -0.17],
    "position": [-5, -1, -1],
    "lookAt": [-1.72477, 0.05395, -0.00147],
}

# Days 16..30 of the simulated series — 00.radar.yaml ships days 1..15
# (09-03..09-17); these are the values the line chart grows with, one day per
# frame.
TAIL_DAYS = [
    ("09-18", 3.53, 3.53, 3.53),
    ("09-19", 3.64, 3.42, 3.77),
    ("09-20", 3.45, 3.38, 3.90),
    ("09-21", 3.28, 3.37, 3.67),
    ("09-22", 3.36, 3.51, 3.70),
    ("09-23", 3.55, 3.41, 3.80),
    ("09-24", 3.60, 3.45, 3.78),
    ("09-25", 3.77, 3.68, 3.77),
    ("09-26", 3.85, 3.46, 3.87),
    ("09-27", 3.92, 3.70, 4.03),
    ("09-28", 3.81, 3.65, 4.11),
    ("09-29", 3.57, 3.63, 3.95),
    ("09-30", 3.38, 3.41, 4.08),
    ("10-01", 3.20, 3.28, 4.03),
    ("10-02", 3.39, 3.20, 4.00),
]
DEPTS = ("研发部", "市场部", "运营部")

ANALYTICS_TEXT = """先看一组模拟数据：11 个人、3 个部门，每人每天按六个维度打分——出勤纪律、工作质量、工作量投入、协作沟通、学习成长、创新性（0–5 分），连续 30 天。下面第一张图是三个部门的日均分走势，第二张把每个部门的每个人叠在同一个六维坐标系里比形状；再附一张复核表，可以逐人改分并提交。"""

SUMMARY_TEXT = """总结：近 30 天三个部门的平均分整体稳中有升；研发部波动最大（3.20–4.07），市场部次之，运营部最平稳且收在高位，末尾两天三个部门同步回落。"""

CAR_TEXT = """先从体量大的看起。这台车不是网格模型——它没有面片、没有 UV、也没有贴图，而是几十万个高斯椭球按位置、尺度和朝向堆出来的（学术名字叫 3D Gaussian Splatting）。

可以把它想成一团"会发光的雾"：每个点自带颜色、透明度和方向，渲染时按视线方向排序，再把每个椭球投影到屏幕上混合成图像——不是先还原成三角形再算光照。好处很直接：它保留的是真实拍摄的观感。钣金的反射、车窗的厚度、边缘那圈高光，都会一次性带过来，不用人工建模、也不用猜材质参数。代价同样明确：文件大、要按视角排序、没法像网格那样随手改拓扑或者绑骨骼做动画。所以它现在的定位更像"把真实场景搬进浏览器"，而不是替代游戏里的网格资产。

这台车可以拖动围着转，滚轮拉近看细节。你注意到的远近层次是它自带的——这也是这类重建看起来比低模"实"的原因。"""

SHOE_TEXT = """换一件小的。这两件东西的共同点是——都不是靠参数表说服人，而是靠"看起来、转起来是什么样"。所以文字我只给必要的部分，剩下的直接给你一个能转的三维模型。

这双鞋解决的问题不是"让你跑得更快"，而是"跑完之后脚还能要"。一双鞋的价值很少体现在某一次能多跑两秒，更多体现在明天你还愿不愿意出门。中底是上下两层的密度配方：上层软，负责落地那一下的缓冲；下层偏硬，负责把力还给你。踩下去先被吃掉大半冲击，再把你推回地面，所以它不像纯软底那样"踩得深、起不来"。系带做的是快拉式，一只手一次收紧，戴手套也操作得了；鞋面是一整片针织，没有拼接缝线，长距离之后不会在脚背和脚趾外侧磨出硬边。鞋楦偏中性，前掌留了大概一个拇指的余量，宽脚也塞得进去。

下面就是这个模型：左键拖动绕它转一圈，看侧墙的支撑结构和鞋底的纹路；右键拖动是平移，滚轮缩放。"""

PATCH_TEXT = """最后一步换个方向：前面都在"看"，这一段是"改"。下面这块编辑器是 Monaco——VS Code 用的那个编辑器内核。代码不是嵌在页面里的：它和上面两件商品一样是随消息下来的一个参数，写进数据槽，编辑器订阅这个槽。

这意味着两件事。一是库的下载由载荷到达决定——编辑器这一段文字先到，Monaco 那四点几兆的库要等代码真的送上来才拉，所以"消息里有编辑器"不等于"首屏多下四兆"。二是上行同一条腿：改两笔、按 Ctrl/Cmd+S，整段代码连同语言 id 会沿复核表提交走的那条通道回给生产者，生产者据此回写文件或发起评审。

试着改一行——比如把 collect() 换成别的东西——然后按 Ctrl/Cmd+S，看 stage 控制台的 `<-` 块。"""

GRAPH_TEXT = """最后看一眼左边那张图——它一直在这儿，只是你没注意到。这一页的布局从第一秒就是这个形态：左边一张常驻的关系图，右边一条流。没有菜单，没有标签页，也没有频道列表。原来的界面把"去哪儿"交给分类，而分类是写完就不改的一次性判断；这里把"去哪儿"交给关系，关系是当下的——"和这件事相连的还有谁"，树状菜单天然答不了。

图里是这套东西自己：圆点是 crate / 组件 / 外部模块，线是"谁渲染谁、谁挂载谁"，点越大、线越粗，表示它在链条上的位置越重。有两处值得看一眼。第一，它不是外包给某个 JS 图库的：渲染、力布局、相机、拾取全在这套 wasm 里，所以旋钮进的是 DSL 的类型化字段（节点尺寸、边的宽度、力参数），不是透传给别人的黑盒配置——也因此没有 three.js 那 1.4 MB。第二，图是长出来的：刚才送的那几份整份快照按节点 id 求差量，组件一批批加进来，已经站好的点不会重排；最后一下是一次距焦——相机飞向那几个节点，别的没消失，只是退到后面。搜索落地时就是这么动的。

左键拖动转视角，滚轮缩放，点一个节点会往上回一条事件。"""


def text_node(slot):
    """A paragraph subscribing to a data slot. Bubble styling comes from the chat
    rack's own item template, not from this node."""
    return {
        "type": "text",
        "attrs": {"format": "md"},
        "bind": {"value": {"kind": "source", "source": slot}},
    }


def chart_node(node_id, slot):
    """`f` mirrors 00.radar.yaml: the chart is a grid item of the band container,
    and the container's track supplies the height."""
    return {
        "type": "chart",
        "id": node_id,
        "attrs": {"class": ["f"]},
        "bind": {"value": {"kind": "source", "source": slot}},
    }


def chart_band(children, grid, css_class=None):
    """A borderless container that gives its charts a definite size."""
    attrs = {"grid": grid}
    if css_class:
        attrs["class"] = css_class
    return {"type": "case", "attrs": attrs, "children": children}


def canvas_node(url, slot, width, height, options=None):
    node = {
        "type": "canvas",
        "url": url,
        "attrs": {"width": width, "height": height},
        "bind": {"value": {"kind": "source", "source": slot}},
    }
    # 模块私有配置走 Canvas.options（ADR 0017）：核心只搬字节，模块自己解释。
    if options is not None:
        node["options"] = options
    return node


def editor_node(node_id, slot, width, height):
    """A code editor bound to a data slot (ADR 0016). Same module-host contract as a
    canvas; what the module renders is Monaco, and the payload is `{code, lang}`."""
    return {
        "type": "editor",
        "id": node_id,
        "attrs": {"width": width, "height": height},
        "bind": {"value": {"kind": "source", "source": slot}},
    }


def frame_message():
    """The whole demo as ONE chat row: the nine children, in reading order."""
    return {
        "action": "append",
        "event": "chat",
        "data": {
            "type": "case",
            "children": [
                text_node(ANALYTICS),
                chart_band([chart_node("dept-trend", TREND)], TREND_BAND),
                chart_band(
                    [chart_node("radar-rd", RADARS[0]),
                     chart_node("radar-mkt", RADARS[1]),
                     chart_node("radar-ops", RADARS[2])],
                    RADAR_BAND,
                    ["gap"],
                ),
                text_node(SUMMARY),
                # the review form is static layout, not a module host: it rides a
                # rack so it can be appended when its turn comes (empty rack = nothing)
                {"type": "rack", "bind": {"value": {"kind": "source", "source": REVIEW}}},
                text_node(SAY_CAR),
                canvas_node(SPLAT_URL, CAR_SLOT, *SPLAT_SIZE),
                text_node(SAY_SHOE),
                canvas_node(THREED_URL, SHOE_SLOT, *SHOE_SIZE),
                text_node(SAY_PATCH),
                editor_node("patch-editor", CODE_SLOT, *CODE_SIZE),
                # The graph is NOT part of the stream any more (ADR 0018): it is
                # the left column of the shell, fed by its own slot. The row ends
                # with the sentence that explains it.
                text_node(SAY_GRAPH),
            ],
        },
    }


def frame_seed_text(slot):
    """Create the slot as an empty string — the first append needs a target."""
    return {
        "action": "set",
        "event": slot,
        "data": {"type": "text", "bind": {"value": {"kind": "default", "default": ""}}},
    }


def frame_set_payload(slot, payload):
    return {
        "action": "set",
        "event": slot,
        "data": {"type": "group", "bind": {"value": {"kind": "default", "default": payload}}},
    }


def frame_patch(slot, path, value):
    """Patch into a data slot (no row id: slots are addressed by name)."""
    return {"action": "patch", "event": slot, "path": path, "op": "append", "value": value}


def frame_review_form():
    """The review table + form — 06.table.yaml's shape (a table whose first column
    is editable, plus a submit button) with content that fits this demo. All three
    input kinds appear (number / bool / text), and one field carries a `payload`
    so the uplink is visibly more than the field value."""

    def th(t):
        return {"type": "th", "children": [
            {"type": "text", "bind": {"value": {"kind": "default", "default": t}}}]}

    def td(t):
        return {"type": "td", "children": [
            {"type": "text", "bind": {"value": {"kind": "default", "default": t}}}]}

    def row(field, kind, default, payload, name, dept, score, note):
        bindval = {"kind": "field", "field": field, "type": kind, "default": default}
        if payload is not None:
            bindval["payload"] = payload
        return {"type": "tr", "children": [
            {"type": "td", "children": [{"type": "input", "bind": {"value": bindval}}]},
            td(name), td(dept), td(score), td(note),
        ]}

    return {
        "action": "append",
        "event": REVIEW,
        "data": {
            "type": "form",
            "id": "review-form",
            "bind": {"value": {"kind": "event", "event": "review"}},
            "children": [
                {
                    "type": "text",
                    "attrs": {"format": "md"},
                    "bind": {"value": {"kind": "default", "default":
                        "复核（第一列可改，三种字段各一；点提交后字段值随 `review` 事件上行，stage 控制台可见）"}},
                },
                {
                    "type": "case",
                    "attrs": {"class": ["gap", "md"]},
                    "children": [
                        {
                            "type": "table",
                            "children": [
                                {"type": "thead", "children": [{"type": "tr", "children": [
                                    th("复核项"), th("姓名"), th("部门"), th("日均分"), th("备注")]}]},
                                {"type": "tbody", "children": [
                                    row("zw", "number", 4.13, None,
                                        "张伟", "研发部", "4.13", "后半月走低"),
                                    row("cj", "bool", False, {"dept": "市场部", "note": "波动最大"},
                                        "陈静", "市场部", "4.24", "波动最大"),
                                    row("sy", "text", "已复核", None,
                                        "孙悦", "运营部", "4.28", "收在高位"),
                                ]},
                            ],
                        }
                    ],
                },
                {
                    "type": "button",
                    "attrs": {"oneshot": False},
                    "bind": {"value": {"kind": "submit", "default": "提交复核"}},
                },
            ],
        },
    }


def chart_data():
    """00.radar.yaml's SET frames, verbatim — the example's own append frame stays
    behind (posting it would open a second box). A text-level split on the frame
    separator: no YAML dependency, so this runs on a bare `python3`."""
    text = open(RADAR_FILE, encoding="utf-8").read()
    at = text.index("\n- action: set")
    return text[at + 1:]


def type_out(mirror, slot, text, delay, label):
    """Stream `text` into `slot` the way an LLM streams tokens."""
    step = 3  # characters per frame — roughly one CJK token
    for i in range(0, len(text), step):
        chunk = text[i : i + step]
        r = send_json(mirror, frame_patch(slot, TEXT_VALUE, chunk))
        print(f"    [{label} {i + len(chunk)}/{len(text)}] {chunk!r} → {r}")
        time.sleep(delay)


class Mirror:
    def __init__(self, host, port):
        self.url = f"http://{host}:{port}/send"

    def send(self, body):
        req = urllib.request.Request(self.url, data=body.encode("utf-8"), method="POST")
        try:
            with urllib.request.urlopen(req, timeout=15) as r:
                return r.read().decode().strip()
        except urllib.error.HTTPError as e:
            return f"HTTP {e.code}: {e.read().decode().strip()}"
        except urllib.error.URLError as e:
            return f"unreachable ({e.reason}) — is `stage serve` running?"


def send_json(mirror, payload):
    return mirror.send(json.dumps(payload, ensure_ascii=False))


class Step:
    def __init__(self, auto):
        self.auto = auto
        self.n = 0

    def wait(self, title):
        self.n += 1
        print(f"\n[{self.n}] {title}")
        if self.auto:
            time.sleep(0.3)
        else:
            try:
                input("    ↵ 回车继续 … ")
            except EOFError:
                print("    (stdin closed — switching to --auto)")
                self.auto = True


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=3002)
    ap.add_argument("--auto", action="store_true", help="no Enter waits (fixed delay per step)")
    ap.add_argument(
        "--delay",
        type=float,
        default=None,
        help="seconds per typewriter frame (default 0.05 with --auto, else 0.07)",
    )
    args = ap.parse_args()
    delay = args.delay if args.delay is not None else (0.05 if args.auto else 0.07)

    mirror = Mirror(args.host, args.port)
    step = Step(args.auto)
    print("fluxen walkthrough — 打开 UI（如 http://localhost:3002?codec=json）后开始。")

    def push_file(fname):
        return mirror.send(open(f"examples/yaml/{fname}", encoding="utf-8").read())

    step.wait("发送 00.main.yaml")
    print(f"    send → {push_file('00.main.yaml')}")

    step.wait("整条消息入场（一个 box：引言 / 折线 / 雷达 / 摘要 / 复核表 / 车 / 鞋 / 编辑器 / 关系图，全部绑槽）")
    print(f"    append row → {send_json(mirror, frame_message())}")
    for slot in (ANALYTICS, SUMMARY, SAY_CAR, SAY_SHOE, SAY_PATCH, SAY_GRAPH):
        print(f"    seed slot  → {send_json(mirror, frame_seed_text(slot))}")

    step.wait("打字机：分析引言")
    type_out(mirror, ANALYTICS, ANALYTICS_TEXT, delay, "引言")

    step.wait("两个图表入场（发 00.radar.yaml 的 set 帧：折线 spec + 三个雷达 spec）")
    print(f"    send chart data → {mirror.send(chart_data())}")

    step.wait(f"逐日 patch 追加后 {len(TAIL_DAYS)} 天（折线长到 30 天）")
    for i, (day, rd, mkt, ops) in enumerate(TAIL_DAYS, start=1):
        body = [
            frame_patch(TREND, TREND_DATA, {"day": day, "avg": v, "dept": d})
            for v, d in zip((rd, mkt, ops), DEPTS)
        ]
        print(f"    [{i}/{len(TAIL_DAYS)}] {day} → {send_json(mirror, body)}")
        time.sleep(0.12)

    step.wait("打字机：摘要")
    type_out(mirror, SUMMARY, SUMMARY_TEXT, delay, "摘要")

    step.wait("复核表 + 表单入场（append 进这一行里的 rack 槽）")
    print(f"    append form → {send_json(mirror, frame_review_form())}")

    step.wait("打字机：第一件商品（泼溅车）")
    type_out(mirror, SAY_CAR, CAR_TEXT, delay, "车")

    step.wait("泼溅车入场（载荷写进槽，载荷到达时把 box 撑开那一段）")
    print(
        "    set payload → "
        f"{send_json(mirror, frame_set_payload(CAR_SLOT, {'assets': {'scene': TRUCK_URL}, 'camera': TRUCK_CAMERA}))}"
    )

    step.wait("打字机：第二件商品（鞋）")
    type_out(mirror, SAY_SHOE, SHOE_TEXT, delay, "鞋")

    step.wait("鞋的网格模型入场（载荷写进槽）")
    print(
        f"    set payload → {send_json(mirror, frame_set_payload(SHOE_SLOT, {'assets': {'shoe': SHOE_URL}}))}"
    )

    step.wait("打字机：待审的那段代码的说明")
    type_out(mirror, SAY_PATCH, PATCH_TEXT, delay, "代码说明")

    step.wait("编辑器入场（代码作为参数写进槽；Monaco 在载荷到达时才拉——ADR 0015）")
    print(
        "    set payload → "
        f"{send_json(mirror, frame_set_payload(CODE_SLOT, {'code': PATCH_CODE, 'lang': 'rust'}))}"
    )

    step.wait("在编辑器里改两笔、按 Ctrl/Cmd+S：stage 控制台出现 `<-` submit + 本节点 id + {code, lang}")

    step.wait("打字机：最后一段——关系图（这套东西自己长成一张图）")
    type_out(mirror, SAY_GRAPH, GRAPH_TEXT, delay, "关系图")

    for label, n_nodes, n_links in GRAPH_BEATS:
        step.wait(f"左图生长：{label}（整份快照写进 graph 槽——{n_nodes} 点 / {n_links} 线，组件按 id 求差量、不重排）")
        payload = {"nodes": GRAPH_NODES[:n_nodes], "links": GRAPH_LINKS[:n_links], "focus": None}
        print(f"    set payload → {send_json(mirror, frame_set_payload(GRAPH_SLOT, payload))}")
        time.sleep(0.8)

    step.wait("左图距焦：把发「搜索」那一下的等价物打出来——相机飞向 accrete/content/transport 这一簇")
    focus_payload = {
        "nodes": GRAPH_NODES,
        "links": GRAPH_LINKS,
        "focus": {"ids": ["accrete", "content", "transport"], "seq": 1},
    }
    print(f"    set payload → {send_json(mirror, frame_set_payload(GRAPH_SLOT, focus_payload))}")
    time.sleep(1.2)

    print(
        "\n完成。左边是常驻的关系图（可拖动旋转、滚轮缩放、点节点回事件），右边是消息流；"
        "折线 30 天，复核表可提交，编辑器可回传代码。"
    )


if __name__ == "__main__":
    sys.exit(main())