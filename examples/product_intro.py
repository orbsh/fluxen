#!/usr/bin/env python3
"""Product intro demo: a simulated LLM presenting goods, one message at a time.

Steps (each waits for one Enter; --auto runs unattended):
  1  send 00.main.yaml      skeleton + the home/about pages
  2  send 00.chat.yaml      the chat page joins the menu
  3  inject `page`=chat     switch the visible page remotely (ADR 0011)
  4  append the message row (text / model / text / model, ONE box) and seed the
                            two text slots
  5  TYPE the first paragraph into its slot
  6  set the shoe payload   the mesh model fills its area inside that box
  7  TYPE the second paragraph
  8  set the splat payload  the 3DGS scene fills its area

Same carrier as walkthrough.py (POST frames to the mirror's /send, one Enter per
step, --auto for unattended runs) and the helpers are imported from it, so there
is one implementation of the transport and the step semantics.

Implementation choices worth naming:

- One box, not four. The message is a single `case` whose children are
  (text, canvas, text, canvas): one rack row, so one bubble, with the two
  paragraphs and the two viewers interleaved. Every child is bound to a DATA
  slot — that is what lets a streamed write re-render only that child; patching
  the row itself would re-render the whole box and remount every module host
  inside it (docs/PLAN.md). The row's children are fixed at append time, but an
  empty data area occupies NO space (ADR 0012), so each model area appears the
  moment its payload lands and the box grows by that much; the module then
  resizes into place through the host's resize leg, with no remount. A child
  appearing LATER would need a layout-plane patch, i.e. exactly the remount the
  data-slot rule avoids.
- Both paragraphs live in DATA slots (`say_intro` / `say_car`): the typewriter is
  a stream of patch-append frames, each carrying only the new fragment.
- Models land AFTER the paragraph above them: each payload `set` is sent only
  once that paragraph has finished, so the reveal order is text -> mesh model ->
  text -> splat scene rather than everything at once.
- The typewriter needs no per-character Enter: a step starts the stream and the
  loop paces it (--delay, default 0.05s unattended / 0.07s interactive).

Prerequisites: crates/ui_leptos/assets/3dbrowser/ (with MaterialsVariantsShoe.glb)
and crates/ui_leptos/assets/splatviewer/ (with truck.ksplat). Run from the repo
root (paths below are relative to it).

Usage:
  python3 examples/product_intro.py [--host 127.0.0.1] [--port 3002] [--auto]

Ctrl-C quits.
"""

import argparse
import sys
import time

from walkthrough import Mirror, Step, send_json

SAY_INTRO = "say_intro"        # data slot: first paragraph (about the shoe)
SAY_CAR = "say_car"            # data slot: second paragraph (about the splat)
SHOE_SLOT = "intro_shoe"       # data slot: the mesh model's payload
CAR_SLOT = "intro_car"         # data slot: the splat scene's payload

THREED_URL = "/assets/3dbrowser/index.js"
SPLAT_URL = "/assets/splatviewer/index.js"
SHOE_URL = "/assets/3dbrowser/MaterialsVariantsShoe.glb"
TRUCK_URL = "/assets/splatviewer/truck.ksplat"

# The two model areas inside the box: a canvas node reserves its size up front
# (it is part of the row, not appended when the payload arrives).
SHOE_SIZE = ("640px", "420px")
SPLAT_SIZE = ("720px", "460px")

# 3DGS scenes carry their own coordinate convention; the camera triple is the one
# from GaussianSplats3D's own demo page for this scene (demo/truck.html),
# otherwise the viewer's default ([0,10,15] looking at the origin, up=[0,1,0])
# frames the scene crooked.
TRUCK_CAMERA = {
    "up": [0, -1, -0.17],
    "position": [-5, -1, -1],
    "lookAt": [-1.72477, 0.05395, -0.00147],
}

INTRO_TEXT = """好，我按"先看鞋、再看车"的顺序摆给你。先说结论：这两件东西的共同点是——都不是靠参数表说服人，而是靠"看起来、转起来是什么样"。所以文字我只给必要的部分，剩下的直接给你一个能转的三维模型。

这双鞋解决的问题不是"让你跑得更快"，而是"跑完之后脚还能要"。一双鞋的价值很少体现在某一次能多跑两秒，更多体现在明天你还愿不愿意出门。中底是上下两层的密度配方：上层软，负责落地那一下的缓冲；下层偏硬，负责把力还给你。踩下去先被吃掉大半冲击，再把你推回地面，所以它不像纯软底那样"踩得深、起不来"。系带做的是快拉式，一只手一次收紧，戴手套也操作得了；鞋面是一整片针织，没有拼接缝线，长距离之后不会在脚背和脚趾外侧磨出硬边。鞋楦偏中性，前掌留了大概一个拇指的余量，宽脚也塞得进去。

下面就是这个模型：左键拖动绕它转一圈，看侧墙的支撑结构和鞋底的纹路；右键拖动是平移，滚轮缩放。"""

