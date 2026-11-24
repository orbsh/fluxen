# 0038 — A page has options; a skin is how a border is painted

Status: **Accepted** (2026-11-24).

Two things a frame cannot say today: where a page-level option lives, and who decides how a border is painted. The first was answered by an accident of history — `inject` wrote page values from the far side, and ADR 0031 had already ordered it deleted. The second was never asked at all: `border: var(--border)` was the only way a border could be drawn.

---

## 1. `inject` is gone (ADR 0031 §2, finally)

ADR 0031 §2 says "`source` / `sink`; delete `report` and `inject`". `report` was executed — `both` is what it said. `inject` was not: it kept a `Content::Inject` variant, a consuming arm in `ui_core`'s dispatch, an equal-value guard with three unit tests, an example frame and three phrases in the wire documentation, and this repository has since been read twice as if its status were the other one ("nobody uses it" and "the mechanism exists").

Deleted in this change: the variant and `InjectOp`, the dispatch arm, `inject_writes` (the guard that existed only for its equal-value rule), `content/tests/inject_op.rs`, `examples/yaml/00.inject.yaml`, and the action list, the table row and the symmetry sentence in `docs/external-integration.md` and its `.zh-CN` twin. ADR 0011 is marked retired and keeps its record; the stale `inject` mentions in `docs/ideas/emergent-ui.md` now name the in-page write instead.

What is lost with it, stated now rather than discovered later: **the far side can no longer plant a page-local assignment.** ADR 0031 §1 unified values into one bind whose `scope` says who is authoritative, so the far side still has a door — `set` writes the slot's node, and a `both` read takes it as the fallback (`tiles` builds exactly that bind for its `step` slot, tiles.rs:481-489, so `set slot: page, data: {type: text, source: {default: home}}` moves a window). What it cannot do any more is *outrank* the page: `value_of` reads the local assignment first (hooks.rs:107-111), so a remote `set` decides only while the page has not chosen, and after the first in-page click that slot is the page's. `inject` was the exception that could overwrite the local half; the exception is gone, the asymmetry ADR 0031 described is now the whole rule. A `config` option (§2) is a setting, not a state: it is the one thing the far side still sets as a matter of course.

## 2. Options live in a namespace of their own: `config`

A page option is not application data. It has no row identity, no node tree and no producer that owns it, and it has to be answerable as a whole — "what is this page configured as" dumps to one object out of one namespace.

A slot *could* hold it: `ValueSlot` carries any JSON, not an `Accrete` node. So this is a naming and auditing decision, not a capability one. It does not use the slot namespace, because slots are the application's names — a producer's `skin` slot and the framework's `skin` option would be one word in one map, and "which of these names is configuration" would stop being answerable by looking.

**Decision.** `Ctx.config` — its own namespace, and one signal **per option**: `RwSignal<HashMap<String, ConfigSlot>>` with `ConfigSlot = RwSignal<ConfigValue>` and `ConfigValue { local: Option<Value>, remote: Option<Value> }`. The outer map grows only when a new option appears; a write to one option notifies that option's subscribers and nobody else, which is the per-key arrangement the data and value planes already use. An option is a member — `skin = "sketch"` — and it reads as `local` first, `remote` behind it.

`action: config` is the **primary** writer: a page's configuration normally arrives from the far side together with the frames that build the page — one `action: config` beside the `create` — so what a page looks like is a decision the producer makes at initialization, not a switch the reader flicks. There are exactly two writers, one primary and one optional:

- `action: config {data}` — the far side. **Merge, not replace**: every member in `data` is written on its own and every member left out keeps its value, so two frames touching different options cannot undo each other, and `{}` changes nothing at all. A member set to `null` clears that option's `remote` half — that is how a producer hands one option back to the page without touching the rest, and there is deliberately no clear-all. This writes the `remote` half only; a page's own choice is never overwritten by a frame. A member equal to what is already there is not written and notifies nobody (level semantics — the guard ADR 0011 §2 asked for, now per option). No authority axis: a setting has one writer at a time and the frame that just arrived is the later one.
- `Ctx::set_config(path, value)` — the page's own optional writer, with no surface behind it (§5), and it writes the `local` half. The first pointer segment names the option; anything after it writes inside that option's value, so a nested object is still one option and a write inside it re-notifies that option only. `null` at the option level clears the local half, which hands the option back to the far side's value. Containers are created as needed.

