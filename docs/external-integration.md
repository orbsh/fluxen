# External integration

For anyone who serves this renderer to a browser: a Rust gateway, a Python host, an embedded
page. It states what you reuse, what you must supply, the wire, how a frame becomes a screen, and
two worked examples — the prism venue (Rust, serving the built artifact) and reflux (Python, pinned
loom).

The rule the whole document is about: **the renderer never adapts to a host.** A host adapts to
the renderer: it supplies a transport and a boot, and speaks the renderer's wire.

## 1. What you reuse, what you supply

Reuse — the core, no I/O, no browser assumptions:

| crate | holds |
|:--|:--|
| `accrete` (+ `accrete_macro`) | the node vocabulary and the eight actions |
| `content` (+ `codec`) | `Outflow` / the action structs; JSON + CBOR codecs |
| `ui-core` (today `ui_leptos` minus boot) | `Ctx` (layout/data/list/vals/drag signals), `render::dispatch`, components, widgets |
| `forcegraph` | the 3-D graph core (camera, layout, picking) — no framework, no DSL, no protocol |
| `ui-assets` | the CSS registry (`manganis` requires assets to live in a crate) |

Deliberately NOT in that list: `transport-ws` (`examples/transport-ws`) and the boot
(`examples/renderer`). Both live under `examples/` — they are OUR host, not part of the surface a
consumer depends on. The criterion is one line: `crates/` holds what
an external consumer depends on by name; our own host implementations live in `examples/`; `stage`
stays a crate because it is a debugging TOOL, not an example. Core depends on neither.

Supply:

1. **A transport** — one impl of `transport::Transport`: bytes out, a stream of raw frames in. It
   encodes nothing and interprets nothing; the crate's own header says so.
2. **A boot** — where the page comes from, where the socket is, whether the page gets a JS uplink
   handle.
3. **Frames** — the application's view operations, and a layout document if the application has a
   static skeleton.

The seam is exactly one constructor: `Ctx::new(transport: Rc<dyn Transport>, codec: ActiveCodec)`.
Above it nothing knows where bytes come from; below it nothing knows Accrete.

## 2. The wire

Downstream, a frame **is** the action list — the eight actions (`create` / `set` / `append` /
`move` / `remove` / `patch` / `tmpl` / `empty`) as an ordered array. There is no envelope on this
side (ADR 0021 decision 6). `content` keeps only the actions and `Outflow`, and a host keeps an
envelope only when it has more than one kind of frame to categorise — prism does (`connected`,
`init.error`, `echo.result`); `stage` and reflux have a single verb each, so neither has one.

A frame is therefore an array, always — one action is a one-element array — and it arrives as either
a JSON array or a CBOR array. Anything else is refused where it enters, with a message that names the
reason: the old envelope shape (a JSON object or a CBOR map) decodes to `frame is an envelope (map),
not an action list`, so a host that still sends `Message` sees why instead of a blank page.

Host verbs (`connected`, `login.result`, `turn.delta`) ride the host's own envelope, on the host's
own leg. The renderer never sees them and offers no filter — ADR 0003's `ev` gate left with the
envelope.

Upstream (page → host): `content::Outflow { dest, id?, data }` — the *destination*: which handler on the
host this interaction calls. It is deliberately not `slot`; the two directions are asymmetric. `event`
never appears on this wire (ADR 0023).

Codec: CBOR by default; the reading side auto-detects the format per frame from its first byte, and
`?codec=` pins only what the client SENDS. A host may mix them freely.

Discipline: the frame's shape is checked at the producer, not hoped for at the renderer. An action
whose fields are wrong is a decode failure with a log, never a partial draw. `reflux/wire.py` in the
reflux repo is a complete worked example of that gate — closure over the vocabulary plus domain
checks, run where the frame is built.

## 3. The render pipeline — bytes in, pixels out

A frame is not a patch to a DOM. It is a write to a signal, and the signal is what redraws. Both
directions pass through one seam: everything below is `ui-core` plus the transport you supply.

### 3.1 Downstream: host → page → screen

1. **Bytes.** `WsTransport::new(url, codec)` opens the socket, splits it into write and read halves,
   and a read loop pushes every incoming message into an unbounded channel — `Text` as its UTF-8
   bytes, `Bytes` as-is. `Transport::on()` exposes that channel as `Stream<Vec<u8>>`. Your
   transport's job ends here: it encodes nothing and interprets nothing.
