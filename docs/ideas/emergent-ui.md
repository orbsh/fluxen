# Emergent UI — structure that appears from the situation

The reference is Breath of the Wild's stamina wheel: it exists only while you are climbing, swimming or gliding, sits where attention already is, and is gone otherwise. A dialog that grows a form when the conversation needs one is the same idea. Collected here because the renderer must not close doors this needs open; nothing is scheduled.

It has a counterpart, and the counterpart is already built: navigational UI — configuration and options, entries that must stay where the user last saw them. The two are the ends of one axis, **how stable the structure is**, and fluxen already splits along it: the shell is the stable frame (navigation, fixed entries), the `routes` slot is the part that moves. A fixed entry earns its fixedness because its position is a memory anchor. Emergent UI earns its instability only where the situation has already located attention — the ring is around the character, the drop target is under the pointer. Where attention is not already there, an appearing control is just a control that moved.

## What the tree already gives

- Any block at any moment: `set` / `patch` on a named slot, `append` / `remove` by row identity, `create` for the root. Slot granularity is exactly what "one block appears" needs.
- **`pages` (ADR 0010) is the keep-alive switcher, and both semantics are per container** — `display="render"` mounts only the selected row and its state goes with the unmount; `display="dom"` keeps every row in the DOM and flips a `hide` class on the others. The producer picks which, per container, which is the "optional" a per-component solution cannot give.
- The server-driven flip is wired end to end: `inject` writes the vals plane (ADR 0011) → a `Local` bind reads that slot (`hooks.rs::resolve_bind`) → `pages`' select effect sets the key → only the class closure re-runs, so no row is rebuilt. One `inject` buys a tab switch with zero state loss and no client code.
- `rack` rows are keyed (`tachys::view::keyed::keyed`, key = row id), which is the renderer's own proof that DOM identity can survive a data write.
- `fold` is the opposite by design: closed renders no children at all (`fold.rs`, `if s { render children } else { div() }`), so a fold body losing state is the contract. Memory-cheap is why it exists; making it keep-alive would be a different container or an opt-in, not a change to the default.
- Overlays for the non-blocking case: `float` / `layer` / `portal`; drag: `grab` / `dock`; identity: the auth gate draws the login surface in place of the page, which is emergent UI the framework already owns.
- A route turn is a **replace**: the `routes` slot's consumer is `placeholder_` (`case.rs`), which calls `render_accrete` on whatever node the slot holds, so each page turn rebuilds the page subtree and takes focus, scroll and input buffers with it. "Only the slot is written" saves the rest of the page, not the page's own state.

## The three limits

1. **Continuity is not on the wire.** A decaying quantity (stamina, cooldown, remaining context) is a 60 fps value; the wire carries discrete facts and the client runtime is a read-only mirror by design. Continuity lives in CSS transitions in the styling layer, or in a module (canvas) that draws itself.
2. **Replacement takes local state with it, and the fix is the container, not per-component slots.** Values can be hoisted into a slot — `pages`' select already is one — but focus, scroll position, IME composition and module state (Monaco, GL contexts) have no op and no binding, so no amount of naming preserves them. Only *not rebuilding* does, which is a property of the container (`pages`' two modes), not an annotation every component carries. Per-component hoisting is also the wrong end: it turns one structural decision into N components' worth of bookkeeping.
3. **Situation facts mostly cannot come up.** Uplink has five doors (input/textarea Enter, select option click, form submit button, module `host.send`, forcegraph node-click) plus `hello` / `nav`; `ResizeObserver` is wired only for chart/canvas/rack resize. A fact like "the viewport is narrow" never reaches the producer today.

## Viewport, in three layers

- **Presentation only** — stack vs side by side, what to deemphasise — belongs to CSS in the shell. `custom.css` already carries `@media (max-width: 1000px)` and `(max-width: 720px)`, and `.hide` sits in the custom layer so it beats `.f`. No wire involved.
- **Which block is shown** — discrete, structural — belongs to a `pages` select slot. The client may flip it in-page (local bind + an emit), or the server may flip it with one `inject`. Viewport is only an input to that decision, and the client is the one holding it.
- **The server must know in order to build a different tree** — a page that does not exist on this device, data trimmed per device — is the only case that needs an uplink. It does not exist today; it would be one host event sent on a threshold crossing, never a frame per resize.

## "The server holds the client's state, like a game"

That is already the model: the server holds the page tree and the slots, the client renders and reports events. What a game adds on top is a tick plus an input stream — continuous input up, one authoritative frame down per tick. The wire refuses that on purpose: focus, scroll and uncommitted input are 60 fps quantities, and pushing them up is the expensive half of a game's architecture. The split that holds is the same one the rest of the stack uses: continuous and derived stays on the client (CSS, local slots), discrete and structural goes up.

## Scenarios, by the fact that triggers them

- Continuous threshold: stamina, cooldown, ammo, context budget, task progress — visible only in the band where the fact means something.
- An action in progress: dragging, long press, marquee selection, a keymap prefix (vim's operator-pending), a command palette that exists only while its chord is held.
- Selection: a bulk-action bar that appears when count > 0 and goes when it hits 0.
- The other three states: empty / loading / error / refused — the lowest-cost emergent UI, already an application obligation.
- Identity: logged out shows the login surface instead of the page (framework-owned).
- Staged workflow: the next step appears when the current one is satisfied; a form grows the field the previous answer implies.
- Relations: unread badge, presence, a retry that exists only after a failure.
- Device: wide shows detail beside the list, narrow replaces it.
- Environment and time: time of day, link quality (degrade the 3D module), battery.
- Attention budget: hints for a novice that retire with proficiency.

## A first experiment, with no new types

One page drawing `pages(display="dom")` with two panes — one holding an `input`, one holding a `chart` — its `select` bound to a local slot, flipped by an `inject` from the server and by an in-page emit from the client. What to measure: does the hidden input keep its text after a round trip; does the chart keep its GL context and its animation phase; what does keeping both panes mounted cost in memory and in frame size. That single page decides whether the keep-alive path is usable or merely present.

## Open

- Does the general case need its own name, or is `pages` already the shape? Its rows are any Accrete node and its key is any string, so the type may be sufficient and only the word reads wrong (a deck is not a page list). The name is the user's to pick.
- `fold`: stay memory-cheap by design, or gain a keep option? Two containers with a keep flag would duplicate one switch.
- Should a route turn be allowed to stop writing the `routes` slot and flip a key instead? That is where kept routes land, and it is a reflux contract change (page turn → `inject` plus a path marker), not a fluxen type.