#!/usr/bin/env python3
"""Fluxen walkthrough demo: ONE chat box, every element streamed into a data slot.

Steps (each waits for one Enter; --auto runs unattended, --delay paces the text):
  1  send 00.main.yaml      the two-column shell (left graph, right stream), with the
                            page's `sketch` option set first: every border in the
                            walkthrough is hand-drawn (ADR 0038)
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
    // edit a couple of lines, press Ctrl/Cmd+S: the snippet and its language id go back up
    // the same uplink
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
    ("analysis intro", ("g2chart", "fold")),
    ("charts land", ("svg", "mermaid")),
    ("review table and form", ("popup", "portal")),
    ("splat car", ("3dbrowser", "splatviewer")),
    ("shoe mesh", ("monaco",)),
    ("code editor", ("trunk", "binaryen", "tokio")),
    ("graph paragraph", ()),
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
DEPTS = ("R&D", "Marketing", "Operations")

ANALYTICS_TEXT = """Start with a set of simulated numbers: 11 people, 3 departments, each person scoring on their own set of dimensions every day for 30 days — R&D on 5 (attendance, quality of work, effort, collaboration, learning and growth), Marketing on 6 (plus innovation), Operations on 7 (plus customer awareness), 0-5 each. The first chart below is each department's daily average; the second overlays every person in a department in one coordinate system so the shapes can be compared (the three radars have 5 / 6 / 7 axes — the axis count follows the data). Then a review table you can edit person by person and submit."""

SUMMARY_TEXT = """Summary: over the last 30 days the three departments' averages held steady or rose slightly; R&D swung widest (3.20-4.07), Marketing second, Operations the steadiest and ending high — and all three slipped together over the final two days."""

CAR_TEXT = """Start with the big one. This car is not a mesh — no polygons, no UVs, no textures. It is a few hundred thousand Gaussian ellipsoids stacked by position, scale and orientation (the academic name is 3D Gaussian Splatting).

Think of it as a cloud of glowing fog: every point carries its own colour, opacity and direction; at render time the points are sorted by view direction and each ellipsoid is projected onto the screen and blended into the image — not reconstructed into triangles and then lit. The payoff is immediate: what it keeps is how the real thing looked when captured. The reflection in the panels, the thickness of the glass, the rim of highlights — all arrive at once, with no modelling and no guessing at material parameters. The cost is just as clear: big files, per-view sorting, and no way to retopologise it or rig it for animation the way a mesh allows. So today it is "put a real scene in a browser", not a replacement for a game's mesh assets.

Drag to spin it around, scroll to move in. The depth you notice is part of the capture — which is also why this kind of reconstruction reads as more solid than a low-poly asset."""

SHOE_TEXT = """Now something small. What these two items share is that neither convinces you with a spec sheet — both are about how they look and move. So the words carry only what is necessary and the rest arrives as a model you can spin.

The problem this shoe solves is not "run faster" but "still have feet afterwards". A shoe's value rarely shows up in shaving two seconds off one run; it shows up in whether you want to go out again tomorrow. The midsole is a two-layer density recipe: the upper layer is soft and takes the landing, the lower is firmer and gives the force back. Step down and most of the impact is absorbed, then you are pushed off the ground again — which is why it is not a purely soft sole that sinks and never returns. The lacing is a quick pull: one hand, one tighten, workable in gloves. The upper is a single knit with no stitched seams, so nothing grinds at the top of the foot or outside the toes over distance. The last is neutral with about a thumb's width up front, so a wide foot fits.

Here is the model: left-drag to orbit it and look at the sidewall structure and the outsole pattern; right-drag pans, the wheel zooms."""

PATCH_TEXT = """Last step, a different direction: everything so far has been looking — this one is changing. The editor below is Monaco, the core VS Code runs on. The code is not embedded in the page: like the two products above, it travels with the message as a parameter into a data slot, and the editor subscribes to that slot.

Two consequences. First, the library download is decided by the payload arriving: this paragraph gets there first, and Monaco's four-odd-megabyte are pulled only once the code is actually sent — so "a message with an editor in it" is not "four megabytes on first paint". Second, the uplink is the same leg: edit a couple of lines, press Ctrl/Cmd+S, and the whole snippet plus its language id goes back to the producer on the channel the review table submits on, so the producer writes the file or opens a review with it.

Try a line — swap collect() for something else — then press Ctrl/Cmd+S and watch the `<-` block in the stage console."""

