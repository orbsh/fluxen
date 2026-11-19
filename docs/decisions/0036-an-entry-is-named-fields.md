# 0036 — A group of named data is a node: `entry`

Status: **Accepted** (2026-11-19).

An `entry` is one item of a list: a group of named data. The model could have said that with
a map — the shape is what JSON reaches for — and the map is wrong here, because a value in a map
is data the renderer can neither address nor style nor give a channel to. An `entry` is a node
whose children are its fields, and a field's name is that child's `attrs.selector`.

---

## Why the name is the child's `selector`

`selector` is already how this renderer addresses a thing that is not an element, and a field is
not an element: it has no tag of its own (the grid draws the cell), so it needs an address, and
every node already carries one. A map key would be a second way to name a node, sitting next to
the one the rest of the frame language uses for exactly this.

## Why a field's value must be a node

A field is not a value, it is a *thing that shows* a value: it has a class, a selector of its
own, a channel (`source`), and sometimes children (a cell holding a list). All of that is the
node vocabulary that already exists. If the value were a scalar in a map, each of those would
need a second spelling, and the second spelling would grow a second set of rules. Text is
`{type: text, source: {...}}`; a number is the same node with a number in it.

## The nesting rule

A field whose direct children are all entries holds a **list of entries**, and a grid draws it
as child rows under the row it belongs to. That is the whole of "hierarchy instead of spans"
(ADR 0031 §4): the data already says which row a row belongs to, so the markup does not merge
cells to say it again, and nothing is indented by a value the model cannot see.

An entry that names itself (`id`) keeps that name across removals and re-sorts. One that does
not is addressed by where it arrived (ADR 0031 §4, "Every item has a key").

## The rejected alternatives

**A `{name: value}` map.** Cheapest to write, and it cannot be extended: the key is not an
address, so a cell cannot be styled by the frame's own vocabulary; a field cannot carry a
channel, so a cell that reads a slot has no place to say so; and a field that is missing, empty,
or a different kind of value are all the same absence.

**Parallel arrays with a header list** — `{fields: [name, price], rows: [[ada, 12], …]}`. The
names live in a different place from the values, which makes every row positional: reorder one
entry in `fields` and every value in every row silently moves to another field. A row that is
one cell short shifts the same way, and nothing at load can tell. A field is named, not
numbered, or it is not a field.