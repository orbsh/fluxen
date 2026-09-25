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

`stage` is the dev gateway: a WS mirror + console REPL, so you can drive the
UI without any upstream producer. It never inspects payloads — it routes raw
frames between peers.

Endpoints once running (default port 3002):

- `GET /channel` — UI renderer connects here (Fluxen's `WsTransport` path)
- `GET /cli` — programmatic peers (the console, curl-ws, your own tools)
- `POST /send` — body is KDL; parsed into a `Content::Create` frame and broadcast

Terminal 1 — start the gateway (mirror + interactive console in one process):

```
cargo run -p stage -- serve            # listens on :3002
```

The console REPL streams every frame back (`<- {...}`), including events the
UI emits — use it as both sender and event monitor. Commands: `/send <file.kdl>`,
`/raw <json>` (send a bare `Message<Brick>` — needed for Set/Join frames),
`/quit`.

Terminal 2 — build and serve the UI (defaults wire up to :3002 already:
`index.html` carries `data-host="localhost:3002"`):

```
cargo install trunk       # once
cd crates/ui_leptos && trunk serve    # listens on :8281
```

Open <http://localhost:8281/> — the page connects to the mirror over
`/channel`. Query params: `?token=*** (auth passthrough), `?codec=json`
(default is CBOR; use `json` while debugging to read frames in devtools).

Terminal 3 (or the console) — push content:

```
curl -X POST --data-binary @examples/kdl/chat_layout.kdl http://localhost:3002/send
curl -X POST --data-binary @examples/kdl/login_set.kdl  http://localhost:3002/send
```

`/send` wraps everything as Create (replaces the root layout). To stream
Set/Join frames against a running layout, send a bare message via `/raw` in
the console, e.g. a chat token append:

```
/raw {"sender":"demo","content":[{"action":"join","event":"chat","method":"concat","data":{"type":"text","id":"m1","bind":{"value":{"kind":"default","default":"hello "}}}}]}
```

Rows streamed into a rack should carry `id` — merge and DOM identity are both
keyed on it (see docs/PLAN.md conventions).

Offline helper (no server needed):

```
cargo run -p stage -- tojson examples/kdl/chat_layout.kdl   # KDL -> Brick JSON
```

Streaming semantics, key behavior, and codec details: see the wiki doc linked
above plus ADRs in `docs/decisions/`.

