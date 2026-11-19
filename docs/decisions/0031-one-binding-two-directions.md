# 0031 — A value has a slot and a scope; `rack` and `pages` become `tiles`; `float` and `table` are not words

Status: **Proposed** (draft for review — nothing here is implemented)

Four questions, one theme: the vocabulary encodes distinctions the framework does not need, so a
frame author has to know more than the model contains.

---

## 1. A value has a SLOT, and a closed SCOPE

### The problem

Today one `bind` field carries both the address and the mode: a `kind` says whether the value comes
from a server slot, from a page-local value, or is just a constant. The difference between the first
two is not where the value lives — both live in the same reactive store in the page — it is where it
is *authoritative*:

- a slot the far side owns: written by ops, replayed, survives a reload;
- a slot this page owns: never uploaded, dies with the tree.

Expressing that as two binding shapes gives the renderer two resolution paths for one idea.

### Why not a prefix

`slot: "local:search"` cannot say "both", and the frame that needs both is the common one (a select
whose value is local while the choice is also sent up). Worse, an open prefix cannot be validated:
`loca:search` is indistinguishable from a deliberate name, so a typo becomes a silently different
value. Open-ended prefixes in an address are a parsing convention where a closed enumeration would
do; this decision keeps the frame language free of them.

### The proposal

The noun is **slot** — the thing holds a value; it is not a path (nothing walks it), and it is not an
`id` (the node already has one of those). Where it lives is a separate, closed field:

```yaml
source: { slot: "notice", scope: remote }   # default; written by ops, replayed
source: { slot: "search", scope: local }    # this page only; nothing goes over the wire
source: { slot: "draft",  scope: both }     # local now, mirrored up as it changes
```

`remote | local | both` is the whole value domain, so a mistake is an error, not a new namespace. It
also says what the old prefixes were groping for without naming a deployment: `local` is where this
rendering happens, `remote` is the side that owns the session — words that survive SSR.

### What it costs

Every frame in the repo is rewritten (mechanical). The old spellings are not worth keeping — nothing
is shipped to users yet.

---

## 2. Two channels: source and sink

### The problem

The same `bind` means different things depending on the node: on `text` a read, on `input` a read
plus a write-back, on `select` a read plus a report. A frame that writes upstream cannot be
identified by reading the frame; you must know every component's default semantics. And a read-only
target looks exactly like a writable one until something silently does nothing.

### The proposal

Two channels, one direction each, and every node declares which it accepts — `source` is where the
value comes from, `sink` is where it goes:

```yaml
source: { slot: "draft", scope: both }   # the value flows in
sink:   { slot: "draft", scope: both }   # the value flows out
```

- `text` accepts `source`; `input` accepts `source` + `sink`; `select` accepts `source` + `sink`.
- `scope: local` — an assignment, nothing leaves the page. `scope: remote` — the existing `set` op.
  `scope: both` — assign locally and mirror the change up; the far side stays authoritative on
  replay, which is what makes a "local draft that the server has seen" expressible without a
  third channel.
- A node given a channel it does not accept is a frame error at load time, not silence at runtime.
- One value per channel, no inner `value` key. Every component in the renderer reads exactly that key
  today, so the map around it is ceremony; a node that later needs a second value gets a named field
  of its own (`items:`), not a resurrected map.
- `default` keeps its one meaning — what to use when there is no value — and covers both cases:
  `source: { default: "untitled" }` is a constant (no slot at all), `source: { slot: "draft",
  default: "untitled" }` is a slot with a fallback while it is still empty. A constant is therefore
  not a third channel, and `kind: default` has nothing left to say.

### What this deletes

`report` disappears. Sending an action to the far side was never a separate mechanism — it is a value
flowing out. A select that tells the island what was chosen is
`sink: { slot: "choice", scope: remote }`: one op, one address, no third channel to keep
consistent.

`inject` goes with it. Nothing needs a private door into a component when the component can read a
slot.

---

## 3. `float` says the same thing as `panel`

`float` positions a box at the viewport edge and renders what is bound, with no trigger and no mask.
That is `panel` with `place: viewport`, `state: bound`, minus nothing. Its enter/leave behaviour is
the rack's, not the container's.

Delete `float`. A floating box that has nothing to say has nothing to show — the rule every other
empty container already follows.

---

## 4. `rack` and `pages` are one node: `tiles`

