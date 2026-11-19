# 0035 — `tiles` has a direction: two axes, four directions

Status: **Accepted** (2026-11-19).

`tiles` had no direction. `row` versus `col` was a CSS class — `.cols` means *fields present*, not
"a column" — and the only axis switch anywhere was `FrameAttr.horizontal`, which is about how a
frame's children flow. A table, a wall of cards, a reversed list and a transposed (paged) table are
the same node seen along a different axis, so the axis belongs in the model.

---

## The decision

```yaml
type: tiles
attrs:
  dir: row | row-reverse | col | col-reverse   # absent = col
  wrap: wrap | wrap-reverse | nowrap           # absent = wrap
  fields: [...]                                # optional; the grid's cells
  header: {selector, layers: 1 | 2}            # optional; needs fields
```

`dir` names the way the **items** flow, exactly as flex means it: `row` lays the items out left to
right, `row-reverse` right to left, `col` top to bottom, `col-reverse` bottom to top. The words are
flex's words down to the `-reverse` spellings, which reverse the *order* the items arrive in without
changing the axis — a chat list newest-first is `col-reverse`, not another node, and a strip read
backwards is `row-reverse`.

## The default is `col`, and that is deliberate

Absent is `col`, not flex's `row`. `tiles` is a list of items, and an unlabelled list is vertical:
the shape a frame means when it says nothing is the one a chat log, an inbox and a set of messages
already have. It is also the reason the change costs nothing — every frame in the repo that never
named a direction is a vertical list or a table, and both keep the shape they had.

This is the single place the vocabulary deliberately differs from flex, which lays out inline
content and therefore starts on a line. Flex's default describes a sentence; `tiles`' default
describes a list. Everything else — the four words, the order they reverse, `nowrap` — is flex's.

## Why the fields axis is the cross axis

An entry's cells are what it is made of, so the fields run **across** the item flow and the items
flow along `dir`. That fixes which axis the one track list is read on:

- `dir: col*` (the default): the items flow down, so the field tracks are each entry's **column**
  axis — an entry is a row of cells, the header is a line of cells above it (`below` with
  `wrap-reverse`). That is today's table, unchanged.
- `dir: row*`: the items flow across, so the field tracks are each entry's **row** axis — an entry
  is one column of cells, and the header becomes the first column (the last with `wrap-reverse`).

Nothing is duplicated: one track list, read on whichever axis `dir` leaves free. The renderer's
custom properties say the same thing in one place — `--tile-fields` is the list the frame wrote,
`--tile-columns` / `--tile-rows` are the grid's track lists, and `--tile-flow` is the axis the cells
flow on inside an entry, which is the cross axis of `dir` and therefore always the opposite word.

`wrap` is the line axis, which is why it can move the header: lines are laid out along the field
axis, so reversing the lines puts the fields on the other side — the header row below the entries,
or the header column on their right.

## The class carries the direction

The renderer emits `dir-row`, `dir-row-reverse`, `dir-col` or `dir-col-reverse` and the stylesheet
turns that class into `flex-direction`; `wrap` adds `wrap` / `wrap-reverse`. The generic `col` /
`rev` pair `.flex` understands is **not** emitted: `dir` already names the axis, and a second statement
of it is a second thing that can disagree. `use_common_css` leaves `Tiles` alone for the same
reason — it adds `col` to every other vertical container, and `dir: row` has to be able to say row.

## Why `rows` became `layers`

`rows` named where the header sat, and that stopped being true the moment a direction made the header
a column. A **layer** is one line of header cells along the field axis: rows when the fields run
across (`dir: col*`), columns when they run down (`dir: row*`). Two layers are the group line plus
the leaf line, still derived from the field keys (`price/retail` under `price`) and still refused
beyond two at decode. A direction word in the model would have been the same mistake the whole
change removes.

## What it costs, and what it does not touch

`window` and `step` are untouched: a window still selects entries by key, whichever way they flow.
`.tiles-row` still names the block that holds an entry's cells — it is a row only when the items
flow down, and renaming it would churn every stylesheet for a word. `Field.width` is a CSS track
size, so under `dir: row` it sizes a row; the spelling is the frame's, the model does not grow a
second one. A strip with no fields needs neither fields nor a header: `dir: row-reverse` is a strip
read backwards, `dir: col` the vertical list — and both are the same node with a different value.

## The rejected alternative

`direction: row | col` on the frame: one axis and no reverse. It cannot say a reversed list without
a separate class — which is exactly the CSS-defined axis this record removes — and it says nothing
about the fields, so the fields' axis would still have to be inferred from markup.

## A dead end that is not an error

`wrap-reverse` with no `fields` flips an axis nothing is laid out on: there is no header to move and
no field line to reverse. It is a **silent no-op**, not a load error — the value is redundant, not
impossible, and refusing it would make a frame that is merely over-specified fail to load. The
renderer emits the class anyway, so a frame's own CSS can still use it.