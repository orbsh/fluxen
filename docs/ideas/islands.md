# Islands — what the word is for

The plane-level model is already written down and this note does not restate it: `prism/docs/MODELING.md` § "Islands: one application, several booths" (Chinese twin: 岛式组装) is the source, `prism/docs/PLAN.md` records the 2026-10-30 ruling that `view:connect` is answered by whoever declared it (which is what makes several booths able to own one application), and `prism/docs/adr/0017-prism-connection-plane.md` makes the auth surface the first working island — one reserved slot name (`AUTH_REGION`, `crates/prism/src/auth.rs`) and a booth that owns the region's contents. Neither fluxen's docs nor the wiki carry the word at all. What follows is the renderer's half of the argument and the comparison the plane's note has no reason to make.

## What the word usually means, and what it costs

Astro-style islands: a page of static markup plus interactive components hydrated independently, several frameworks permitted on one page, a bundler deciding what each island loads. The judgement to record is that a meta-framework of that kind supplies a packaging mechanism and not the things a page needs in order to stay one application — no shared UI vocabulary, no shared meaning for an interaction, no shared transport. Each island keeps its own runtime, its own styling convention and its own idea of what an event is, so the page becomes a set of applications rather than one application in parts.

The target is also the wrong one. "Several stacks can live on one page" is a bundler's achievement; what island mode is actually after is that separate business modules evolve on their own schedule and still compose into one interface. Coexistence of stacks is what makes that second goal expensive, not what achieves it.

## What this stack supplies instead

- **One vocabulary, and it is data.** An island draws with the same eight operations as the shell does. There is nothing to translate, so there is no second semantics to reconcile — which is the failure mode of the polyglot page, where each framework's event means something slightly different.
- **No hydration.** The client is one renderer for every island; nothing on the client is per-island. So there is no hydration boundary to place correctly, no per-island runtime to ship, and no version arbitration among them.
- **One channel per device, addressed by slot and by dest.** Islands are namespaces on one wire — a set of slot names and a set of dest names — not bundles on one page. A new part is *declared*, not loaded: the shell does not change, the page does not reload, and nobody shares a build with anyone.
- **Cost is per operation, not per page.** A `set` and an `append` each cost one DOM mutation while a `create` costs twelve, and a frame is a few hundred bytes against a page turn's kilobytes (the measured numbers live in `docs/PERF.md`; they move with the wire vocabulary). An island appearing is a frame.
- **The backend is already uncoupled.** aura's booths are instances with their own lifecycle, mailbox and store, and a realm federates them. The booth that owns the logic is the one that emits its region's layout — prism's "one description, two sides" — so a part cannot draw differently from what it does.
- **The other half is emergent UI** (`emergent-ui.md`). Islands answer *who owns which region*; emergent UI answers *when a region or a control should exist at all*. Ownership is one axis, timing the other, and a page moves along both: a region that appears because the situation calls for it, filled by whichever booth declared the event behind it.

## Why this is not "several stacks on one page"

The defect of the polyglot page is that consistency becomes a project. One design system per framework, one event convention per framework, one CSS strategy per framework, and a negotiation each time one of them upgrades. Unify the vocabulary and styling collapses into one token lane in the shell's CSS. Unify the meaning of an interaction and "what a click does" becomes a `dest` name that every island spells the same way. Unify the channel and no island owns a transport. What stays per island is the part that *should* differ: its logic, its data, and the layout of its own region.

## What is missing today

- **No multi-booth application exists in code yet.** The one working island is the auth surface; prism's island-mode ruling is recorded but unexercised beyond it, and reflux's examples are single-application.
- **Slot-name ownership is unguarded.** A name is recorded where it is used, and nothing stops two independently written booths from writing the same slot. An islands model needs one rule — a prefix per booth, or a declared region map — and it is not decided.
- **Kept islands need the switcher.** A route turn replaces the page (see `emergent-ui.md`), so an island that must keep its state while another is shown needs the `pages(display="dom")` shape — landed, and with no call site.
- **There is no map of the application.** Which booth owns which event and which slot is answerable today only by reading the declarations (`reflux dests`, `/status`). An island-mode deployment wants that list as a surface of its own, because it is what tells an operator which parts can be replaced without touching the shell.

## The sentence to keep

An island here is not "a component written in another framework"; it is a layout that declared an event, and the isolation is what the wire gives.