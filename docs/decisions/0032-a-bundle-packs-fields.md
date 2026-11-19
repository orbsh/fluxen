# 0032 — A bundle packs fields; a field is named by its `tag`

Status: **Accepted** (2026-11-19). Follows ADR 0031: one value has a slot and a
scope, and each node declares the channels it accepts.

Five changes, one idea: a group of inputs that leaves together is a thing, and
the things inside it are named by it rather than by their own addresses.

## 1. `form` becomes `bundle`

A form is an HTML element; what the frame needed was "pack these fields into one
object and send it". `bundle` names that and nothing else: it owns the exit
(`sink`) and the timing (`attrs.submit`), not the fields. The name also stops
promising a `<form>` in the DOM — the renderer draws a `div`.

## 2. A field's name is its `tag`, not its `sink` slot

The old walk read each child's `sink.slot` as the field name, which made one
address mean two things: where a value lands and which field it is. They are not
the same. `slot` says where a value lands (ADR 0031 §1-2); `tag` says which field
it is inside a bundle. So a field can keep its own local landing, and two bundles
can pack fields that share a slot, without the pack changing shape.

A `tag` is what opts a field into its bundle. No tag: the input keeps its own
exit and sends on Enter, exactly as before. The bundle's `id` marks the
submission (the instance), never the fields.

## 3. Timing lives on the bundle, and it is a list

When to send is one question per submission, so it belongs where the exit is.
Modes combine — `submit: [enter, button]` is a chat box, `[change]` is live
packing (autocomplete, a draft that mirrors as it is typed) — so the field is a
set. Absent means `[enter]`. A bare string is accepted for the one-mode case.
Timing is not `scope`: `scope` says where a value lands, `submit` says when the
group leaves; they are separate axes and separate fields.

The mode is resolved by the nearest enclosing bundle that declares it: one
interaction, one send. Nesting therefore cannot multiply uplinks, and a bundle
that wants the whole object live just leaves the inner bundle without `change`.

## 4. Bundles nest, and the walk stops there

A nested bundle contributes exactly one key to the outer object — its `tag`, or
its `id` when there is no tag — whose value is its own packed object. The walk
does not descend into it: the inner bundle's fields are its own, and flattening
them into the outer object would make the two packs the same object with
different keys. This is what makes

```yaml
bundle (submit: [enter, button]) > bundle (submit: [change], tag: mention) > input (tag: q)
```

an autocomplete that sends as you type, inside a form that sends on Enter.

## 5. `textarea` is an input

It had the same channels as `input` and differed only in height. Multi-line is a
layout of a field, so it is `multiline: true` — one node, one field name, one set
of channels. The `placeholder` channel moves to `input` with it.

## Rejected: timing on the field

`instant: true` on an input (the shape `FormAttr` hinted at) is the alternative we
did not take. It puts the submission's clock on one field, so one live field drags
every field of the form along: the sender must still assemble the whole object,
and every other field silently receives an exit it never declared. Timing is a
property of the group, so it lives on the group.

## What it costs

Every frame that used `form` or `textarea` is rewritten (mechanical), and the
packed payload loses the per-field `{data: …}` wrapper: a submission is now
`{tag: value, …}`.