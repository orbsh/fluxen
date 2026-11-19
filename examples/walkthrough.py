#!/usr/bin/env python3
"""Fluxen walkthrough demo: ONE chat box, every element streamed into a data slot.

Steps (each waits for one Enter; --auto runs unattended, --delay paces the text):
  1  send 00.main.yaml      the two-column shell (left graph, right stream)
  2  append the ONE message row (13 children, every child bound to a slot) and
                            seed the six text slots
  3  TYPE the analytics intro
  4  drag the bubble's pin tile to the rail at the window's right edge — the
                            stage console shows the `pin` drop, `{id, tag}` (ADR 0034)
  5  send the chart data    the two charts draw into their bands (line chart +
                            the three radars — 00.radar.yaml's set frames)
  6  one patch per day      the line chart grows to 30 days
  7  TYPE the summary
  8  append the review table + form into the row's review slot -> the form appears
  9  TYPE the car paragraph, then set the splat payload
 10  TYPE the shoe paragraph, then set the shoe payload
 11  TYPE the code-review paragraph, then set the editor's {code, lang} payload
     (Monaco is pulled at THAT moment — ADR 0015) and press Ctrl/Cmd+S to watch
     the uplink land on the stage console
 12  TYPE the graph paragraph — the left graph has been growing all along: every
     beat above pushed a cumulative snapshot, so a chart appearing on the right
     also added its node on the left (ADR 0018)

The box in child order (what the reader watches fill in):

    intro text | pin tile | line chart | radar band | summary text | review form |
    car text | splat canvas | shoe text | shoe canvas | patch text | code editor |
    graph text

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
- The bubble carries ONE draggable unit (ADR 0034): a `frame` whose `attrs.draggable`
  says the tag `snippet`, with a small chip as its only child. The cargo IS that
  subtree, read from the node at dragstart — there is no second payload bag to keep
  in step with the text, so dragging costs the same whether the paragraph is ten
  characters or ten thousand. The landing zone is a `frame` with `accept: {tag:
  snippet}` and `sink: {slot: pin}` on the shell's right edge, so only snippet-tagged
  cargo can land; anything else is refused silently, which is the pairing rule
  working, not a failure. The source adds no style and no size of its own — what you
  see is the `text` child.
- The charts are canvases pointed at the g2chart module (ADR 0025) and carry no
  size of their own, so the size comes from the container (ADR 0009): each one
  sits in a borderless band whose definite size comes from `attrs.grid`, the
  same shape `00.radar.yaml` uses. The band is part of the row from the start —
  unlike the model canvases below (where the size is on the node and ADR 0012
  can collapse it), nothing here can be derived from an empty data area, so it
  reads as an empty band until the spec lands and draws into it.
- The charts are slot-bound (`trend` / `radar_rd` / `radar_mkt` / `radar_ops`).
  `examples/yaml/00.radar.yaml` stays the single source of that chart data: this
  demo posts only its SET frames, because posting the whole file would open a
  SECOND box with the example's own row.
- The review table + form is static layout, not a module host, so it cannot be
  payload-gated: it rides a `tiles` (slot `review`) and is appended when its turn
  comes — an empty slot renders nothing and takes no space. The table inside it is
  a `tiles` with `fields` (ADR 0031 §4), so a row is an `entry` in a row slot
  (`review_rows`), not a `<tr>`: there is no table/theader/tr/td family any more.
- Models land AFTER the paragraph above them; the trend keeps growing one day per
  frame (the walkthrough's original "chart grows" beat).
- The code editor is a canvas pointed at the editor module (ADR 0025): it is in
  the row from the start, bound to its own slot, so it costs ZERO requests until
  the code payload lands (ADR 0015) — Monaco is ~4.5 MB and must never ride the
  page load. Its save is Ctrl/Cmd+S in the editor, which uplinks `{code, lang}`
  over the same leg the review form submits through (ADR 0016).
- The graph is the LEFT COLUMN, not a message (ADR 0018), and it is not a finale:
  each beat above pushes the snapshot for what has appeared so far, so the two
  halves advance together. A native `forcegraph`
  component with its own WebGL2 renderer — no JS asset, no `url`, no module
  contract, no three.js. Its knobs are typed DSL (`style`), not an opaque options
  bag, because the renderer is ours. Cumulative snapshots (not per-node patches)
  because one update = one physics push; it diffs by node id, so re-sending what
  is already there is a no-op, and an append never re-frames the camera. The
  final frame is a `focus` directive — the equivalent of "a search landed".

Prerequisites: examples/renderer/assets/3dbrowser/ (with MaterialsVariantsShoe.glb),
examples/renderer/assets/splatviewer/ (with truck.ksplat), examples/renderer/assets/g2chart/
(the chart module + its g2.min.js), and examples/renderer/assets/editor/ +
examples/renderer/assets/monaco/ (the editor wrapper + Monaco's AMD subset). Build recipes
for all of them are in the Dockerfile. The graph needs no assets: its renderer is compiled
into the wasm bundle.
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
REVIEW = "review_form"         # data slot: the tiles the review form is appended to
REVIEW_ROWS = "review_rows"    # data slot: the review table's rows (entries)
REVIEW_CHANNEL = "review"      # uplink target the bundle submits on (ADR 0023)
ANALYTICS = "analytics_intro"  # data slot: the opening paragraph
SAY_CAR = "say_car"            # data slot: the splat paragraph
SAY_SHOE = "say_shoe"          # data slot: the shoe paragraph
SAY_PATCH = "say_patch"        # data slot: the code-review paragraph
CAR_SLOT = "intro_car"         # data slot: the splat scene's payload
SHOE_SLOT = "intro_shoe"       # data slot: the mesh model's payload
CODE_SLOT = "patch_code"       # data slot: the editor's payload {code, lang}
SAY_GRAPH = "say_graph"        # data slot: the relationship-graph paragraph
GRAPH_SLOT = "graph"           # data slot: the force graph's {nodes, links, focus?}

TREND_DATA = "/source/default/data"     # pointer into the trend slot node
TEXT_VALUE = "/source/default"          # pointer into a text slot node
RADAR_FILE = "examples/yaml/00.radar.yaml"  # the single source of the chart data

THREED_URL = "/assets/3dbrowser/index.js"
SPLAT_URL = "/assets/splatviewer/index.js"
G2CHART_URL = "/assets/g2chart/index.js"
EDITOR_URL = "/assets/editor/index.js"
SHOE_URL = "/assets/3dbrowser/MaterialsVariantsShoe.glb"
TRUCK_URL = "/assets/splatviewer/truck.ksplat"

# The two model areas inside the box. The node carries the size, and an empty
# data area takes no space (ADR 0012), so the area appears with its payload.
SPLAT_SIZE = ("720px", "460px")
SHOE_SIZE = ("640px", "420px")


# The editor area (ADR 0016 + 0025). It is a canvas — the same host as the model
# areas — pointed at the editor module, with a definite size; its payload is
# {code, lang} — "the code as a parameter".
CODE_SIZE = ("100%", "280px")
PATCH_CODE = """pub fn render(nodes: &[Node]) -> Frame {
    // 改两笔再按 Ctrl/Cmd+S：整段代码连同语言 id 会从同一条上行通道回上去
    nodes.iter().map(Node::draw).collect()
}
"""

# The closing graph: the renderer's own pieces, i.e. what the reader has been
# watching work. 33 nodes / 42 links in five clusters; the standalone carrier is
# examples/yaml/20.forcegraph.yaml (smaller, same vocabulary) — this table keeps
# the demo free of a YAML parser. `weight` drives node size, `strength` drives
# edge width — both are typed `style` fields.
# The graph is the project's own structure, not decoration: crates, the nodes
# they render, the modules that ride inside them, and the libraries underneath.
# Small graphs all look like a blob, and a blob is exactly what this view must
# not look like.
#
# These are node IDS (payload data: `{id, name, kind, weight}`), not wire
# vocabulary: `stage` here is the crate, not the retired tile count.
# component names still say `rack`/`pages`/`form`/`layer` because the diagram is
# the diagram the narrative describes (33 points, 42 lines) — the demo's own
# frames below use the current types (`tiles`, `bundle`, `panel`).
_CRATES = [
    ("accrete", 9), ("ui_leptos", 8), ("stage", 6), ("content", 7),
    ("transport", 5), ("transport_ws", 4), ("forcegraph", 7), ("ui_assets", 4),
    ("accrete_macro", 3), ("ui_leptos_macro", 3), ("fluxen", 6),
]
_NODES = [
    ("frame", "component", 7), ("canvas", "component", 7), ("rack", "component", 6),
    ("pages", "component", 5), ("form", "component", 6),
    ("popup", "component", 4), ("layer", "component", 5), ("fold", "component", 4),
    ("svg", "component", 4), ("portal", "component", 3),
    ("3dbrowser", "module", 5), ("splatviewer", "module", 5), ("g2chart", "module", 4),
    ("monaco", "module", 6), ("mermaid", "module", 3),
    ("leptos", "lib", 7), ("wasm_bindgen", "lib", 6), ("web_sys", "lib", 6),
    ("serde", "lib", 5), ("tokio", "lib", 4), ("binaryen", "tool", 3), ("trunk", "tool", 3),
]
GRAPH_NODES = (
    [{"id": name, "name": name, "kind": "crate", "weight": w} for name, w in _CRATES]
    + [{"id": name, "name": name, "kind": kind, "weight": w} for name, kind, w in _NODES]
)

GRAPH_LINKS = [
    # the crate layer: who compiles into whom, who serves whom
    {"source": "ui_leptos", "target": "accrete", "kind": "renders", "strength": 5},
    {"source": "ui_leptos", "target": "ui_leptos_macro", "kind": "generates", "strength": 3},
    {"source": "accrete", "target": "accrete_macro", "kind": "generates", "strength": 3},
    {"source": "ui_leptos", "target": "ui_assets", "kind": "loads", "strength": 3},
    {"source": "ui_leptos", "target": "forcegraph", "kind": "mounts", "strength": 4},
    {"source": "forcegraph", "target": "accrete", "kind": "node", "strength": 4},
    {"source": "stage", "target": "content", "kind": "encodes", "strength": 5},
    {"source": "stage", "target": "ui_leptos", "kind": "serves", "strength": 4},
    {"source": "stage", "target": "transport", "kind": "bridges", "strength": 3},
    {"source": "transport", "target": "transport_ws", "kind": "extends", "strength": 3},
    {"source": "transport", "target": "content", "kind": "carries", "strength": 3},
    {"source": "content", "target": "serde", "kind": "derives", "strength": 3},
    {"source": "transport", "target": "serde", "kind": "derives", "strength": 2},
    {"source": "transport_ws", "target": "tokio", "kind": "runs-on", "strength": 3},
    {"source": "fluxen", "target": "stage", "kind": "starts", "strength": 4},
    {"source": "fluxen", "target": "transport_ws", "kind": "starts", "strength": 3},
    # components the shell mounts, and the accrete nodes they are
    {"source": "frame", "target": "accrete", "kind": "node", "strength": 5},
    {"source": "frame", "target": "ui_leptos", "kind": "mounted-by", "strength": 3},
    {"source": "canvas", "target": "ui_leptos", "kind": "mounted-by", "strength": 3},
    {"source": "rack", "target": "ui_leptos", "kind": "mounted-by", "strength": 3},
    {"source": "rack", "target": "pages", "kind": "rows", "strength": 3},
    {"source": "pages", "target": "frame", "kind": "arranges", "strength": 2},
    {"source": "form", "target": "accrete", "kind": "node", "strength": 3},
    {"source": "form", "target": "ui_leptos", "kind": "mounted-by", "strength": 2},
    {"source": "popup", "target": "ui_leptos", "kind": "mounted-by", "strength": 2},
    {"source": "layer", "target": "ui_leptos", "kind": "mounted-by", "strength": 2},
    {"source": "fold", "target": "layer", "kind": "stacks-in", "strength": 2},
    {"source": "svg", "target": "ui_leptos", "kind": "mounted-by", "strength": 2},
    {"source": "portal", "target": "layer", "kind": "escapes-to", "strength": 2},
    # modules ride inside components
    {"source": "3dbrowser", "target": "canvas", "kind": "mounts", "strength": 4},
    {"source": "splatviewer", "target": "canvas", "kind": "mounts", "strength": 4},
    {"source": "g2chart", "target": "canvas", "kind": "mounts", "strength": 3},
    {"source": "monaco", "target": "canvas", "kind": "mounts", "strength": 5},
    {"source": "mermaid", "target": "canvas", "kind": "mounts", "strength": 3},
    # the platform underneath
    {"source": "leptos", "target": "wasm_bindgen", "kind": "compiles-to", "strength": 4},
    {"source": "web_sys", "target": "wasm_bindgen", "kind": "binds", "strength": 4},
    {"source": "ui_leptos", "target": "leptos", "kind": "built-with", "strength": 5},
    {"source": "forcegraph", "target": "web_sys", "kind": "draws-with", "strength": 5},
    {"source": "forcegraph", "target": "wasm_bindgen", "kind": "binds", "strength": 3},
    {"source": "trunk", "target": "ui_leptos", "kind": "builds", "strength": 3},
    {"source": "trunk", "target": "binaryen", "kind": "shrinks-with", "strength": 2},
    {"source": "binaryen", "target": "wasm_bindgen", "kind": "optimises", "strength": 2},
]

# What the first frame already shows: the crate layer and the shell that renders
# it, twenty nodes. The rest arrives with the beats that name it, so the graph
# grows while the stream talks — an edge appears only once BOTH ends are on
# screen.
# Cumulative snapshots — one write = one physics push; the component diffs by
# node id, so re-sending what is already there is a no-op and an append never
# re-frames the camera.
GRAPH_SEED = {
    "accrete", "ui_leptos", "stage", "content", "transport", "transport_ws",
    "forcegraph", "ui_assets", "accrete_macro", "ui_leptos_macro", "fluxen",
    "frame", "canvas", "rack", "pages", "form", "layer",
    "leptos", "wasm_bindgen", "web_sys", "serde",
}

# (beat label, node ids that arrive with it)
GRAPH_REVEAL = (
    ("分析引言", ("g2chart", "fold")),
    ("图表落位", ("svg", "mermaid")),
    ("复核表与表单", ("popup", "portal")),
    ("泼溅车", ("3dbrowser", "splatviewer")),
    ("鞋的网格模型", ("monaco",)),
    ("代码编辑器", ("trunk", "binaryen", "tokio")),
    ("关系图段落", ()),
)

def graph_payload(revealed):
    """The cumulative snapshot for the ids on screen; links need both ends."""
    nodes = [node for node in GRAPH_NODES if node["id"] in revealed]
    links = [
        link
        for link in GRAPH_LINKS
        if link["source"] in revealed and link["target"] in revealed
    ]
    return {"nodes": nodes, "links": links, "focus": None}

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

ANALYTICS_TEXT = """先看一组模拟数据：11 个人、3 个部门，每人每天按各自的维度打分——研发部 5 项（出勤纪律、工作质量、工作量投入、协作沟通、学习成长），市场部 6 项（另加创新性），运营部 7 项（再加客户意识），0–5 分，连续 30 天。下面第一张图是三个部门的日均分走势，第二张把每个部门的每个人叠在同一个坐标系里比形状（三张雷达分别是 5 / 6 / 7 轴，轴数跟着数据走）；再附一张复核表，可以逐人改分并提交。"""

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
        "source": {"slot": slot},
    }


def pin_tile(slot, title):
    """A draggable unit INSIDE the bubble (ADR 0034).

    A `frame` carries its own cargo: `attrs.draggable` tags this box `snippet`, and
    the cargo IS the subtree it owns — read from the node at dragstart, so there is
    no payload bag to keep in step with anything. The chip is just a `text` child
    with classes; the source adds no style and no size of its own, so what you see
    is the child. The rail's `accept: {tag: snippet}` in 00.main.yaml is what
    matches this tag — nothing else can land there, and the drop reports
    `{id, tag}` (this frame's id plus the tag).
    """
    return {
        "type": "frame",
        "id": f"pin-{slot}",
        "attrs": {"class": ["nogrow"], "draggable": {"tag": "snippet"}},
        "children": [
            {
                "type": "text",
                "attrs": {"class": ["box", "is-border", "round", "nogrow"]},
                "source": {"default": f"≡ 把「{title}」拖到右边"},
            }
        ],
    }


def chart_node(node_id, slot):
    """A canvas pointed at the g2chart module (ADR 0025). `flex` mirrors
    00.radar.yaml: it is a grid item of the band container, and the container's
    track supplies the height."""
    return {
        "type": "canvas",
        "url": G2CHART_URL,
        "id": node_id,
        "attrs": {"class": ["flex"]},
        "source": {"slot": slot},
    }


def chart_band(children, grid, css_class=None):
    """A borderless container that gives its charts a definite size."""
    attrs = {"grid": grid}
    if css_class:
        attrs["class"] = css_class
    return {"type": "frame", "attrs": attrs, "children": children}


def canvas_node(url, slot, width, height, options=None):
    node = {
        "type": "canvas",
        "url": url,
        "attrs": {"width": width, "height": height},
        "source": {"slot": slot},
    }
    # 模块私有配置走 Canvas.options（ADR 0017）：核心只搬字节，模块自己解释。
    if options is not None:
        node["options"] = options
    return node


def editor_node(node_id, slot, width, height):
    """A code editor bound to a data slot (ADR 0016). It is the same `canvas` host
    as the model areas, pointed at the editor module (ADR 0025); what the module
    renders is Monaco, and the payload is `{code, lang}`."""
    return {
        "type": "canvas",
        "url": EDITOR_URL,
        "id": node_id,
        "attrs": {"width": width, "height": height},
        "source": {"slot": slot},
    }


def frame_message():
    """The whole demo as ONE chat row: the children, in reading order."""
    return {
        "action": "append",
        "slot": "chat",
        "data": {
            "type": "frame",
            "children": [
                text_node(ANALYTICS),
                # 气泡内部的一枚可拖单元（ADR 0034）：段落是内容，这一格是动作。
                pin_tile(ANALYTICS, "分析引言"),
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
                # tiles so it can be appended when its turn comes (empty = nothing)
                {"type": "tiles", "source": {"slot": REVIEW}},
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
        "slot": slot,
        "data": {"type": "text", "source": {"default": ""}},
    }


def frame_set_payload(slot, payload):
    """Set a slot to a frame that carries the payload as its value (ADR 0027).

    A `frame` is the one node that owns a subtree and a value at once, so the old
    `group` wrapper (and the retired `bind.value.kind: default` bag) is this: a
    frame whose `source.default` IS the payload. `20.forcegraph.yaml` and
    `00.radar.yaml` write their set frames exactly this way.
    """
    return {
        "action": "set",
        "slot": slot,
        "data": {"type": "frame", "source": {"default": payload}},
    }


def frame_patch(slot, path, value):
    """Patch into a data slot (no row id: slots are addressed by name)."""
    return {"action": "patch", "slot": slot, "path": path, "op": "append", "value": value}


def frame_review_form():
    """The review table + form — 06.table.yaml's shape: a `bundle` (what `form`
    was) holding a text, a `tiles` grid and a submit button, with the rows appended
    into their own slot as `entry` nodes. There is no table/thead/tbody/tr/th/td
    family any more (ADR 0031 §4): a field is `{key, title, width, align}`, the
    header is a hook (`{selector, layers}`) whose cells are derived from the keys,
    and a row is an `entry` whose children carry `attrs.selector`.

    All three input kinds appear (number / bool / text); each writes to a local
    slot keyed by its row, and the bundle's submit carries the form up the `review`
    channel.
    """

    fields = [
        {"key": "score", "title": "复核项", "width": "8rem"},
        {"key": "name", "title": "姓名"},
        {"key": "dept", "title": "部门"},
        {"key": "avg", "title": "日均分", "width": "6rem", "align": "end"},
        {"key": "note", "title": "备注"},
    ]

    def cell(name, text):
        return {"type": "text", "attrs": {"selector": name}, "source": {"default": text}}

    def row(key, kind, default, name, dept, avg, note):
        return {
            "action": "append",
            "slot": REVIEW_ROWS,
            "data": {
                "type": "entry",
                "id": key,
                "children": [
                    {
                        "type": "input",
                        "attrs": {"selector": "score"},
                        "source": {"default": default, "type": kind},
                        "sink": {"slot": key, "scope": "local"},
                    },
                    cell("name", name),
                    cell("dept", dept),
                    cell("avg", avg),
                    cell("note", note),
                ],
            },
        }

    form = {
        "action": "append",
        "slot": REVIEW,
        "data": {
            "type": "bundle",
            "id": "review-form",
            "attrs": {"submit": ["enter", "button"]},
            # The uplink name, NOT the list slot it is appended to: a value whose
            # `sink` names its own slot would render itself, and the renderer's
            # cycle guard drops such a row at the write (slots::note in ctx.rs).
            "sink": {"slot": REVIEW_CHANNEL},
            "children": [
                {
                    "type": "text",
                    "attrs": {"format": "md"},
                    "source": {"default":
                        "复核（第一列可改，三种字段各一；点提交后整张复核表随 `review` 上行，stage 控制台可见）"},
                },
                {
                    "type": "frame",
                    "attrs": {"class": ["gap", "gap-md"]},
                    "children": [
                        {
                            "type": "tiles",
                            "attrs": {
                                "class": ["--review"],
                                "fields": fields,
                                "header": {"selector": "--review-head"},
                            },
                            "source": {"slot": REVIEW_ROWS},
                        }
                    ],
                },
                {
                    "type": "button",
                    "attrs": {"oneshot": False},
                    "source": {"default": "提交复核"},
                    "sink": {},
                },
            ],
        },
    }
    return [
        form,
        row("zw", "number", "4.13", "张伟", "研发部", "4.13", "后半月走低"),
        row("cj", "bool", False, "陈静", "市场部", "4.24", "波动最大"),
        row("sy", "text", "已复核", "孙悦", "运营部", "4.28", "收在高位"),
    ]


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

    # The graph grows with the stream: every beat that makes something appear on
    # the right also adds its node on the left, so the reader watches the two
    # halves tell the same story instead of a reveal at the end.
    revealed = set(GRAPH_SEED)

    def grow(label):
        ids = next((ids for name, ids in GRAPH_REVEAL if name == label), ())
        revealed.update(ids)
        snapshot = graph_payload(revealed)
        print(
            f"    graph ← {label}：{len(snapshot['nodes'])} 点 / {len(snapshot['links'])} 线 → "
            f"{send_json(mirror, frame_set_payload(GRAPH_SLOT, snapshot))}"
        )

    step.wait("发送 00.main.yaml")
    print(f"    send → {push_file('00.main.yaml')}")

    step.wait("整条消息入场（一个 box：引言 / 折线 / 雷达 / 摘要 / 复核表 / 车 / 鞋 / 编辑器 / 关系图，全部绑槽）")
    print(f"    append row → {send_json(mirror, frame_message())}")
    for slot in (ANALYTICS, SUMMARY, SAY_CAR, SAY_SHOE, SAY_PATCH, SAY_GRAPH):
        print(f"    seed slot  → {send_json(mirror, frame_seed_text(slot))}")

    step.wait("打字机：分析引言")
    type_out(mirror, ANALYTICS, ANALYTICS_TEXT, delay, "引言")
    grow("分析引言")

    step.wait(
        "气泡里那枚「≡ 把「分析引言」拖到右边」是一个单元，不是装饰：按住它拖到窗口右边缘"
        "那条窄轨（拖拽在飞时它自己展开），松手 → stage 控制台出现 `<- pin`，带落点 id 和 "
        "{id, tag}（货物是那个框自己的子树）；同一件货往别处拖什么也不会发生（那条轨只收 snippet）"
    )

    step.wait("两个图表入场（发 00.radar.yaml 的 set 帧：折线 spec + 三个雷达 spec）")
    print(f"    send chart data → {mirror.send(chart_data())}")
    grow("图表落位")

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

    step.wait("复核表 + 表单入场（append 进这一行里的 review 槽；行是 review_rows 里的 entry）")
    print(f"    append form + rows → {send_json(mirror, frame_review_form())}")
    grow("复核表与表单")

    step.wait("打字机：第一件商品（泼溅车）")
    type_out(mirror, SAY_CAR, CAR_TEXT, delay, "车")

    step.wait("泼溅车入场（载荷写进槽，载荷到达时把 box 撑开那一段）")
    print(
        "    set payload → "
        f"{send_json(mirror, frame_set_payload(CAR_SLOT, {'assets': {'scene': TRUCK_URL}, 'camera': TRUCK_CAMERA}))}"
    )
    grow("泼溅车")

    step.wait("打字机：第二件商品（鞋）")
    type_out(mirror, SAY_SHOE, SHOE_TEXT, delay, "鞋")

    step.wait("鞋的网格模型入场（载荷写进槽）")
    print(
        f"    set payload → {send_json(mirror, frame_set_payload(SHOE_SLOT, {'assets': {'shoe': SHOE_URL}}))}"
    )
    grow("鞋的网格模型")

    step.wait("打字机：待审的那段代码的说明")
    type_out(mirror, SAY_PATCH, PATCH_TEXT, delay, "代码说明")

    step.wait("编辑器入场（代码作为参数写进槽；Monaco 在载荷到达时才拉——ADR 0015）")
    print(
        "    set payload → "
        f"{send_json(mirror, frame_set_payload(CODE_SLOT, {'code': PATCH_CODE, 'lang': 'rust'}))}"
    )
    grow("代码编辑器")

    step.wait("在编辑器里改两笔、按 Ctrl/Cmd+S：stage 控制台出现 `<-` submit + 本节点 id + {code, lang}")

    step.wait("打字机：最后一段——关系图（这套东西自己长成一张图）")
    type_out(mirror, SAY_GRAPH, GRAPH_TEXT, delay, "关系图")
    grow("关系图段落")

    step.wait("左图距焦：把发「搜索」那一下的等价物打出来——相机飞向 accrete/content/transport 这一簇")
    focus_payload = graph_payload(revealed)
    focus_payload["focus"] = {"ids": ["accrete", "content", "transport"], "seq": 1}
    print(f"    set payload → {send_json(mirror, frame_set_payload(GRAPH_SLOT, focus_payload))}")
    time.sleep(1.2)

    print(
        "\n完成。左边是常驻的关系图（可拖动旋转、滚轮缩放、点节点回事件），右边是消息流；"
        "折线 30 天，复核表可提交，编辑器可回传代码。"
    )


if __name__ == "__main__":
    sys.exit(main())