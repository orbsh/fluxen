# External integration

For anyone who serves this renderer to a browser: a Rust gateway, a Python host, an embedded
page. It states what you reuse, what you must supply, the wire, how a frame becomes a screen, and
two worked examples — the prism venue (Rust, re-wrapped core) and reflux (Python, pinned bundle).

The rule the whole document is about: **the renderer never adapts to a host.** A host adapts to
the renderer: it supplies a transport and a boot, and speaks the renderer's wire.

## 1. What you reuse, what you supply

Reuse — the core, no I/O, no browser assumptions:

| crate | holds |
|:--|:--|
| `accrete` (+ `accrete_macro`) | the node vocabulary and the eight actions |
| `content` (+ `codec`) | `Message` / `Outflow` / the action structs; JSON + CBOR codecs |
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

Downstream, the renderer consumes one thing: an **action list** — the eight actions (`create` /
`set` / `append` / `remove` / `patch` / `inject` / `tmpl` / `empty`) as an ordered array.

The envelope around that list is **your packaging**. Stage, prism and reflux each build their own
`Message { ev, sender, created?, content }`; `sender` and `created` the renderer never reads — they
are the host's business. The one envelope field it looks at is `ev`, and only to decide whether the
frame is addressed to it: `ev == "draw"` renders, any other value is debug-logged and left alone —
which is where a host's own verbs ride (`connected`, `login.result`, `error`, `turn.delta`; ADR 0003,
the operation channel). One socket, one envelope, and the only part that has to be right for
rendering is the action list inside it.

Upstream (page → host): `content::Outflow { event, id?, data }`.

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
   `ActiveCodec::decode_auto::<Message<Accrete>>`. The first byte picks the format — `{` = JSON,
   `0xA0..=0xBF` = CBOR — regardless of the codec the page pinned for its uplink, so a host may switch
   formats mid-stream. A frame that does not decode is logged and dropped; it is never half-applied.
4. **The `ev` gate.** `dispatch_msg` returns immediately for any `ev` other than `draw` (ADR 0003).
   Host verbs — `connected`, `login.result`, `turn.delta` — travel on the same socket and are
   ignored here by design.
5. **Apply.** The action list applies in order, and each action writes exactly one signal:

   | action | writes | detail |
   |:--|:--|:--|
   | `tmpl` | the template table | `{{key}}` substitution, run at `create`/`set`/`append` (ADR 0006) |
   | `create` | `ctx.layout` | the root document; the page's reactive root re-renders |
   | `set` | data slot `event` | one named node |
   | `append` | list slot `event` | row id read from `data.id`; duplicate ids rejected; `position` counts from the tail (0 = end) |
   | `remove` | list slot `event` | by row id |
   | `patch` | layout root, one list row, or a data slot | JSON Pointer (RFC 6901) into the node's WIRE shape; `event: ""` = root, `id` set = that row, else the named slot |
   | `inject` | vals slot | local plane; an equal value short-circuits |

   These writes read untracked and then `set`: dispatch runs inside the effect that consumes the
   frame, so a tracked read would subscribe that effect to the slots it is about to write.
