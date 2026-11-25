# 0039 — A read and a write are not one type

Status: **Accepted** (2026-11-24). Implemented in the same commit; nothing is shipped to users yet, so the old
shape is not kept under a second name.

---

## 1. The problem: one struct, two directions

ADR 0031 §2 split one channel into two — `source` in, `sink` out — and gave both the same struct (`Bind`), because
at the time there was **one** channel to spell and splitting the shape as well would have been two changes in one
decision. That was the right order to make the changes in, and the wrong place to stop.

Once the directions are named separately, the shared struct is visibly a compromise:

- **`default` is a read-only idea.** It answers "what do I show when there is no value". A write has no such
  question — it either has a value to send or it does not send — and nothing in the renderer ever read
  `sink.default`. Two live examples still carried it (`sink: {slot: channel::select, default: "2"}` and
  `sink: {slot: ok, default: Count}`): dead keys, kept alive only by the schema allowing them.
- **The write's address was spelled `slot`.** ADR 0023 already settled the word for an uplink target — `dest` —
  and the frame it produces says so (`Outflow { dest, id?, data }`). So `ctx.emit` translated `slot` into `dest`
  at the moment of sending, which is a rename that exists only to reconcile two names for one thing.
- **The type said the two directions were alike.** They are alike in two of four fields (`scope`, `path`) and
  unlike in the rest; a frame author reading the schema learns nothing about which fields matter to which
  direction.

## 2. The decision

Two types, one per direction:

```yaml
source: { slot: "draft", scope: both, path: "/body", default: "untitled" }   # Source
sink:   { dest: "message", scope: both, path: "/body" }                      # Sink
```

- `Source { slot?, scope = remote, path?, default? }` — where a value comes from, who is authoritative about it,
  which member of it, what to show when there is none.
- `Sink { dest?, scope = remote, path? }` — where a value goes, who is authoritative about it, which member it
  owns.
- `dest` is the same word the outflow carries (ADR 0023), so emitting is a pass-through instead of a translation.
  A `sink` with no `dest` stays legal — it is an address-less write, and validation accepts it — but it no
  longer has a job: it was how a button said "I am the confirm key", and `Button` has no `sink` at all
  (amended 2026-11-25, §2a below).
- Both refuse unknown keys, so the two shapes that used to decode and mean nothing now fail loudly:
  `sink: {slot: "x"}` (a write with a read's address) and `sink: {dest: "x", default: 1}` (a write with a read's
  fallback).
- What did **not** change: `scope` is one closed enumeration on both sides (`remote | local | both`, ADR 0031 §1),
  and `slot` still means the same thing wherever it appears — the page-side name a value lives under.

### 2a. The button's marker is gone (amended 2026-11-25)

`Button` had a `sink` that was never a channel: the renderer read only whether it was present, to decide
"draw this button at all", and the press went to the enclosing loom's sink, payload and id
(`ui_core::hooks::submit`). So the field's `dest`, `scope` and `path` were dead, and the demo layer had to
write an inert address into every confirm key just to make the button visible.

The information was already in one place: `loom.attrs.submit` names the timing, and `nearest(loom,
SubmitMode::Button)` finds the loom a button belongs to. A button therefore carries `id`, `attrs` and
`source` (its label) — nothing else. Outside a button-submitting loom it draws nothing, exactly as before,
and the decision is no longer the button's to make.

## 3. What it deletes

- `sink.default`, and the two dead keys that were riding on it.
- The `slot → dest` translation in `ctx.emit`.
- The claim, in the type, that a read and a write are interchangeable. The trait keeps `get_source`/`get_sink` as
  two methods returning two types, which is what a node that accepts one and not the other already meant.

## 4. What it costs

A breaking schema change across the example corpus and any host that builds frames from its own code: every
`sink: {slot: X}` becomes `sink: {dest: X}`. Mechanical, and cheap now — nothing is shipped, and the alternative
is a schema that keeps teaching the wrong thing about two directions that were deliberately made distinct.

## 5. Known wart, recorded rather than fixed

`path` on a **remote** sink is still ignored: `emit` writes the member locally but sends the whole payload up. It
is legal because a `both` sink uses the local half of it; it is unused because nothing has asked for the remote
half yet. When that changes the decision belongs to the send side, not to this type.