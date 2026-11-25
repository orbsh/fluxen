# Fluxen

[中文版](README.zh.md)

AI-native UI rendering library: Accrete DSL + Leptos rendering + operation protocol (create/set/append/move/remove/patch) + CBOR codec. Carved out of Fluxora (the event-bus/Gateway half became [Prism](../prism/); Fluxora itself is migrating to Leptos and keeps its name).

Design: [Fluxora 架构](https://github.com/orbsh/wiki/blob/main/projects/fluxora-architecture.md) (wiki — Accrete DSL, operation protocol, codec decisions all documented there).

## What moved in

- **Accrete DSL** (`accrete` / `accrete_macro`): closed typed enum, `#[serde(tag = "type")]`, Bind system, JsonSchema validation for AI generation
- **Operation protocol** (ADR 0005, replaced the original streaming merge): create/set/append/move/remove/patch actions; `move` reorders a row already in a list, without rebuilding it; `patch` addresses any node by JSON Pointer into its wire shape; `tmpl` registers a stamp definition — a node tree whose `"$name"` values are filled in whole (ADR 0037)
- **Codec** (`codec`): `ActiveCodec` enum dispatch (Json/CBOR), URL-param handshake pinning, `encode_ws()` helper

## Position

Rendering layer only — no event bus, no gateway, no transport. Upstream (Prism / Aura realm / any producer) sends Accrete operations; Fluxen renders and applies them. The thick-shell principle stays: AI generates schema-validated structured JSON (content + structure), the framework owns styling and rendering determinism.

Integrating it into a host (a gateway that re-wraps the core, or a server that serves a pinned
bundle) is a documented contract, not folklore: [docs/external-integration.md](docs/external-integration.md)
(English) · [中文](docs/external-integration.zh-CN.md). The rule it states: the renderer never adapts
to a host — the host supplies a transport and a boot and speaks the renderer's wire.

## Data binding

Two types, one per direction: a read is a `Source` — `{slot?, scope?, path?, default?}` — and a write is a
`Sink` — `{dest?, scope?, path?}` (ADR 0031 split the channels, ADR 0039 the type). Both name one value and one
scope; what differs is what each direction has to say about it.

```yaml
type: text
source: {slot: "notice", path: "/title", default: "untitled"}
```

- `slot` — the name a value lives under. Absent = no slot: `default` alone is a constant.
- `dest` — where a value goes. It is the same word the outflow carries (`{dest, id?, data}`, ADR 0023), so a
  sink and a frame say the same thing about one address.
- `scope` — who is authoritative: `remote` (default; written by ops, replayed), `local` (this page only, dies
  with the tree), `both` (local now, mirrored up).
- `path` — a JSON Pointer into the value, so the shape belongs to the producer and a node picks one field out of
  it (ADR 0008). Read side: which member to show. Write side: which member this node owns — it writes that one
  and leaves the rest alone, which only a `scope: local` write uses. `"/"` means the root: an object written
  there merges into what is already there, member by member, which is how a control sets its own options
  without owning the rest of the slot.
- `default` — read side only: what to show when there is no value. A write has no use for it, and a sink does
  not accept it.

`source` and `sink` are two channels, not one type called twice. Which nodes may hold one is the design —
`text` and `tiles` read; **a field does not write**: an `input` keeps `source` + `kind` (what kind of field:
`number`, `bool`, `text`, `password` — element type and parse in one place) and a `select` keeps `source` +
`options`, and both are named by `tag`. What
sends is the enclosing **bundle**: it packs its tagged fields into one object and owns the exit, the timing
(`submit: [enter, button, change]`), the budget (`sends`), the aftermath (`reset`), and the `id` the outflow
carries (ADR 0032). That `id` is the request identity — a server that published the form matches the answer by
it — while `tag` is what the far side reads inside `data`:

```yaml
type: bundle
id: "login-42"                       # the request: what comes back is matched by this
sink: {dest: "session"}              # where the packed object goes
attrs: {submit: [enter, button]}     # when it goes
children:
- {type: input, tag: user, kind: text}
- {type: input, tag: pass, kind: password}
```

A bundle may sit inside another bundle: the inner one packs on its own trigger (say `change`, for a completion
box), the outer one packs everything when its own trigger fires — one field, several ways to send it
(docs/interaction-patterns.md §4).

## Dev workflow (`stage`)

`stage` is the dev gateway: a WS mirror + console REPL + UI server, so the
whole loop runs from one command — no upstream producer needed. The mirror
never inspects payloads; it routes raw frames between peers.

Endpoints once running (default port 3002):

- `GET /channel` — UI renderer connects here (Fluxen's `WsTransport` path)
- `GET /cli` — programmatic peers (the console, curl-ws, your own tools)
- `POST /send` — body is YAML (or JSON — YAML is a superset); parsed into a
  frame and broadcast to all peers. The action head is honored:
  create/set/append/move/remove/patch/tmpl files are all first-class
- everything else — the UI itself: reverse-proxyed to `trunk serve` when its
  port (default 8281) is up at startup, otherwise served statically from the
  trunk dist dir (default `examples/renderer/dist`). Probe is TCP-only and
  sticky — restart `stage serve` after starting trunk.

Start and open <http://localhost:3002/>:

```
cargo run -p stage -- serve            # mirror + UI + console, one process
cargo run -p stage -- serve --trunk 8281 --dist examples/renderer/dist   # override
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
array of them; the action head is preserved (create/set/append/move/remove/patch/
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

A frame is the action list itself — a JSON (or CBOR) array, nothing wrapped around it
(ADR 0021 decision 6). An envelope-shaped object is refused by name, not ignored.

To stream updates against a running layout, use `/raw` in the console,
e.g. a chat token append onto row `m1` (JSON Pointer into the row's wire shape):

```
/raw [{"action":"patch","slot":"chat","id":"m1","path":"/bind/value/default","op":"append","value":"hello "}]
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


## Releasing the renderer

The renderer ships as release assets, so a host needs neither this repo nor `trunk`:

```
python3 scripts/ui_dist.py            # build, then assemble into dist-release/
python3 scripts/ui_dist.py --no-build # assemble an existing examples/renderer/dist
```

A `v*` tag runs the same script in CI (`.github/workflows/ui-release.yml`) and publishes:

- `manifest.json` — `entry.js` / `entry.wasm`, plus sha256 and size for every file
- the four files it names (`ui_leptos-<hash>.js`, `…_bg.wasm`, `assets/core.css`, `assets/custom.css`)
- `SHA256SUMS`, and `fluxen-ui-<tag>.tar.gz` (the same four plus the manifest)

Fetch by tag — e.g. `https://github.com/orbsh/fluxen/releases/download/v0.1.0/manifest.json` — copy
the four files into your static root, and pass the **hashed** wasm path to `init({ module_or_path })`:
the glue's default `ui_leptos_bg.wasm` does not exist under trunk's hashed names (PLAN §24).

Not included: `assets/3dbrowser`, `assets/splatviewer`, `assets/monaco`, `assets/mermaid.min.js`,
`assets/g2chart`. Those are module media, fetched only when a frame names them and assembled per
deployment (see the Dockerfile).