CAR_TEXT = """换个体量更大的。后面这台车不是网格模型——它没有面片、没有 UV、也没有贴图，而是几十万个高斯椭球按位置、尺度和朝向堆出来的（学术名字叫 3D Gaussian Splatting）。

可以把它想成一团"会发光的雾"：每个点自带颜色、透明度和方向，渲染时按视线方向排序，再把每个椭球投影到屏幕上混合成图像——不是先还原成三角形再算光照。好处很直接：它保留的是真实拍摄的观感。钣金的反射、车窗的厚度、边缘那圈高光，都会一次性带过来，不用人工建模、也不用猜材质参数。代价同样明确：文件大、要按视角排序、没法像网格那样随手改拓扑或者绑骨骼做动画。所以它现在的定位更像"把真实场景搬进浏览器"，而不是替代游戏里的网格资产。

这台车可以拖动围着转，滚轮拉近看细节。你注意到的远近层次是它自带的——这也是这类重建看起来比低模"实"的原因。"""


def text_node(slot):
    """A paragraph subscribing to a data slot. Bubble styling comes from the chat
    rack's own item template, not from this node."""
    return {
        "type": "text",
        "attrs": {"format": "md"},
        "bind": {"value": {"kind": "source", "source": slot}},
    }


def canvas_node(url, slot, width, height):
    return {
        "type": "canvas",
        "url": url,
        "attrs": {"width": width, "height": height},
        "bind": {"value": {"kind": "source", "source": slot}},
    }


def frame_message():
    """The whole demo as ONE chat row — text, model, text, model in one box."""
    return {
        "action": "append",
        "event": "chat",
        "data": {
            "type": "case",
            "children": [
                text_node(SAY_INTRO),
                canvas_node(THREED_URL, SHOE_SLOT, *SHOE_SIZE),
                text_node(SAY_CAR),
                canvas_node(SPLAT_URL, CAR_SLOT, *SPLAT_SIZE),
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


def frame_append_text(slot, fragment):
    """One typewriter frame: append a fragment into the slot's string value."""
    return {
        "action": "patch",
        "event": slot,
        "path": "/bind/value/default",
        "op": "append",
        "value": fragment,
    }


def type_out(mirror, slot, text, delay, label):
    """Stream `text` into `slot` the way an LLM streams tokens."""
    step = 3  # characters per frame — roughly one CJK token
    for i in range(0, len(text), step):
        chunk = text[i : i + step]
        r = send_json(mirror, frame_append_text(slot, chunk))
        print(f"    [{label} {i + len(chunk)}/{len(text)}] {chunk!r} → {r}")
        time.sleep(delay)


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
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
    print("fluxen 商品介绍演示 — 打开 UI（如 http://localhost:3002?codec=json）后开始。")

    def push_file(fname):
        return mirror.send(open(f"examples/yaml/{fname}", encoding="utf-8").read())

    step.wait("发送 00.main.yaml")
    print(f"    send → {push_file('00.main.yaml')}")

    step.wait("发送 00.chat.yaml")
    print(f"    send → {push_file('00.chat.yaml')}")

    # ADR 0011: remote write into the vals plane — the skeleton has no in-page
    # writer for `page`, so pages stays blank until this inject lands.
    step.wait("inject `page` = chat——远程把当前页切到 chat（vals 平面写入）")
    print(f"    send → {send_json(mirror, {'action': 'inject', 'slot': 'page', 'data': 'chat'})}")

    step.wait("整条消息入场（一个 box：文字槽 + 两个 canvas 槽，图文混排）")
    print(f"    append row → {send_json(mirror, frame_message())}")
    print(f"    seed slot  → {send_json(mirror, frame_seed_text(SAY_INTRO))}")
    print(f"    seed slot  → {send_json(mirror, frame_seed_text(SAY_CAR))}")

    step.wait("打字机：第一段（讲鞋，逐 token 流式写槽）")
    type_out(mirror, SAY_INTRO, INTRO_TEXT, delay, "文字1")

    step.wait("鞋的网格模型入场（载荷写进槽，模型填进 box 里预留的那块）")
    print(
        f"    set payload → {send_json(mirror, frame_set_payload(SHOE_SLOT, {'assets': {'shoe': SHOE_URL}}))}"
    )

    step.wait("打字机：第二段（讲泼溅车）")
    type_out(mirror, SAY_CAR, CAR_TEXT, delay, "文字2")

    step.wait("泼溅车入场（载荷写进槽）")
    print(
        "    set payload → "
        f"{send_json(mirror, frame_set_payload(CAR_SLOT, {'assets': {'scene': TRUCK_URL}, 'camera': TRUCK_CAMERA}))}"
    )

    print("\n完成。一个 box 里两段文字各自流式打完，鞋与泼溅车分别在载荷到达时把 box 撑开那一段。")


if __name__ == "__main__":
    sys.exit(main())