The precedence is `both`'s: the page's value wins while it is set, the far side's is the fallback (ADR 0031 §1, `hooks::value_of`). That ordering is not a concession to the page — it is what makes remote primary in practice, because most applications never expose an option to their reader at all: absent a local choice, every option is exactly what the producer sent. A page that does expose one is saying the reader may overrule this default, and a default that cannot be overruled is a lock, not a default.

A frame-declared node does **not** write the configuration. A frame asks for a border; it does not carry the page's options. That keeps `Bind` untouched: no new scope value, no reserved slot name, no second address space inside the bind vocabulary.

## 3. A skin is how a requested border is painted

A frame asks for a border with a class — `is-border` (main.css:957) — and has never asked how it is drawn. That is the seam: `config.skin` chooses the paint. Absent means `normal`, which is exactly today's rendering — no extra node, no observer, no branch taken.

Mirroring to CSS: every option becomes a `data-*` attribute on the document root, one effect per option created when it first appears — a write sets its own attribute, a removal takes it away, and no other attribute is touched. `skin = "sketch"` yields `html[data-skin="sketch"]`; absent yields no attribute at all. The boot installs the mirror through one `ui_core` helper, because the page shell is the boot's business (ui-core/src/lib.rs states that split) and a host that replaces the boot (prism, reflux) calls the same line. Nothing is per-option code, and the whole option set is visible as attributes — to devtools and to CSS alike.