2. **Bytes into a queue.** `Ctx::new` spawns a loop that pushes each frame's bytes onto
   `ctx.transport.frame` — a pending queue (`RwSignal<Vec<Vec<u8>>>`), not "the latest frame". The
   read loop can take several frames in one poll while the effect only runs at a microtask boundary,
   so a single-value signal would let a later frame overwrite an earlier one: the layout pushed on
   connect disappears and the page stays blank. Order is semantics here, so the queue holds it. The
   socket is therefore never on the render path — Leptos does the scheduling.
3. **Decode.** An `Effect` on that queue drains the batch and decodes each frame with
   `ActiveCodec::decode_auto::<Vec<Content<Accrete>>>`. The first byte picks the format — `[` = JSON
   array, `0x80..=0x9F` = CBOR array — regardless of the codec the page pinned for its uplink, so a
   host may switch formats mid-stream. A frame that does not decode is logged and dropped; it is
   never half-applied.
4. **Apply.** The action list applies in order, and each action writes exactly one signal:

   | action | writes | detail |
   |:--|:--|:--|
   | `tmpl` | the stamp table | `{name, params, data}`: `data` is the node tree itself, `"$name"` values are filled in whole at `create`/`set`/`append` (ADR 0037) |
   | `create` | `ctx.layout` | the root document; the page's reactive root re-renders |
   | `set` | data slot `slot` | one named node |
   | `append` | list slot `slot` | row id read from `data.id`; duplicate ids rejected; `position` counts from the tail (0 = end) |
   | `move` | list slot `slot` | the row `id` must already be in the list; `position` counts like `append`'s but on the list with the row lifted out; out of range rejected |
   | `remove` | list slot `slot` | by row id |
   | `patch` | layout root, one list row, or a data slot | JSON Pointer (RFC 6901) into the node's WIRE shape; `slot: ""` = root, `id` set = that row, else the named slot |

   These writes read untracked and then `set`: dispatch runs inside the effect that consumes the
   frame, so a tracked read would subscribe that effect to the slots it is about to write.

   **One word, one meaning.** An address inside the tree is a `slot` — `create` targets the root with
   `slot: ""`, everything else names a slot (or a row of one, via `id`). The uplink is **not** a slot
   (`Outflow.dest`, and the `dest` of a node's `kind: dest`) — the two directions are not symmetric,
   so they do not share a word: downlink writes into a place in the page, uplink calls a handler on the
   host, and the shapes differ too. `event` stays with the DOM and with the host's own events
   (ADR 0022 for the slot, ADR 0023 for the dest).
5. **Render.** The tree is data. `render::dispatch` maps a node to its component (`gen_dispatch!`
   generates the match from `accrete`'s enum), and each component puts its body inside a closure over
   the slots it reads — `text_` through `use_source_value`, `tiles` over its list slot. A `set` or
   `append` therefore re-renders the nodes bound to that slot and nothing else: no diff, no virtual
   DOM — the signal graph *is* the update scope. An empty data slot renders nothing and occupies no
   space (ADR 0012), which is why a streamed paragraph can appear before the rest of its box exists.

A hole, and what fills it. The layout declares a slot by name; the producer writes that name:

```yaml
# in the layout (create)
- type: text
  source: {slot: intro}
```
```yaml
# a later frame
action: set
slot: intro
data: { type: text, source: {default: "hello"} }
```
`set` writes the slot; every node reading `intro` re-renders. To continue the same paragraph, send
`patch` with `slot: intro` and a pointer at the slot node's field — `patch` is the streaming verb;
`set` is the whole-value one. For a list, the layout supplies the shell and the row template:

```yaml
- type: tiles
  source: {slot: chat}
  children:
  - type: frame
    attrs: { class: [as-stretch, box, is-border, round] }
```
and each `append { slot: chat, data: { id: m42, ... } }` adds one row, keyed by `data.id`, so
appending does not remount the rows above it.

### 3.2 Upstream: page → host

1. **A component emits.** The node's exit is its `sink`: `Sink { dest, scope, path }` (ADR 0031 §2).
   A node that has one gets a closure from `use_target`, and that closure calls
   `ctx.emit(&sink, id, payload)`; a node without a `sink` gets none, because it only reads. `id` is
   the emitting node's own id (`None` = none); `payload` is whatever that component produced — a form
   value, a   drag cargo, a page number. A sink with no `dest` has nowhere to go: `emit` returns false
   and sends nothing. An `input`'s change reaches the loom on its own clock (`emit`: a 250 ms
   debounce by default, `{throttle: ms}` or `{debounce: 0}` otherwise — ADR 0032 §8), so a
   `submit: [change]` frame count is a rate the producer chose, not a keystroke count.
2. **`Event` goes on the wire.** `ctx.send` → `Transport::emit(transport::Event)` →
   `WsTransport::emit`, which builds `content::Outflow { dest, id?, data }`, encodes it with the
   codec the page was built with (JSON → text frame, CBOR → binary frame) and sends. `send` is a
   future: gloo's sink suspends while the socket is still connecting and wakes when it opens, so an
   emit fired before `open` is not lost.
3. **`scope: local` never leaves the page.** A local write with a `path` changes one member of the
   value in that slot and leaves the rest alone (ADR 0031 §2); without a `path` the whole `Outflow` is
   stored in the slot named by `dest`. A local emit and an uplink frame therefore carry one shape
   (ADR 0010), which is how the whole chain can be exercised with no socket at all.
4. **The page's own handle.** The boot publishes `window.fluxen.send(name, data)`; it funnels into
   the same `Ctx::send`. A page script and a component share one uplink and one frame shape.
5. **What the host receives** is `Outflow` and nothing else: `id` is the emitter, `data` is
   free-form and belongs entirely to the component that produced it. The renderer adds no envelope
   and no metadata of its own.

### 3.3 What this pipeline promises, and what it assumes

- **Order.** Actions inside one frame apply in list order; frames apply in arrival order.
- **Failure is per action and loud.** A rejected action logs and the rest of the frame still applies;
  a frame that does not decode logs and applies nothing. Shape errors are the producer's to prevent
  (§2) — a decoded-but-wrong frame can only be skipped, never repaired.
- **Addresses must resolve.** `patch` into an empty slot, `remove` of an absent row, `append` with a
  duplicate id are all "skipped, with a warning". A producer may send `create` and `patch` back to
  back; it may not assume the renderer replays or rewrites anything.
- **Nothing is stateful in the transport.** Reconnect and re-send the layout; the page is a
  projection of the frames it has received since it mounted.

### The page's own transport

`window.fluxenTransport = { emit(bytes), subscribe(push), close? }` — install it before mount and it owns the
whole link: the boot reads it once, moves only bytes (the codec stays in Rust and still comes from
`data-codec` / `?codec=`), and both directions go through it. Retry, backoff and resumption are then the
host's business (ADR 0040 §3). Without it the default `LiveWs` runs: a `__ping__` after 5 s of inbound
silence (the host answers `__pong__`), a dead link declared after 10 s of silence and closed by the page
itself, and reconnection from 500 ms doubling to a 10 s cap with jitter (ADR 0044). The policy is one
attribute on `#main` — `data-transport='{"heartbeat": 0}'` turns the probes off, `{"retry": false}` turns
reconnection off, and both numbers plus the schedule can be written out; an unknown key is a mount error
rather than a knob that does nothing. A hook that is present but missing `emit` or `subscribe` is a mount error — never a silent fallback to our own socket, which would
leave the page's JS and its wasm each holding a socket that the other cannot see.

## 4. The boot, and why it becomes its own crate

Today `ui_leptos::mount()` is the only boot: it reads `#main`'s `data-host` / `data-path` /
`data-codec` / `data-token`, opens `ws://<host><path>`, mounts the reactive root, and publishes
`window.fluxen.send` as the page-level uplink. That is a good boot for a **pinned loom** and a
bad fit for a host that re-wraps the core in its own binary.

Decision (2026-10-29): split `ui_leptos` into

- **`ui-core`** — `ctx` / `render` / `components` / `widgets`. No `#main`, no `transport-ws`.
- **`boot`** — the `#main` reader, the WS transport wiring, `mount()`, `window.fluxen.send`.

`fluxen` then has something to be a facade about: it re-exports `ui-core` always, and `boot` behind
a feature. The three consumption styles get names instead of folklore:

| style | who | reuses | supplies |
|:--|:--|:--|:--|
| core + own boot | nobody today (the road is there for a host that wraps the core in-process) | `ui-core`, `accrete`, `content`, `forcegraph`, `ui-assets` | its own transport, its own page entry |
| boot + own frames | prism (Rust, serves the artifact), reflux (Python, pinned loom) | the loom built from the example boot (`#main`, WS, `window.fluxen.send`) | the frames, over its own socket; prism also the static root and `/app.css` |
| our boot + your transport | a host that needs a JS transport (ADR 0040) | the loom built from the example boot (`#main`, `Ctx`, `mount_to`, `window.fluxen.send`) | only the transport: `window.fluxenTransport`'s `emit`/`subscribe`/`close?`, bytes in and out; the codec stays in Rust |

The test for the two rows is who builds the socket, not who writes Rust: what prism writes in Rust is
the **producer** (gateway, identity, turns). On the renderer's side it serves the artifact, exactly as
reflux does — `crates/prism`'s only fluxen dependency is `content` (the wire vocabulary), with no
`ui-core` and no boot of its own.

### A third style: keep our boot, replace only the transport

A host that needs POST + SSE, one socket shared with the rest of its page, auth headers, or its own retry and
backoff does not have to write a Rust boot — and does not have to build a second `ui-core` wasm either (what that
costs is ADR 0040 §2: it does not move the boot out of Rust, it only adds an ABI, a second codec home and a second
artifact). Instead, one global hook:

```
window.fluxenTransport = {
  emit(bytes)      { /* one uplink frame, already encoded */ },
  subscribe(push)  { /* take the sink; hand every downlink frame to push(bytes) */ },
  close?()         { /* optional */ },
}
```

Five rules: **read once, before mount** (`Ctx::new` consumes the transport, so it cannot be swapped later);
**bytes only** — the codec stays in Rust and still comes from `#main[data-codec]` / `?codec=`; **present but
malformed is a mount error**, never a silent fall back to our WebSocket (which would leave a page believing JS owns
the socket while wasm opened its own); **retry belongs to the hook** — the framework never retries; and
`window.fluxen.send` exists in both modes, so page code never learns which transport is running. The only cost is
one extra JS↔wasm crossing per frame in hook mode, and nothing at all on the default path.

## 5. Worked example — prism, the venue

Prism is a WS connection plane (gateway, identity, turns). Its renderer leg is `/channel`, and that leg
speaks **this** wire — not prism's own `{ev, args}` verb frames (those stay on `/ws`, the programmatic
leg for the CLI and peers — the stage precedent: `/channel` for the page, `/cli` for peers):