The six-node table family (`table`/`thead`/`tbody`/`tr`/`th`/`td`) mirrors an element tree, so a frame
spells the data twice: once as rows, once as the grid holding them. Same disease as
`fold`/`popup`/`layer` — classification by what the output looks like.

**`rack` and `pages` become `tiles`**: one node that cuts the area into blocks for the slot's items,
the way `frame` is the edge around one of them. `grid` was the other candidate and it drags CSS along
with it — a grid promises aligned tracks, and an item that is bigger than its neighbours has to be
explained as a span; a tile is just a tile at whatever size it is. Chat bubbles, a board of cards and
a table of equal rows are all the same node.

```yaml
type: tiles
source: { slot: "notice" }
attrs:
  columns: [{ key: "name", title: "Name", width: "12rem" }]   # optional
  stage: all | one                                            # optional; default all
  step: { timer: 4000 } | { slot: "page" }                    # optional
```

Two data shapes, one node, decided by whether `columns` is there:

- **no `columns`** — each item of the slot is a node, rendered as it is: one cell per item. What the
  blocks look like is CSS on the container; a column of them is a list, a row of them is a strip.
- **`columns`** — each item is a record, and each column projects one field into a cell.

### Every item has a key

Selection, removal and replacement all address items by key, so a key has to exist — and it is not
worth a second addressing mode to cover items that arrive without one. Rule: if an item carries a
key, that is its key; if it does not, its position in the list at the moment it enters the slot
becomes its key. The position is read **once**, at insertion, and never recomputed — otherwise
removing the first item renames every later one, and the thing on stage silently becomes a different
thing.

On an ordered slot a positional key is only as stable as the order: inserting at the head or
re-sorting gives an item a key that no longer describes its identity. Data whose identity matters
(messages, rows the far side addresses) carries its own key; the position is the fallback for
ephemeral lists.

### Stage: all of them, or one at a time

`stage: all` lays every item out; `stage: one` lets a single item occupy the whole area — the
marquee, and also the page. Nothing about the container changes between them.

### Step: a timer, or something outside

What advances the stage is a separate decision, and it has exactly two answers:

- `step: { timer: 4000 }` — the node owns a clock (the interval the rack already has: one per node,
  stopped when the node goes away). Rotation, dwell, "newest on top" are all this.
- `step: { slot: "page" }` — the value of a slot says which item is on stage. That is what `pages`
  was: a selection driven from outside (a nav click, a route, another component). No clock.

So `pages` has no reason to exist once selection is just a slot: a keyed dictionary is a slot whose
items carry keys, `stage: one` is the page, and the stepper is either a clock or the slot the
navigation writes. `rack` and `pages` were the same node with different steppers, exactly like
`fold`/`popup`/`layer` were the same container with different axes.

The play policy (newest / rotate / dwell) is not a third concept either: it is what `stage: one` or
`stage: all` does together with a timer stepper.

### A table is tiles, not a `<table>`

Emitting real `table`/`thead`/`th` was the first instinct and it is wrong here: it hands styling back
to the browser, so a frame that only wants arrangement has to override a pile of user-agent rules it
never asked for. Text extractors flatten a table to one dimension anyway, and readers lose the second
dimension in either markup — the semantics buy nothing that pays for the CSS.

So `tiles` renders blocks, and the styling that matters is addressed the way the rest of the renderer
already addresses things — by **selector**:

- `header: { selector, rows: 1 | 2 }` — when headers are enabled, the header cells carry the given
  selector; `rows: 2` gives a two-dimensional header (a row of groups over a row of leaves).
- `column.selectors` — the same mechanism per column, since a column is not an element.

### Spans are not a feature; hierarchy is

`colspan`/`rowspan` exist to fake hierarchy in a flat grid. Where the data is already hierarchical —
an item with children — the renderer draws the hierarchy: a row with children becomes a row plus its
child rows, indented or grouped, and a two-row header expresses the grouping instead of merging cells.
A frame that wants merging can style it; the model does not carry an escape hatch for a shape the data
already has.

This is the largest change here: `columns` (width, alignment, formatting) does not exist yet, and
sorting/paging follow it. Do it last.

---

## Order

1. Slot + `scope` (`remote` / `local` / `both`), one resolution path.
2. `source` / `sink`; delete `report` and `inject`.
3. Delete `float` (a frame + a component).
4. `rack` + `pages` → `tiles` (`stage` + `step`); fold the table family in with `columns`.

## Not in this document

`grab` / `dock` / `template` / `group` are named for how they look rather than what they do, but
their semantics are still moving; renaming them now is churn without a definition to land on.