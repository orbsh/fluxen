# Fluxen

[中文版](README.zh.md)

AI-native UI rendering library: Accrete DSL + Leptos rendering + streaming merge + CBOR codec. Carved out of Fluxora (the event-bus/Gateway half became [Prism](../prism/); Fluxora itself is migrating to Leptos and keeps its name).

Design: [Fluxora 架构](../../.hermes/wiki/projects/fluxora-architecture.md) (wiki — Accrete DSL, merge strategies, codec decisions all documented there).

## What moved in

- **Accrete DSL** (`accrete` / `accrete_macro`): closed typed enum, `#[serde(tag = "type")]`, Bind system, JsonSchema validation for AI generation
- **Streaming merge** (`merge`): Replace / Concat / Delete strategies, `Vec<String>` fragment buffering
- **Codec** (`codec`): `ActiveCodec` enum dispatch (Json/CBOR), URL-param handshake pinning, `encode_ws()` helper

## Position

Rendering layer only — no event bus, no gateway, no transport. Upstream (Prism / Aura realm / any producer) sends Accrete operations; Fluxen renders and merges. The thick-shell principle stays: AI generates schema-validated structured JSON (content + structure), the framework owns styling and rendering determinism.

## Dev workflow (`stage`)

`stage` is the dev gateway: a WS mirror + console REPL + UI server, so the
whole loop runs from one command — no upstream producer needed. The mirror
never inspects payloads; it routes raw frames between peers.

Endpoints once running (default port 3002):

- `GET /channel` — UI renderer connects here (Fluxen's `WsTransport` path)
- `GET /cli` — programmatic peers (the console, curl-ws, your own tools)
- `POST /send` — body is KDL; parsed into a `Content::Create` frame and broadcast
- everything else — the UI itself: reverse-proxyed to `trunk serve` when its
  port (default 8281) is up at startup, otherwise served statically from the
  trunk dist dir (default `crates/ui_leptos/dist`). Probe is TCP-only and
  sticky — restart `stage serve` after starting trunk.

Start and open <http://localhost:3002/>:

```
cargo run -p stage -- serve            # mirror + UI + console, one process
cargo run -p stage -- serve --trunk 8281 --dist crates/ui_leptos/dist   # override
```

The UI's WS host defaults to the page origin, so no `?token`-style config is
needed. Receivers auto-detect per frame (first byte: `{` = JSON, CBOR map
major type = CBOR), so the gateway can mix encodings freely; `?codec=json`
only pins what the UI itself *sends* (user events) — handy for reading them
in devtools. Default send format is CBOR.

The console REPL streams every frame back (`<- {...}`), including events the
UI emits — both sender and event monitor. Commands: `/send <file.kdl>`,
`/raw <json>` (send a bare `Message<Accrete>` — needed for Set/Join frames),
`/quit`.

Push layout frames from a KDL file (wraps as Create — replaces the root layout):

```
curl -X POST --data-binary @examples/kdl/chat_layout.kdl http://localhost:3002/send
```

YAML files carry full Content expressiveness (any action: create/set/join/tmpl —
the action is preserved, not forced to create). A file is one Content item or an
array of them; `?fmt=yaml` selects the parser (KDL stays the default):

```
curl -X POST --data-binary @examples/yaml/02.concat.yaml "http://localhost:3002/send?fmt=yaml"
```

`examples/fluxen.nu` wraps both carriers for nushell (`send <file> [-p <patch>]`
by extension, plus the border-flashing / message-concat / message-replace demo
loops), and `examples/push_demo.py` streams Set/Join frames over a raw `/cli`
WebSocket:

```
nu -c 'use examples/fluxen.nu *; send 02.concat.yaml'
python3 examples/push_demo.py
```

Frames must carry the render channel: top-level `"ev": "draw"` (see
docs/decisions/0003-ev-channel.md). Non-draw frames are ignored by the UI.

To stream Set/Join frames against a running layout, use `/raw` in the console,
e.g. a chat token append:

```
/raw {"ev":"draw","sender":"demo","content":[{"action":"join","event":"chat","method":"concat","data":{"type":"text","id":"m1","bind":{"value":{"kind":"default","default":"hello "}}}}]}
```

Rows streamed into a rack should carry `id` — merge and DOM identity are both
keyed on it (see docs/PLAN.md conventions).

For hot rebuilds during UI development, start `trunk serve` (port 8281)
*before* `stage serve` — the gateway will proxy to it instead of reading dist
from disk. `trunk` is an optional dev tool, not a runtime dependency.

Offline helper (no server needed):

```
cargo run -p stage -- tojson examples/kdl/chat_layout.kdl   # KDL -> Accrete JSON
```

Streaming semantics, key behavior, and codec details: see the wiki doc linked
above plus ADRs in `docs/decisions/`.

