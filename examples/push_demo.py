import socket, base64, os, json, time

key = base64.b64encode(os.urandom(16)).decode()
s = socket.create_connection(("127.0.0.1", 3002), timeout=5)
req = (
    "GET /cli HTTP/1.1\r\nHost: localhost:3002\r\nUpgrade: websocket\r\n"
    "Connection: Upgrade\r\nSec-WebSocket-Key: %s\r\nSec-WebSocket-Version: 13\r\n\r\n" % key
)
s.sendall(req.encode())
buf = b""
while b"\r\n\r\n" not in buf:
    buf += s.recv(4096)

import struct

def send_text(payload):
    data = payload.encode()
    mask = os.urandom(4)
    hdr = bytearray([0x81])
    n = len(data)
    if n < 126:
        hdr.append(0x80 | n)
    else:
        hdr.append(0x80 | 126)
        hdr += struct.pack(">H", n)
    hdr += mask
    s.sendall(bytes(hdr) + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

def set_ev(event, brick):
    return {"ev": "draw", "sender": "demo", "content": [{"action": "set", "event": event, "data": brick}]}

def join(ev, id, v, sel=None):
    data = {"type": "text", "id": id, "bind": {"value": {"kind": "default", "default": v}}}
    if sel:
        # selector lives in attrs (Text's wire shape), not at the top level
        data["attrs"] = {"selector": sel}
    return {"ev": "draw", "sender": "demo", "content": [{"action": "join", "event": ev, "method": "concat", "data": data}]}

text_brick = lambda v: {"type": "text", "bind": {"value": {"kind": "default", "default": v}}}

frames = [
    set_ev("login", text_brick("chat with **AI** (user: alice)")),
    set_ev("float", text_brick("keyed rack demo")),
    join("channel::list", "1", "general"),
    join("channel::list", "2", "rust-dev"),
    join("chat", "u1", "keyed rack 修好了？给我看看", sel="ask"),
    join("chat", "a1", "修好了——"),
]
for f in frames:
    send_text(json.dumps(f))
    time.sleep(0.12)

for tok in [" 追帧只挂新行，", "兄弟行的 DOM 节点", "身份跨 merge/append 存活；", "流式 token 进同一行时", "也只换该行内容。"]:
    send_text(json.dumps(join("chat", "a1", tok)))
    time.sleep(0.22)

print("all frames pushed")
