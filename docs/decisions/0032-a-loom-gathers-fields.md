# 0032 — A loom gathers fields; a field is named by its `tag`

Status: **Accepted** (2026-11-19). Follows ADR 0031: one value has a slot and a
scope, and each node declares the channels it accepts. Revised 2026-11-24: a field has no uplink of its
own any more (§2), and the loom decides what its fields become after a send and how many sends there are
(§6, §7). Renamed 2026-11-25: the node was `bundle` when this ADR was accepted; §1 records both naming
steps and why, and the file slug follows the current name.

One idea: a group of inputs that leaves together is a thing, and the things inside it are named by it
rather than by their own addresses — so **sending, clearing and counting are the loom's**, in one place.

## 1. `form` becomes `bundle`, then `loom`

The node began as `form`. A form is an HTML element, and what the frame needed was "gather these fields into one object and send it"; the renderer draws a `div`, so the name promised something the DOM does not have — and HTML forbids nested forms while a loom nests inside a loom by design (§4). `form` went.

`bundle` replaced it on 2026-11-19 and said one third of the job: things tied together. What the node does is gather its fields, name each one by its `tag`, and send the result as one object. `bundle` is silent on the gathering and on the naming — and the naming is the whole idea (§2) — so it read as a container beside `frame`, `panel` and `tiles`, the words that already mean "a box holding things".

`loom` says the three parts at once: the strands are the fields, the weave pattern is `tag` → key, the cloth is the object that leaves. A loom also nests the way this node nests — a weave inside a weave contributes one panel of the cloth. `bus` was rejected: a bus is a shared path that belongs to nobody, it does not nest, and in this family the word already means Prism. `trunk` was rejected too: a trunk carries toward the root while this payload leaves the tree, and the word is already the build tool's in this repository.

What the node owns is the exit (`sink`), the timing (`attrs.submit`), what its fields become after a send (`attrs.reset`) and how many sends there are (`attrs.sends`) — not the fields' addresses. That division is what §2 onwards is about.

## 2. A field's name is its `tag`, not its `sink` slot

The old walk read each child's `sink.slot` as the field name, which made one
address mean two things: where a value lands and which field it is. They are not
the same. `slot` says where a value lands (ADR 0031 §1-2); `tag` says which field
it is inside a loom. So two looms can pack fields that share a slot without the
pack changing shape.

A `tag` is what opts a field into its loom. **No tag = no uplink.** There used to be a second path: a tag-less
field sent through its own `sink` on Enter, and cleared itself, both hardcoded in the widget. That was a rule
about sending living in a field, which is the very thing this ADR moved to the loom, so it is gone. Clearing
went with it, and came back as the loom's `reset` (§6). The loom's `id` marks the submission (the instance),
never the fields.

**A field has no `sink` at all** (decided and implemented 2026-11-24). Keeping it for a `local` landing made one
node answer two questions — "which field is this" and "where does the page state get written" — and it left the
schema allowing a shape the runtime then had to warn about. What a field keeps is what a field is: `id`, `tag`,
`attrs`, `source` (what it shows) and `kind` (what kind of field it is: element type, parse, and — for a number — the
interval it admits; one declaration, absent means free text). `kind` is a sum type now: a bare word for the kinds that
declare nothing, a map with a `type` for the ones that do (ADR 0050). The one thing that leaves with `sink` is the field's part
in an emit: the `id` an outflow carries is the emitting node's, and a field no longer emits.

