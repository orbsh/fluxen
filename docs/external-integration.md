# External integration

For anyone who serves this renderer to a browser: a Rust gateway, a Python host, an embedded
page. It states what you reuse, what you must supply, the wire, and two worked examples — the
prism venue (Rust, re-wrapped core) and reflux (Python, pinned bundle).

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

Downstream (host → page): `content::Message { ev, sender, created?, content }`.

- `ev == "draw"` renders; **any other `ev` is ignored** by the renderer (debug-logged). That is the
  operation channel (ADR 0003) and it is where a host's own verbs live — `connected`, `login.result`,
  `error`, `turn.delta`. One socket, one frame shape.
- `content` is a list of the eight actions: `create` / `set` / `append` / `remove` / `patch` /
  `inject` / `tmpl` / `empty`.

Upstream (page → host): `content::Outflow { event, id?, data }`.

Codec: CBOR by default; the reading side auto-detects the format per frame from its first byte, and
`?codec=` pins only what the client SENDS. A host may mix them freely.

Discipline: the frame's shape is checked at the producer, not hoped for at the renderer. An action
whose fields are wrong is a decode failure with a log, never a partial draw. `reflux/wire.py` in the
reflux repo is a complete worked example of that gate — closure over the vocabulary plus domain
checks, run where the frame is built.

## 3. The boot, and why it becomes its own crate

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

## 4. Worked example — prism, the venue

Prism is a WS connection plane (gateway, identity, turns). Its UI leg is `/ws`, and that leg speaks
**this** wire — not prism's own `{ev, args}` verb frames:

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
- **What prism supplies:** the transport (its `/ws` leg), the boot (its own `index.html` plus a small
  wasm entry that mounts `ui-core` with that transport), and the frames (layout + booth output).
- **What prism must NOT do:** teach `ui-core` about `{ev,args}`, or ship a second renderer.

## 5. Worked example — reflux, the Python host

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

## 6. What this document is not

- Not a second protocol. The renderer's vocabulary IS the wire's vocabulary; a host that needs
  something else sends more Accrete, not a new frame type.
- Not a prescription for the producer's language. Python, Rust, or anything that can write JSON or
  CBOR.
- Not permission to teach the core about your transport, your auth, or your database. Those arrive
  as frames, verbs, and documents.

See also: `docs/decisions/0021-external-integration.md` (the decision this document explains),
`docs/PLAN.md` (the `ui-core`/`boot` split as scheduled work).