- **Layout.** The moment the socket opens prism pushes, waiting for no uplink: the credential
  fact first, then the shell (`shell.yaml`'s `create`: navigation plus one empty region per
  island), then this viewer's data frame, and finally announces `ui:connect` on the plane —
  whichever booth declared it answers with ordinary frames. There is no "ask for the layout" verb
  and no layout envelope: the layout is the connection's answer.
- **Prism's own verbs** ride its **own** leg and its own shape (`{ev, args}` on `/ws`), not the
  renderer's: the render leg carries action lists only and decodes nothing else (decision 6). Prism's
  verbs therefore stay entirely on prism's side, and a client that cares (a login form) reads them
  there.
- **Streaming.** The producer decides the format. Gravity's turn call takes a `protocol` argument:
  default `data` (raw deltas, for non-UI consumers), `fluxen` (view frames — `append`/`set` against
  the slot the layout declares). Application knowledge stays in the booth; the renderer stays
  generic; nothing about a slot name is compiled into the loom.
- **Auth.** Device identity on connect, `login` as an ordinary uplink, no wall: authentication is a
  business decision per event, and a purely reading layout declares none.
- **What prism supplies:** the transport (its `/channel` leg), the frames (layout + the application's
  view events), and — only if it wants to — its own page. Nothing is asked for on the client side:
  the layout is pushed the moment the socket opens, so the example loom works against prism
  unchanged.
