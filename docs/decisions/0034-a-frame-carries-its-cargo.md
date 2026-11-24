# 0034 — A frame carries its cargo; `group`, `grab` and `dock` are one node

Status: **Accepted** (2026-11-19).

`group` was a frame with no `class`, `grab` a frame with `kind`/`payload`, `dock` a frame that
emits. Three nodes for one box, and the distinction they encoded was never in the box — it was in
what the frame author had to know. All three go into **`frame`**; `template` becomes **`stamp`**
(it instantiates a registered definition and prints it, which is what a stamp does).

---

## The decision

Cargo lives on the frame, in the box's own `attrs`:

```yaml
- type: frame
  id: m42
  attrs: { draggable: { tag: message } }      # this box may be dragged away
  children: [...]                             # …and this subtree is the cargo
- type: frame
  id: rail
  attrs: { accept: { tag: message } }         # this box takes that cargo
  sink: { dest: pin }                         # …and this is where the drop is reported
```

A `frame` owns a subtree, so it can say "this is the thing that moves" without a second field
describing what moves: the cargo **is** the rendered subtree, read at `dragstart` from the node
itself. `grab.kind` becomes `draggable.tag`; `grab.payload` disappears (no second bag to keep in
step); `dock.attrs.accept` (a list of kinds) becomes `accept: {tag}`, since a landing takes one kind
or any. `dock.expand` is not replaced: whether a landing is a thin strip that grows or a box that
was always there is CSS, and that is where it went.

The two directions share the box but not a switch: a frame may be a source, a landing, or both.
A drop must emit, so `accept` without `sink` is a **load error** (`dead_end_drop` in stage), next to
the "read-only node with a `sink`" check.

## Why not on any node

`draggable` on any node would let a `text` be dragged, and an attribute cannot own cargo: the node
it sits on is a value, not a container, so "what is being dragged" would have to be named a second
time. Every box that matters is already a frame — a bubble, a card, a row of a board — so the
general case buys nothing the frame does not cover, and costs a rule ("some nodes can carry a
payload") with no other reason to exist.

## Why not a `dock` inside `bundle`

A `bundle` also collects values and also emits, so "a drop is a bundle with one field" is tempting.
The timings conflict: a drop is a finished gesture — it happens once, at release, and carries
whatever was in the air — while a `bundle` sends on `submit`, which is the user saying "this form is
done" and may fire many times over the life of the page. Folding one into the other would make the
landing's event name a function of a submit policy, and a frame that lands nothing would still be a
form.

## The known gap

An `entry` row can be neither a drag source nor wrapped. `entry` is not a `frame`, so `draggable`
has nowhere to live on it; wrapping it in a frame would put a box between the grid and its rows, and
the row would stop being a row of that grid (the tracks are the container's, and every row is a
direct child). Making a row draggable waits for a rule that says a grid may address a row through
one wrapper — until then, a table row is not a drag handle.