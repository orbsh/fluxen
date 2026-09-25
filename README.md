# Fluxen

[中文版](README.zh.md)

AI-native UI rendering library: Brick DSL + Leptos rendering + streaming merge + CBOR codec. Carved out of Fluxora (the event-bus/Gateway half became [Prism](../prism/); Fluxora itself is migrating to Leptos and keeps its name).

Design: [Fluxora 架构](../../.hermes/wiki/projects/fluxora-architecture.md) (wiki — Brick DSL, merge strategies, codec decisions all documented there).

## What moved in

- **Brick DSL** (`brick` / `brick_macro`): closed typed enum, `#[serde(tag = "type")]`, Bind system, JsonSchema validation for AI generation
- **Streaming merge** (`merge`): Replace / Concat / Delete strategies, `Vec<String>` fragment buffering
- **Codec** (`codec`): `ActiveCodec` enum dispatch (Json/CBOR), URL-param handshake pinning, `encode_ws()` helper

## Position

Rendering layer only — no event bus, no gateway, no transport. Upstream (Prism / Aura realm / any producer) sends Brick operations; Fluxen renders and merges. The thick-shell principle stays: AI generates schema-validated structured JSON (content + structure), the framework owns styling and rendering determinism.

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
needed; `?codec=json` switches from CBOR to readable JSON frames in devtools.

The console REPL streams every frame back (`<- {...}`), including events the
UI emits — both sender and event monitor. Commands: `/send <file.kdl>`,
`/raw <json>` (send a bare `Message<Brick>` — needed for Set/Join frames),
`/quit`.

Push content from anywhere (KDL wraps as Create — replaces the root layout):

```
curl -X POST --data-binary @examples/kdl/chat_layout.kdl http://localhost:3002/send
```

To stream Set/Join frames against a running layout, use `/raw` in the console,
e.g. a chat token append:

```
/raw {"sender":"demo","content":[{"action":"join","event":"chat","method":"concat","data":{"type":"text","id":"m1","bind":{"value":{"kind":"default","default":"hello "}}}}]}
```

Rows streamed into a rack should carry `id` — merge and DOM identity are both
keyed on it (see docs/PLAN.md conventions).

For hot rebuilds during UI development, start `trunk serve` (port 8281)
*before* `stage serve` — the gateway will proxy to it instead of reading dist
from disk. `trunk` is an optional dev tool, not a runtime dependency.

Offline helper (no server needed):

```
cargo run -p stage -- tojson examples/kdl/chat_layout.kdl   # KDL -> Brick JSON
```

Streaming semantics, key behavior, and codec details: see the wiki doc linked
above plus ADRs in `docs/decisions/`.