GRAPH_TEXT = """One last look at the graph on the left — it has been here the whole time. This page has had this shape since its first second: a permanent relationship graph on the left, a stream on the right. No menu, no tabs, no channel list. The old interface handed "where do I go" to categories, and a category is a one-time judgement written once and never revised; here "where do I go" is handed to relations, and relations are current — "who else is connected to this" is a question a tree-shaped menu cannot answer.

The graph is this thing itself: dots are crates / components / external modules, lines are "who renders whom, who mounts whom", and bigger dots and thicker lines mean more weight in the chain. Two things are worth a look. First, it is not outsourced to a JS graph library: rendering, force layout, camera and picking all live in this wasm, so the knobs are typed fields of a DSL (node size, edge width, force parameters) rather than a black-box config passed through to someone else — which is also why there is no 1.4 MB of three.js. Second, the graph grows: the whole-snapshot frames sent earlier are diffed by node id, components arrive in batches, and the points already settled do not reshuffle; the last move is a focus — the camera flies to a few nodes and the rest do not disappear, they fall back. That is how search lands.

Left-drag turns the view, the wheel zooms, and clicking a node sends an event back up."""


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
                "source": {"default": f"≡ drag \"{title}\" to the right"},
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
    # A module's private settings ride Canvas.options (ADR 0017): the core moves bytes,
    # the module interprets them.
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
                # One draggable cell inside the bubble (ADR 0034): the paragraphs are content,
                # this cell is an action.
                pin_tile(ANALYTICS, "analysis intro"),
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
        {"key": "score", "title": "item", "width": "8rem"},
        {"key": "name", "title": "name"},
        {"key": "dept", "title": "dept"},
        {"key": "avg", "title": "daily avg", "width": "6rem", "align": "end"},
        {"key": "note", "title": "note"},
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
                        "Review (the first column is editable, one field of each kind; on "
                        "submit the whole table goes up as `review` and shows in the stage console)"},
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
                    "source": {"default": "submit review"},
                    "sink": {},
                },
            ],
        },
    }
    return [
        form,
        row("zw", "number", "4.13", "Zhang Wei", "R&D", "4.13", "slow in the second half"),
        row("cj", "bool", False, "Chen Jing", "Marketing", "4.24", "widest swing"),
        row("sy", "text", "reviewed", "Sun Yue", "Operations", "4.28", "ends high"),
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
                input("    ↵ Enter to continue … ")
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
    print("fluxen walkthrough — open the UI (e.g. http://localhost:3002?codec=json) and we start.")

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
            f"    graph ← {label}: {len(snapshot['nodes'])} nodes / {len(snapshot['links'])} links → "
            f"{send_json(mirror, frame_set_payload(GRAPH_SLOT, snapshot))}"
        )

    step.wait("send 00.main.yaml")
    # Hand-drawn borders (ADR 0038 §3): this demo starts sketched. `sketch` is one member of the object
    # that lives in the page's option slot, sent with the page — a value like any other, so a page that
    # wants a switch owns the same member from its side (a bool field whose sink names the path).
    print(
        f"    sketch → {mirror.send('- action: set\n  slot: __CONFIG__\n  data:\n    type: frame\n    source:\n      default: {sketch: true}\n')}"
    )
    print(f"    send → {push_file('00.main.yaml')}")

    step.wait("the whole message arrives (one box: intro / line / radar / summary / review table / car / shoe / editor / graph, every child bound to a slot)")
    print(f"    append row → {send_json(mirror, frame_message())}")
    for slot in (ANALYTICS, SUMMARY, SAY_CAR, SAY_SHOE, SAY_PATCH, SAY_GRAPH):
        print(f"    seed slot  → {send_json(mirror, frame_seed_text(slot))}")

    step.wait("typewriter: the analytics intro")
    type_out(mirror, ANALYTICS, ANALYTICS_TEXT, delay, "intro")
    grow("analysis intro")

    step.wait(
        "That \"≡ drag analysis intro to the right\" cell is an element, not decoration: hold it "
        "and drag it to the narrow rail at the window's right edge (the rail opens while a drag "
        "is in flight); let go → the stage console shows `<- pin` with the drop id and {id, tag} "
        "(the cargo is that box's own subtree); the same cargo dragged anywhere else does "
        "nothing (that rail only takes snippets)"
    )

    step.wait("both chart bands arrive (send 00.radar.yaml's set frames: the line spec + three radar specs)")
    print(f"    send chart data → {mirror.send(chart_data())}")
    grow("charts land")

    step.wait(f"one patch per day, {len(TAIL_DAYS)} days (the line grows to 30 days)")
    for i, (day, rd, mkt, ops) in enumerate(TAIL_DAYS, start=1):
        body = [
            frame_patch(TREND, TREND_DATA, {"day": day, "avg": v, "dept": d})
            for v, d in zip((rd, mkt, ops), DEPTS)
        ]
        print(f"    [{i}/{len(TAIL_DAYS)}] {day} → {send_json(mirror, body)}")
        time.sleep(0.12)

    step.wait("typewriter: the summary")
    type_out(mirror, SUMMARY, SUMMARY_TEXT, delay, "summary")

    step.wait("the review table + form arrive (appended into this row's review slot; the row is the entry in review_rows)")
    print(f"    append form + rows → {send_json(mirror, frame_review_form())}")
    grow("review table and form")

    step.wait("typewriter: the first product (the splat car)")
    type_out(mirror, SAY_CAR, CAR_TEXT, delay, "car")

    step.wait("the splat car arrives (the payload lands in the slot and opens the box as it gets there)")
    print(
        "    set payload → "
        f"{send_json(mirror, frame_set_payload(CAR_SLOT, {'assets': {'scene': TRUCK_URL}, 'camera': TRUCK_CAMERA}))}"
    )
    grow("splat car")

    step.wait("typewriter: the second product (the shoe)")
    type_out(mirror, SAY_SHOE, SHOE_TEXT, delay, "shoe")

    step.wait("the shoe mesh arrives (the payload lands in the slot)")
    print(
        f"    set payload → {send_json(mirror, frame_set_payload(SHOE_SLOT, {'assets': {'shoe': SHOE_URL}}))}"
    )
    grow("shoe mesh")

    step.wait("typewriter: the note about the code under review")
    type_out(mirror, SAY_PATCH, PATCH_TEXT, delay, "code note")

    step.wait("the editor arrives (the code lands in the slot as a parameter; Monaco is pulled when the payload gets there — ADR 0015)")
    print(
        "    set payload → "
        f"{send_json(mirror, frame_set_payload(CODE_SLOT, {'code': PATCH_CODE, 'lang': 'rust'}))}"
    )
    grow("code editor")

    step.wait("edit a couple of lines and press Ctrl/Cmd+S: the stage console shows `<-` submit + this node's id + {code, lang}")

    step.wait("typewriter: the last paragraph — the graph (this thing growing itself into a picture)")
    type_out(mirror, SAY_GRAPH, GRAPH_TEXT, delay, "graph")
    grow("graph paragraph")

    step.wait("focus the graph: the equivalent of a search — the camera flies to the accrete/content/transport cluster")
    focus_payload = graph_payload(revealed)
    focus_payload["focus"] = {"ids": ["accrete", "content", "transport"], "seq": 1}
    print(f"    set payload → {send_json(mirror, frame_set_payload(GRAPH_SLOT, focus_payload))}")
    time.sleep(1.2)

    print(
        "\nDone. On the left is the permanent graph (drag to turn, wheel to zoom, click a node to send"
        " an event up); on the right the message stream, the line at 30 days, a review table you can"
        " submit, and an editor that hands code back."
    )


if __name__ == "__main__":
    sys.exit(main())