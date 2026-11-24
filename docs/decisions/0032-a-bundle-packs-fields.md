# 0032 — A bundle packs fields; a field is named by its `tag`

Status: **Accepted** (2026-11-19). Follows ADR 0031: one value has a slot and a
scope, and each node declares the channels it accepts. Revised 2026-11-24: a field has no uplink of its
own any more (§2), and the bundle decides what its fields become after a send and how many sends there are
(§6, §7).

One idea: a group of inputs that leaves together is a thing, and the things inside it are named by it
rather than by their own addresses — so **sending, clearing and counting are the bundle's**, in one place.

## 1. `form` becomes `bundle`

A form is an HTML element; what the frame needed was "pack these fields into one
object and send it". `bundle` names that and nothing else: it owns the exit
(`sink`) and the timing (`attrs.submit`), not the fields. The name also stops
promising a `<form>` in the DOM — the renderer draws a `div`.

## 2. A field's name is its `tag`, not its `sink` slot

The old walk read each child's `sink.slot` as the field name, which made one
address mean two things: where a value lands and which field it is. They are not
the same. `slot` says where a value lands (ADR 0031 §1-2); `tag` says which field
it is inside a bundle. So two bundles can pack fields that share a slot without the
pack changing shape.

A `tag` is what opts a field into its bundle. **No tag = no uplink.** There used to be a second path: a tag-less
field sent through its own `sink` on Enter, and cleared itself, both hardcoded in the widget. That was a rule
about sending living in a field, which is the very thing this ADR moved to the bundle, so it is gone. Clearing
went with it, and came back as the bundle's `reset` (§6). The bundle's `id` marks the submission (the instance),
never the fields.

**A field has no `sink` at all** (decided and implemented 2026-11-24). Keeping it for a `local` landing made one
node answer two questions — "which field is this" and "where does the page state get written" — and it left the
schema allowing a shape the runtime then had to warn about. What a field keeps is what a field is: `id`, `tag`,
`attrs`, `source` (what it shows) and `kind` (what kind of field it is: element type and parse, one declaration —
`number`, `bool`, `text`, `password`; absent means text). The one thing that leaves with `sink` is the field's part
in an emit: the `id` an outflow carries is the emitting node's, and a field no longer emits.

