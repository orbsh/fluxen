# GPUI as a second host

Collected because the question will come back. Nothing here is scheduled, and nothing in it is a decision: it is what the tree already supports, what it would cost, and the judgement an actual proposal would have to pass first.

## The verdict in two sentences

The seam allows it — `accrete`, `content` and `transport` know no framework and no DOM, so a GPUI renderer can attach exactly where `ui_core` attaches. The rendering half is not a port but a rewrite, and the shape it lands in is a native window, not a browser.

## What already holds

- The vocabulary is pure data: `crates/accrete/src/lib.rs` is a closed enum plus `Bind`, `crates/content` carries the frames, and `crates/transport/src/lib.rs` is 31 lines of `emit` / `on` over raw bytes. In = `Vec<Content<Accrete>>`, out = `Outflow`. None of the three mentions leptos or `web_sys`.
- That this half builds natively is already proven, not hoped: `crates/stage` is a native binary over the same three crates (plus `tokio-tungstenite`, which is the WS client a native host needs anyway).
- ADR 0002's judgement is not "one backend only". Its closing line is the actual rule: a second backend is admissible when its render model stays inside the same zero-compile-time-structure constraint *and* it does not require the DSL to carry runtime handles. GPUI satisfies both — it builds its element tree from data on every frame, and nothing of it would enter `accrete`. So the answer under the standing ADR is "re-review and it may pass", not "forbidden".
- `crates/forcegraph`'s maths half — `camera`, `force`, `frame`, `pick` — is pure, host-testable and platform-free; only its WebGL2 render half belongs to a browser.

## The bill

- **`ui_core` does not carry over.** 3706 lines of `src/`: `components/` 17 files and `widgets/` 8 are `leptos::html::*` plus `web_sys`; `ctx.rs`'s four planes are `RwSignal` / `Owner` / `SendWrapper` by construction; `render.rs` is a `gen_dispatch!` invocation that emits leptos views. A GPUI backend is a parallel `ui_gpui` of comparable size. The one reusable piece of logic is `dispatch_msg` (`ctx.rs:236`–`382`, the reducer over layout / data / list / vals), and it is reusable as a *shape*, not as code — it writes into signals.
- **Part of the vocabulary is DOM facts, not data.** `panel`'s four cells (fixed positioning and stacking contexts, folded out of `float`/`layer`/`portal`), `tiles`'s follow-the-tail scroll, `frame`'s `attrs.draggable` / `attrs.accept` (`web_sys::DragEvent`), `svg`'s path data, `input` / `textarea`'s Enter semantics and IME. GPUI has an answer for most of these, but every semantic — when an event fires, what it carries — would have to be re-decided and re-pinned with new gates.
- **Five types are deployment artifacts, and they are the hard gap.** `canvas`, `chart` (G2 spec), `editor` (Monaco), `diagram` (`mermaid.min.js`) and `forcegraph` (three-d + WebGL2) all mount an ES module at a container (`components/canvas.rs` shares one `mount_module`). In a native host each needs either its own implementation — a chart renderer, an editor starting from `cosmic-text`, a 3D path through GPUI's canvas — or it gets dropped from the profile. This is the whole distance between "a GPUI host that runs" and "a GPUI host that is usable".
- **The styling channel disappears.** The token lane is `variant` → class → a rule in `reflux.css`'s `@layer reflux`. There is no CSS in a GPUI window, so "styling owned by the shell" becomes a theme map in Rust, and every token in `profile.json` needs a value there.
- **The reactivity semantics change.** Leptos' fine-grained signals are ADR 0002's own stated reason for choosing it: a streamed `Concat` re-runs only the one text node it touched. GPUI is entity + `notify()` with a `Render` rebuild per frame; per-node granularity means per-node entities, and that cost has to be measured rather than assumed (Zed streams text in it, so it is possible — it is not free).
- **The browser facts have nowhere to go.** `reflux:path` → `pushState`, `reflux:title` → `document.title`, `location.query` on `hello` / `reflux:nav`, and `window.fluxen.send` are all the *client runtime's*, and that runtime is browser-shaped by definition. A native window has no address bar: routing either grows a self-drawn one or the routing contract is re-decided. A native `Transport` is the easy part (gloo-net → `tokio-tungstenite`).
- **Distribution changes wholesale.** Today fluxen ships as a trunk-built wasm loom that reflux pins into a page and a wheel. A GPUI host is a native binary; the `pip install reflux` path does not apply to it at all.

## GPUI's own state

The official `gpui` crate on crates.io is 0.2.2, published 2025-10, and nothing since; it no longer builds against current `libc` / `ashpd`, so `cargo add gpui` is a broken first step today. Following upstream means a git dependency or the `gpui-unofficial` mirror (1.28.x, tracking Zed tags, Apache-2.0). It is pre-1.0 with breaking changes between releases, ~66K SLoC, and its Linux support is real (wayland and x11 backends, Zed runs on it).

## Three readings, three different answers

- **"fluxen gains a native host."** Admissible. The price is a second renderer plus a ruling on the five deployment artifacts and on the token lane. It is ADR-scale work: write the backend review under ADR 0002's judgement first, then build the smallest host that can draw the eight actions and the artifact-free subset of the vocabulary.
- **"reflux becomes a desktop application."** A different and much larger question: reflux's browser assumptions — the address bar as the routing authority, a refresh being a new device, the token riding `hello`, the wheel distributing a page — would all be re-answered. Under that premise GPUI is the expensive route; a native client that talks the wire and ignores `accrete` is cheaper.
- **A native shell around the existing loom** (wry / tauri hosting the current wasm). Not "GPUI as the frontend", but the only option that puts today's renderer in a desktop window with zero renderer work.

## What a proposal would have to settle first

The five artifact-carrying types (`canvas` / `chart` / `editor` / `diagram` / `forcegraph`): implement, defer behind the module-host seam, or remove from the native profile. Everything else on the list above is cost; that one is a product decision, and it is what decides whether a GPUI host is a smaller fluxen or a differently-shaped one.