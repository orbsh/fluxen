#!/usr/bin/env python3
"""Interactive walkthrough: press Enter to advance one step.

Steps (each waits for one Enter):
  1  send 00.main.yaml     skeleton + the home/about pages
  2  send 00.chat.yaml     the chat page joins the menu
  3  inject `page`=chat    switch the visible page remotely (ADR 0011)
  4  send 00.radar.yaml    line chart (15 days) + summary placeholder + radars
  5  x15  patch-append one day per Enter (one frame = three appends, one per dept)
  6  xN   patch-append one summary token per Enter, below the chart

Frames go to the mirror's POST /send, so no WS handshake is needed and the
stage console echoes each one. The running row is `#scoreboard` in the `chat`
list — patches are only addressable by row id (ADR 0005): the trend data sits at
<row>/children/0/bind/value/default/data, the radar block at <row>/children/1
and the summary text at <row>/children/2/bind/value/default.

Usage:
  python3 examples/walkthrough.py [--host 127.0.0.1] [--port 3002] [--auto]

--auto skips the Enter waits (fixed delay per step), for unattended checks.
Ctrl-C quits.
"""

import argparse
import json
import sys
import time
import urllib.error
import urllib.request

ROW = "scoreboard"          # row id in the chat list (00.radar.yaml)
LIST = "chat"               # list slot carrying the row
TREND = "/children/0/bind/value/default/data"
SUMMARY = "/children/2/bind/value/default"

# Days 16..30 of the simulated series — the file itself ships days 1..15
# (09-03..09-17); these are the values the chart grows with, one day per Enter.
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

# The summary is streamed the same way the chat tokens are: one patch per
# Enter, each frame carrying only the new fragment.
SUMMARY_TOKENS = [
    "总结：",
    "近 30 天三个部门的平均分",
    "整体稳中有升；",
    "研发部波动最大（3.20–4.07），",
    "市场部次之，",
    "运营部最平稳且收在高位，",
    "末尾两天三个部门同步回落。",
]


def frame_patch(path, value):
    return {"action": "patch", "event": LIST, "id": ROW, "path": path, "op": "append", "value": value}


class Mirror:
    def __init__(self, host, port):
        self.url = f"http://{host}:{port}/send"

    def send(self, body):
        req = urllib.request.Request(self.url, data=body.encode("utf-8"), method="POST")
        try:
            with urllib.request.urlopen(req, timeout=5) as r:
                return r.read().decode().strip()
        except urllib.error.HTTPError as e:
            return f"HTTP {e.code}: {e.read().decode().strip()}"
        except urllib.error.URLError as e:
            return f"unreachable ({e.reason}) — is `stage serve` running?"


# 00.radar.yaml carries no row id: a manual /send of it can be repeated (no id =
# positional row, no duplicate-id rejection). Patches are addressable only by row
# id, so the demo's own push injects one — the file stays reusable.
ROW_ANCHOR = "    type: case\n    attrs:\n"


def inject_row_id(body):
    if body.count(ROW_ANCHOR) != 1:
        sys.exit(f"00.radar.yaml: expected exactly 1 {ROW_ANCHOR!r} anchor, found {body.count(ROW_ANCHOR)}")
    return body.replace(ROW_ANCHOR, f"    type: case\n    id: {ROW}\n    attrs:\n", 1)


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
    args = ap.parse_args()

    mirror = Mirror(args.host, args.port)
    step = Step(args.auto)
    print("fluxen walkthrough — 打开 UI（如 http://localhost:3002?codec=json）后开始。")

    def push_file(fname):
        body = open(f"examples/yaml/{fname}", encoding="utf-8").read()
        if fname == "00.radar.yaml":
            body = inject_row_id(body)
        return mirror.send(body)

    step.wait("发送 00.main.yaml")
    print(f"    send → {push_file('00.main.yaml')}")

    step.wait("发送 00.chat.yaml")
    print(f"    send → {push_file('00.chat.yaml')}")

    # ADR 0011: remote write into the vals plane — the skeleton has no in-page
    # writer for `page`, so pages stays blank until this inject lands.
    step.wait("inject `page` = chat——远程把当前页切到 chat（vals 平面写入）")
    print(f"    send → {send_json(mirror, {'action': 'inject', 'slot': 'page', 'data': 'chat'})}")

    step.wait("发送 00.radar.yaml")
    print(f"    send → {push_file('00.radar.yaml')}")

    step.wait(f"逐日 patch 追加后 {len(TAIL_DAYS)} 天（每天一帧，三个部门各一条 append）")
    for i, (day, rd, mkt, ops) in enumerate(TAIL_DAYS, start=1):
        if i > 1:
            step.wait(f"追加 {day}")
        body = [frame_patch(TREND, {"day": day, "avg": v, "dept": d}) for v, d in zip((rd, mkt, ops), DEPTS)]
        print(f"    [{i}/{len(TAIL_DAYS)}] {day} → {send_json(mirror, body)}")

    step.wait(f"逐 token patch 追加图表下方的摘要（{len(SUMMARY_TOKENS)} 帧，每帧只带新增片段）")
    for i, tok in enumerate(SUMMARY_TOKENS, start=1):
        if i > 1:
            step.wait(f"追加 token {i}/{len(SUMMARY_TOKENS)}")
        print(f"    {tok!r} → {send_json(mirror, frame_patch(SUMMARY, tok))}")

    print("\n完成。图表应为 30 天，摘要完整。")


if __name__ == "__main__":
    sys.exit(main())