**Which id a submission carries.** An outflow is `{event, id?, data}` and the `id` is the emitting node's
(ADR 0010): it marks a *request*, so a server that published that form can match the answer, and two submissions
of the same form stay apart even when their payloads look alike. The emitter is the loom now, so the loom is
what needs an `id`. Inside the form a value is named by its `tag` — a form does not need ids to tell its own
fields apart, and the far side reads them by name. The field's `id` stays in the schema, but only as node
identity: it is the key an item keeps when a field sits in a list (ADR 0031 §4), and what `patch`/`remove`
address. Nothing about it goes over the wire. `kind` is spelled that way because `type` is already the node's own discriminator on the wire,
and it is not the read's or the write's: a field has one type. The local landing a field used to own now belongs
to a loom whose own sink is `local` with `submit: ["change"]` — one wrapper, the same cost §4 describes, and the
count and the reset come with it. `Bind` lost `type` in the same move, and `Input`/`Picker` refuse unknown keys,
so a page still carrying the old `sink` hears about it instead of quietly losing the write. A downstream SDK that
refused that dead shape in its own build step (reflux's `_check_uplinks`) drops it at the same time, because it
was the same rule kept in a second place. Tracked as scratch ticket #470.

## 3. When the group leaves lives on the loom; how often a keystroke is heard lives on the field

When to send is one question per submission, so it belongs where the exit is.
Modes combine — `submit: [enter, button]` is a chat box, `[change]` is live
packing (autocomplete, a draft that mirrors as it is typed) — so the field is a
set. Absent means `[enter]`. A bare string is accepted for the one-mode case.
Timing is not `scope`: `scope` says where a value lands, `submit` says when the
group leaves; they are separate axes and separate fields.

The mode is resolved by the nearest enclosing loom that declares it: one
interaction, one send. Nesting therefore cannot multiply uplinks, and a loom
that wants the whole object live just leaves the inner loom without `change`.

The other half of timing is the field's: `emit` (§8) says how often a keystroke
reaches the loom, which is how often a `[change]` submit can fire at all. Two
owners, two claims — a field cannot create a submission, and a loom cannot make
typing arrive faster than the field says.

## 4. Looms nest, and the walk stops there

A nested loom contributes exactly one key to the outer object — its `tag` — whose value is its own packed
object. `tag` here is the same rule a field follows, because a nested loom *is* an input of the enclosing
loom: the thing that packs it names it. Neither of the loom's other two words may stand in for it. `id` is
this submission's identity — a server matches the answer by it — so letting it double as a key would move the
parent's shape whenever someone gave a nested loom an id, and the parent would appear to change because of a
fact about replies. `dest` is where the inner loom's *own* sends go, which is a different question: in the
autocomplete below the inner group sends to `mentions` while the outer object calls it `mention`, and it must
be possible to say both. A nested loom with no `tag` therefore packs nothing into its parent: its fields are
its own, and it sends them itself if it has a `dest`. (`tag` is read only when nesting — on a top-level loom
it says nothing.)

The walk does not descend into a named nested loom either: the inner loom's fields are its own, and
flattening them into the outer object would make the two packs the same object with different keys. This is what makes

```yaml
loom (submit: [enter, button]) > loom (submit: [change], tag: mention) > input (tag: q)
```

an autocomplete that sends as you type, inside a form that sends on Enter.

That example is a design decision, not a side effect: **one input can answer more than one question about
when to send, and it does so by wearing more than one loom.** The field itself has no exit (§2), so the
input does not need a mode switch — the inner loom decides the suggestion's exit, the outer one the
message's, and neither knows about the other. The nearest declaring loom wins (§3), so a change never
reaches the outer loom and nesting cannot multiply sends. The cost is that every input which leaves the
page sits inside a loom, one wrapper deep; what that buys is a single place where sending is decided,
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
| `keep` (absent) | what it was — the behaviour every loom had |
| `clear` | the empty value **of its own shape**: `""`, `0`, `false`, `{}` |
| `default` | the value it started with (`source.default`, typed by the field) |

The loom asks it of **every field it packed**, nested looms included: a submission is one object, so the
aftermath is one decision, and a parent that clears must empty the box its nested loom owns — otherwise the
chat box inside an autocomplete would keep its text after the message left. A nested loom that sends on its
own applies its own mode over its own subtree and never reaches up. The reset runs post-order, so a child's
cleared pack lands in the parent's slot for it before the parent refreshes. `clear` only makes sense with `submit: [enter, button]` — a
`change`-mode loom that clears would empty the field on every keystroke — so it is documented, not
policed; the field is the wrong place to enforce a timing decision.

One thing a reset has to get past: once a reader has typed, the element keeps its own text and its own
`checked`, and a binding that writes the string it wrote last time is skipped — so clearing the signal is
not clearing the box. The field therefore pushes its signal onto the element (`value`, or `checked` for a
switch) whenever the two disagree, which is a no-op in the ordinary case where they already match.

## 7. `sends` is a count, and `-1` is no count at all

A loom may be granted a number of sends: `attrs.sends: n`. Each send that actually leaves (a `sink`
exists) takes one off; at zero the loom's fields are `disabled` and `submit` refuses, so a form that is
spent looks spent instead of silently doing nothing. Absent is unlimited, and `-1` says the same thing
out loud; `0` means locked from the start, which is how a "you already sent this" echo is drawn without a
second node type.

This count is **local state of the page**, not a permission. It stops the same reader pressing twice; the
authority on whether a thing may still change is the far side, and it still says no. Nothing about it is
persisted: a reload starts the count again.

## Timing on the field: a rate, not an exit

`instant: true` on an input — the shape `FormAttr` hinted at — was rejected, and it should have been: a
field that declares "I submit" hands every other field of the group an exit it never asked for, because
a submission packs the whole object.

`emit` (§8) is a different claim, and it survived that objection. It does not say a submission happens;
it says how often one keystroke is *heard*, so it can only ever reduce sends. The loom still owns the
exit and still decides whether a change is a send at all (`submit: [change]`) — which is why a field with
`emit` and a loom with only `[enter]` is not a contradiction but the most common shape: the value is heard
as it settles, the send waits for Enter.

What the split buys is that the two knobs answer the two questions separately. A search box wants
`{debounce: 300}` and one frame per pause; a live preview wants `{throttle: 50}` so it keeps moving without
a frame per letter; a composer wants the 250 ms default and Enter to flush it immediately (a submit cancels
whatever is pending and packs what is on screen, so `[enter, change]` never sends a value older than the
keypress that preceded it).

## 8. A field is heard at most as often as it says

`emit` is the field's clock: `{debounce: ms}` notifies once the typing has been quiet that long, carrying the
last value; `{throttle: ms}` notifies at once and then at most once per window, the window's last value
landing when it closes. Closed and one arm at a time — the two answer different questions, and a frame asking
for both is asking for a shape nobody uses (`{debounce: 1, throttle: 2}` does not decode).

What is timed is only the *notification*: the field's own signal is written on the keystroke, so typing never
lags behind a window. Absent means `{debounce: 250}`, and that default is what makes `submit: [change]` usable
at all — without it a per-keystroke submit spends a `sends: 1` grant on the first character, clears a
`reset: clear` box mid-word, and puts one frame per letter on the wire. `{debounce: 0}` is the way back to a
frame per keystroke, for the frames that want it.

Two rules keep the clock honest. A send cancels whatever is pending — `submit` packs the fields' current
values, so a flush is just "send now", and Enter or a button press never carries a stale value. A reset cancels
too, because the value a pending timer would carry is the one the reset just cleared.

## What it costs

Every frame that used `form` or `textarea` is rewritten (mechanical), and the
packed payload loses the per-field `{data: …}` wrapper: a submission is now
`{tag: value, …}`. A field that used to send on its own needs a loom around it — one wrapper for one
field is the price of having one rule ("the loom is the exit") instead of two, and the demo chat box is
exactly that shape.