- **What prism must NOT do:** teach `ui-core` about `{ev,args}`, or ship a second renderer.

## 6. Worked example — reflux, the Python host

Reflux is the reference host for the pinned-loom style: a Python producer builds SDK nodes and the
eight actions (`reflux/wire.py`), a static shell (`www/`) loads the pinned renderer loom, and the
page uplinks through `window.fluxen.send` — the handle `mount()` publishes. Its wire is the action list
itself — a bare array, field for field this document's contract.

Under the split it consumes **`boot`** (its loom is a boot build) and never the core directly. If
it ever needs in-process rendering it moves to core + own boot — a road with no consumer today, and
prism is not on it.

Two adjustments it owes to the current standard:

- **Public URL.** A release loom declares its mount point at build time. Both hosts serve the
  loom at the page root (the bundle IS the origin: `/` answers its `index.html`, and the artifact's
  own `/ui_leptos-….js` and `/assets/core.css` resolve as built), so one root-relative build serves
  both — a host never rewrites the artifact's links and never builds twice for the mount point.
- **The uplink handle belongs to `boot`.** `reflux.js` requires `window.fluxen.send`; if that
  handle's name or contract changes, it changes in `boot`, and reflux follows the crate, not the
  other way round.

## 7. What this document is not

- Not a second protocol. The renderer's vocabulary IS the wire's vocabulary; a host that needs
  something else sends more Accrete, not a new frame type.
- Not a prescription for the producer's language. Python, Rust, or anything that can write JSON or
  CBOR.
- Not permission to teach the core about your transport, your auth, or your database. Those arrive
  as frames, verbs, and documents.

See also: `docs/decisions/0021-external-integration.md` (the decision this document explains),
`docs/PLAN.md` (the `ui-core`/`boot` split as scheduled work).