6. **Render.** The tree is data. `render::dispatch` maps a node to its component (`gen_dispatch!`
   generates the match from `accrete`'s enum), and each component puts its body inside a closure over
   the slots it reads — `text_` through `use_source_value`, `rack` over its list slot. A `set` or
   `append` therefore re-renders the nodes bound to that slot and nothing else: no diff, no virtual
   DOM — the signal graph *is* the update scope. An empty data slot renders nothing and occupies no
   space (ADR 0012), which is why a streamed paragraph can appear before the rest of its box exists.

A hole, and what fills it. The layout declares a slot by name; the producer writes that name:

```yaml
# in the layout (create)
- type: text
  bind: { value: { kind: source, source: intro, default: "" } }
```
```yaml
# a later frame
action: set
event: intro
data: { type: text, bind: { value: { kind: default, default: "hello" } } }
```
`set` writes the slot; every node reading `intro` re-renders. To continue the same paragraph, send
`patch` with `event: intro` and a pointer at the slot node's field — `patch` is the streaming verb;
`set` is the whole-value one. For a list, the layout supplies the shell and the row template:

```yaml
- type: rack
  bind: { value: { kind: source, source: chat } }
  item:
  - type: case
    attrs: { class: [as-stretch, box, border, round] }
```
and each `append { event: chat, data: { id: m42, ... } }` adds one row, keyed by `data.id`, so
appending does not remount the rows above it.

### 3.2 Upstream: page → host

1. **A component emits.** A node whose `bind["value"]` is `Event { event }` or `Local { slot }` calls
   `ctx.emit(variant, id, payload)` through `use_target`. `id` is the emitting node's own id
   (`node_id`, empty string = none); `payload` is whatever that component produced — a form value, a
   drag cargo, a page number.
2. **`Event` goes on the wire.** `ctx.send` → `Transport::emit(transport::Event)` →
   `WsTransport::emit`, which builds `content::Outflow { event, id?, data }`, encodes it with the
   codec the page was built with (JSON → text frame, CBOR → binary frame) and sends. `send` is a
   future: gloo's sink suspends while the socket is still connecting and wakes when it opens, so an
   emit fired before `open` is not lost.
3. **`Local` never leaves the page.** The same `Outflow`-shaped object is written into that `vals`
   slot, so a subscriber cannot tell a local emit from a remote `inject` (ADR 0010, 0011) — which is
   how the whole chain can be exercised with no socket at all.
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

## 4. The boot, and why it becomes its own crate

Today `ui_leptos::mount()` is the only boot: it reads `#main`'s `data-host` / `data-path` /
`data-codec` / `data-token`, opens `ws://<host><path>`, mounts the reactive root, and publishes
`window.fluxen.send` as the page-level uplink. That is a good boot for a **pinned bundle** and a
bad fit for a host that re-wraps the core in its own binary.

Decision (2026-10-29): split `ui_leptos` into

- **`ui-core`** — `ctx` / `render` / `components` / `widgets`. No `#main`, no `transport-ws`.
- **`boot`** — the `#main` reader, the WS transport wiring, `mount()`, `window.fluxen.send`.

`fluxen` then has something to be a facade about: it re-exports `ui-core` always, and `boot` behind
a feature. The two consumption styles get names instead of folklore:

| style | who | reuses | supplies |
|:--|:--|:--|:--|
| core + own boot | prism (Rust, in-process) | `ui-core`, `accrete`, `content`, `forcegraph`, `ui-assets` | its own transport, its own page entry |
| boot + own frames | reflux (Python, pinned bundle) | the bundle built from the example boot | the frames, over its own socket |

## 5. Worked example — prism, the venue

Prism is a WS connection plane (gateway, identity, turns). Its renderer leg is `/ui`, and that leg
speaks **this** wire — not prism's own `{ev, args}` verb frames (those stay on `/ws`, the programmatic
leg for the CLI and peers — the stage precedent: `/channel` for the page, `/cli` for peers):

- **Layout.** The client's first uplink is `Outflow { event: "init" }`. Prism answers with the
  layout as ordinary `create` frames: a `file:` source delivers the YAML frame list verbatim, an
  `event:` source submits to a global booth and forwards what it returns. There is no layout
  envelope and no second shape — the layout is just the first view operation.
- **Prism's own verbs** ride `Message { ev: "<verb>", content: [] }`; the renderer ignores them by
  design, and a client that cares (a login form) reads them. No second frame shape on the socket.
- **Streaming.** The producer decides the format. Gravity's turn call takes a `protocol` argument:
  default `data` (raw deltas, for non-UI consumers), `fluxen` (view frames — `append`/`set` against
  the slot the layout declares). Application knowledge stays in the booth; the renderer stays
  generic; nothing about a slot name is compiled into the bundle.
- **Auth.** Device identity on connect, `login` as an ordinary uplink, no wall: authentication is a
  business decision per event, and a purely reading layout declares none.
- **What prism supplies:** the transport (its `/channel` leg), the frames (layout + the application's
  view events), and — only if it wants to — its own page. Nothing is asked for on the client side:
  the layout is pushed the moment the socket opens, so the example bundle works against prism
  unchanged.
- **What prism must NOT do:** teach `ui-core` about `{ev,args}`, or ship a second renderer.

## 6. Worked example — reflux, the Python host

Reflux is the reference host for the pinned-bundle style: a Python producer builds SDK nodes and the
eight actions (`reflux/wire.py`), a static shell (`www/`) loads the pinned renderer bundle, and the
page uplinks through `window.fluxen.send` — the handle `mount()` publishes. Its wire is already
`Message { ev: "draw", sender, content }`, field for field this document's wire.

Under the split it consumes **`boot`** (its bundle is a boot build) and never the core directly. If
it ever needs in-process rendering it moves to core + own boot, the same way prism does.

Two adjustments it owes to the current standard:

- **Public URL.** A release bundle declares its mount point at build time. Reflux serves the bundle
  at the page root, so its pinned build keeps a root-relative public URL; prism serves at `/assets/`
  and builds accordingly. Two builds of one core — a host never rewrites the artifact's links.
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