**Which id a submission carries.** An outflow is `{event, id?, data}` and the `id` is the emitting node's
(ADR 0010): it marks a *request*, so a server that published that form can match the answer, and two submissions
of the same form stay apart even when their payloads look alike. The emitter is the bundle now, so the bundle is
what needs an `id`. Inside the form a value is named by its `tag` — a form does not need ids to tell its own
fields apart, and the far side reads them by name. The field's `id` stays in the schema, but only as node
identity: it is the key an item keeps when a field sits in a list (ADR 0031 §4), and what `patch`/`remove`
address. Nothing about it goes over the wire. `kind` is spelled that way because `type` is already the node's own discriminator on the wire,
and it is not the read's or the write's: a field has one type. The local landing a field used to own now belongs
to a bundle whose own sink is `local` with `submit: ["change"]` — one wrapper, the same cost §4 describes, and the
count and the reset come with it. `Bind` lost `type` in the same move, and `Input`/`Select` refuse unknown keys,
so a page still carrying the old `sink` hears about it instead of quietly losing the write. A downstream SDK that
refused that dead shape in its own build step (reflux's `_check_uplinks`) drops it at the same time, because it
was the same rule kept in a second place. Tracked as scratch ticket #470.

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

A nested bundle contributes exactly one key to the outer object — its `tag` — whose value is its own packed
object. `tag` here is the same rule a field follows, because a nested bundle *is* an input of the enclosing
bundle: the thing that packs it names it. Neither of the bundle's other two words may stand in for it. `id` is
this submission's identity — a server matches the answer by it — so letting it double as a key would move the
parent's shape whenever someone gave a nested bundle an id, and the parent would appear to change because of a
fact about replies. `dest` is where the inner bundle's *own* sends go, which is a different question: in the
autocomplete below the inner group sends to `mentions` while the outer object calls it `mention`, and it must
be possible to say both. A nested bundle with no `tag` therefore packs nothing into its parent: its fields are
its own, and it sends them itself if it has a `dest`. (`tag` is read only when nesting — on a top-level bundle
it says nothing.)

The walk does not descend into a named nested bundle either: the inner bundle's fields are its own, and
flattening them into the outer object would make the two packs the same object with different keys. This is what makes

```yaml
bundle (submit: [enter, button]) > bundle (submit: [change], tag: mention) > input (tag: q)
```

an autocomplete that sends as you type, inside a form that sends on Enter.

That example is a design decision, not a side effect: **one input can answer more than one question about
when to send, and it does so by wearing more than one bundle.** The field itself has no exit (§2), so the
input does not need a mode switch — the inner bundle decides the suggestion's exit, the outer one the
message's, and neither knows about the other. The nearest declaring bundle wins (§3), so a change never
reaches the outer bundle and nesting cannot multiply sends. The cost is that every input which leaves the
page sits inside a bundle, one wrapper deep; what that buys is a single place where sending is decided,
which is where the count and the reset live too (§7, §6). The page-author's version of this paragraph is in
`docs/interaction-patterns.md` §4.

## 5. `textarea` is an input

It had the same channels as `input` and differed only in height. Multi-line is a
layout of a field, so it is `multiline: true` — one node, one field name, one set
of channels. The `placeholder` channel moves to `input` with it.

## 6. After a send, the fields become one of three things

`attrs.reset` is one word with three exclusive values, because a field cannot be kept and cleared at the
same time:

| value | after a send |
| --- | --- |
| `keep` (absent) | what it was — the behaviour every bundle had |
| `clear` | the empty value **of its own shape**: `""`, `0`, `false`, `{}` |
| `default` | the value it started with (`source.default`, typed by the field) |

The bundle asks it of **every field it packed**, nested bundles included: a submission is one object, so the
aftermath is one decision, and a parent that clears must empty the box its nested bundle owns — otherwise the
chat box inside an autocomplete would keep its text after the message left. A nested bundle that sends on its
own applies its own mode over its own subtree and never reaches up. The reset runs post-order, so a child's
cleared pack lands in the parent's slot for it before the parent refreshes. `clear` only makes sense with `submit: [enter, button]` — a
`change`-mode bundle that clears would empty the field on every keystroke — so it is documented, not
policed; the field is the wrong place to enforce a timing decision.

One thing a reset has to get past: once a reader has typed, the element keeps its own text and its own
`checked`, and a binding that writes the string it wrote last time is skipped — so clearing the signal is
not clearing the box. The field therefore pushes its signal onto the element (`value`, or `checked` for a
switch) whenever the two disagree, which is a no-op in the ordinary case where they already match.

## 7. `sends` is a count, and `-1` is no count at all

A bundle may be granted a number of sends: `attrs.sends: n`. Each send that actually leaves (a `sink`
exists) takes one off; at zero the bundle's fields are `disabled` and `submit` refuses, so a form that is
spent looks spent instead of silently doing nothing. Absent is unlimited, and `-1` says the same thing
out loud; `0` means locked from the start, which is how a "you already sent this" echo is drawn without a
second node type.

This count is **local state of the page**, not a permission. It stops the same reader pressing twice; the
authority on whether a thing may still change is the far side, and it still says no. Nothing about it is
persisted: a reload starts the count again.

## Rejected: timing on the field

`instant: true` on an input (the shape `FormAttr` hinted at) is the alternative we
did not take. It puts the submission's clock on one field, so one live field drags
every field of the form along: the sender must still assemble the whole object,
and every other field silently receives an exit it never declared. Timing is a
property of the group, so it lives on the group.

## What it costs

Every frame that used `form` or `textarea` is rewritten (mechanical), and the
packed payload loses the per-field `{data: …}` wrapper: a submission is now
`{tag: value, …}`. A field that used to send on its own needs a bundle around it — one wrapper for one
field is the price of having one rule ("the bundle is the exit") instead of two, and the demo chat box is
exactly that shape.