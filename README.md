# Fluxen

[中文版](README.zh.md)

AI-native UI rendering library: Accrete DSL + Leptos rendering + operation protocol (create/set/append/patch/remove) + CBOR codec. Carved out of Fluxora (the event-bus/Gateway half became [Prism](../prism/); Fluxora itself is migrating to Leptos and keeps its name).

Design: [Fluxora 架构](../../.hermes/wiki/projects/fluxora-architecture.md) (wiki — Accrete DSL, operation protocol, codec decisions all documented there).

## What moved in

- **Accrete DSL** (`accrete` / `accrete_macro`): closed typed enum, `#[serde(tag = "type")]`, Bind system, JsonSchema validation for AI generation
- **Operation protocol** (ADR 0005, replaced the original streaming merge): create/set/append/patch/remove actions; `patch` addresses any node by JSON Pointer into its wire shape; `tmpl` registers slot-substitution templates (ADR 0006)
- **Codec** (`codec`): `ActiveCodec` enum dispatch (Json/CBOR), URL-param handshake pinning, `encode_ws()` helper

## Position

Rendering layer only — no event bus, no gateway, no transport. Upstream (Prism / Aura realm / any producer) sends Accrete operations; Fluxen renders and applies them. The thick-shell principle stays: AI generates schema-validated structured JSON (content + structure), the framework owns styling and rendering determinism.

## Dev workflow (`stage`)

`stage` is the dev gateway: a WS mirror + console REPL + UI server, so the
whole loop runs from one command — no upstream producer needed. The mirror
never inspects payloads; it routes raw frames between peers.

Endpoints once running (default port 3002):

- `GET /channel` — UI renderer connects here (Fluxen's `WsTransport` path)
- `GET /cli` — programmatic peers (the console, curl-ws, your own tools)
- `POST /send` — body is YAML (or JSON — YAML is a superset); parsed into a
  frame and broadcast to all peers. The action head is honored:
  create/set/append/patch/remove/tmpl files are all first-class
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

The console REPL streams every frame back as YAML — a bare `<-` line, then the
indented body — for both JSON and CBOR uplinks, including events the UI emits:
both sender and event monitor. Commands: `/send <file.yaml>`,
`/raw <json>` (send a bare `Message<Accrete>`), `/quit`.

Push any frame batch from a YAML file — the file is one Content item or an
array of them; the action head is preserved (create/set/append/patch/remove/
tmpl; see docs/decisions/0005-operation-layer-value-patch.md). The KDL
carrier was retired 2026-10-02 (maintained as YAML/JSON only):

```
curl -X POST --data-binary @examples/yaml/00.main.yaml http://localhost:3002/send
```

`x.nu` (repo root) wraps the carrier for nushell (`send <file> [-p <patch>]`
by extension, plus the border-flashing / message-concat / message-replace demo
loops), and `examples/push_demo.py` streams Set/Append/Patch frames over a raw
`/cli` WebSocket:

```
nu -c 'use x.nu *; send 02.concat.yaml'
python3 examples/push_demo.py
```

Frames must carry the render channel: top-level `"ev": "draw"` (see
docs/decisions/0003-ev-channel.md). Non-draw frames are ignored by the UI.

To stream updates against a running layout, use `/raw` in the console,
e.g. a chat token append onto row `m1` (JSON Pointer into the row's wire shape):

```
/raw {"ev":"draw","sender":"demo","content":[{"action":"patch","event":"chat","id":"m1","path":"/bind/value/default","op":"append","value":"hello "}]}
```

Rows streamed into a rack should carry `id` — row identity (patch addressing
and DOM keyed diff) are both keyed on it (see docs/PLAN.md conventions).

Bind routing (ADR 0008): `kind: event` uplinks the action protocol;
`kind: local { slot }` keeps the payload in-browser on the value plane
(`ctx.vals`), and any reading widget subscribes the same slot (optionally
extracting by `path` — `kind: source` gained the same `path` for wire-shape
pointers). `examples/yaml/14.local_routing.yaml` demonstrates the menu→header
channel pattern with zero transport round trip.

For hot rebuilds during UI development, start `trunk serve` (port 8281)
*before* `stage serve` — the gateway will proxy to it instead of reading dist
from disk. `trunk` is an optional dev tool, not a runtime dependency.

Offline helper (no server needed):

```
cargo run -p stage -- tojson examples/yaml/00.main.yaml   # YAML -> wire JSON (validate)
cargo run -p stage -- schema                                # Accrete wire-shape JSON Schema
```

Streaming semantics, key behavior, and codec details: see the wiki doc linked
above plus ADRs in `docs/decisions/`.