What the skin changes in CSS, and only that, from `custom.css` (the page's own file, and the strongest layer):

- The border keeps its space and loses its paint — `border-color: transparent`. Nothing moves when the skin flips, which is the only reason a drawing is allowed to exist.
- `--round` and `--input-round` go to `0`. The drawing owns the shape; `.is-border.round` sets `overflow: hidden` (main.css:960-963), so a rounder CSS corner would clip content against a shape nobody drew.
- Colours are untouched. `--fg` is `light-dark()` (main.css:77), so one skin draws ink on paper and chalk on a blackboard without knowing that dark mode exists.
- The offset shadow stays: `--shadow-width` (main.css:978) is already the hand-drawn idiom.

`main.css` is not touched.

## 4. A sketched border is drawn from geometry

Attach: the widgets that honour `attrs.class` — `frame`, `panel`, `bundle`, `input`, `select` — render one extra child when the skin is sketch: an `<svg>` at `inset: 0`, `overflow: visible` (a stroke that spills a pixel past the box is the tell, and clipping it would erase the point), `pointer-events: none`, painted after content so the stroke rides the box edge. Five call sites, one helper. `button` ignores `attrs.class` (button.rs:39 hard-codes its class), so a sketchy button waits for that class list to be plumbed — a separate one-line fix, not part of the skin.

Geometry: `roughr` 0.14 draws the primitive from the host's border-box size, read by one `ResizeObserver` per sketched node (the pattern in canvas.rs:399). `Drawable` exposes `sets` and no serializer, so the op→`d` adapter (~40 lines) is ours. A rebuild happens only when the measured size moves by ≥0.5px. The seed is per node (id hash, else a mount counter) and never per size, so consecutive frames are pixel-identical and only a resize re-rolls the wobble.

Paint: every path is `fill="none" stroke="currentColor"` at `--border-width`, so the stroke inherits the colour the page already gave that element. This is the half `roughr` does not deliver: it hands back a baked colour, and a baked colour cannot follow `light-dark`. We take the geometry, we paint with `currentColor`.

Until the first size arrives the CSS border is still painted; the host carries `data-rough="ready"` once the drawing exists, and only then does the paint go transparent. One attribute, and no frame of an un-bordered box.

Scope: rectangles, and ellipses where the element is `.circle`. No hachure fill, no curves, no per-node bespoke shapes. A `tiles` container is never sketched (its children are the items it counts); its rows are CSS's, not the frame's, so v1 leaves them alone. Native popups — `<select>`'s dropdown, a date picker — belong to the browser and are never covered.

Cost: one observer and one path string per sketched node. A thousand-card wall would be a thousand observers, so the gate is a measured number (Order, step 5); if it fails, the answer is "the container is sketched, not the rows", not a second mechanism.

wasm facts, recorded so they are not rediscovered: `rand 0.8` reaches `getrandom 0.2`, which refuses `wasm32-unknown-unknown` without its `js` feature, so the crate needs a `cfg(target_arch = "wasm32")` dependency line; and `roughr` pulls `palette`/`euclid`/`svgtypes`/`svg_path_ops` into a release bundle that is 2.9 MB today (`dist-release`, uncompressed) — the delta is measured and written here before the skin is attached anywhere.

## 5. No settings surface ships

An option nobody can see is an option nobody can audit, and the answer here is not a panel. The far side sets the options when it draws the page, so the audit is the frame itself: one `action: config` in the same stream as the `create` that built the page, replayable like every other frame, greppable in the console, and carrying the whole delta it changed. A settings UI in the renderer would be a second writer with no owner, and the moment it exists "who configured this page" has two answers.

So the reference host ships no configuration surface, and `#main` grows no attribute for one. `set_config` stays as an API: a page that genuinely needs a reader-facing switch — a skin toggle drawn by the page, a preference row in a panel it already has — writes through it, reached by a handle the boot publishes beside `window.fluxen.send`. That UI and that handle are that producer's choice, not the framework's. It is also not a node tree: options are not application content, and a frame that could draw them could also forge them.

## Rejected

- **`vars`, a second page-value plane.** That is `inject` under a new name: a store whose far side could overwrite the page's own assignment — precisely the power §1 removes. ADR 0031 closed the door conceptually; this ADR closes it in code.
- **A reserved slot name** (`@config`, `$config`). Same capability as §2, while taking a name out of the producer's namespace and making the option set look like application data in every log line.
- **A `<RoughFrame>` wrapper component.** The CSS is structural: `--tile-fields` is a track list written from the field count (tiles.rs:413 → main.css:552,579), and `.tiles > *` styles the items (main.css:708). An element between a container and its items *is* a track or an item. A drawing that needs no space must not occupy one.
- **`border-image` from a generated data-URI SVG.** Cheapest by far, and two costs end it: colours are baked into the document (no `currentColor`, so a dark page gets light-mode ink), and `border-image` ignores `border-radius`, which silently voids the frame's own shape.
- **An `feTurbulence`/`feDisplacementMap` filter on a pseudo-element.** Real displacement in ten lines of CSS, and it is pixel noise, not a stroke: it blurs the edge instead of drawing a second, wavering line, and it cannot do hachure. The tell of a sketch is the stroke.
- **Shipped SVG assets per widget type.** A static asset cannot follow the box's size, and a hand-drawn *asset* is a picture of a border, not a border.
- **A handwriting font.** A CJK face is megabytes for text that is mostly CJK. Not in this skin, and not soon.

## Order

1. Delete `inject` — variant, op, arm, guard, tests, example, documentation rows, ADR 0011 status. (Done in this change.)
2. `Ctx.config` (per-option signals), `set_config`, the `config` op (merge per member, `null` removes, equal value dropped), tests beside the existing op tests.
3. The `data-*` mirror on the document root and the skin block in `custom.css`. Live check from the console: `action: config, data: {skin: sketch}`, then the same frame without it.
4. `rough_border` in ui_core, `roughr` behind a wasm target dependency, the size probe — the measured delta goes into this ADR before anything is attached.
5. Attach at the five widgets. Gates: `normal` renders no observer and no extra node; `sketch` covers `examples/yaml` (tabs, form, login, table, chart, forcegraph); frames per second with a 200-row list.