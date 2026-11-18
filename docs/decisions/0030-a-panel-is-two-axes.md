# 0030 — A panel is one container with two axes; a mount point is a name

Status: Accepted (2026-11-16)

`fold`, `popup` and `layer` (ADR-0022, 0026, 0014) were three components for one idea: a content
area that appears and disappears. They were filed by what they looked like, not by what decided
them, and that filing left a hole: a wide sidebar that slides in from the right edge of the
viewport — a drawer — was neither. `popup` is anchored to its trigger and `layer` has no trigger
at all (it mirrors a slot), so a drawer had to be either a popup bent out of shape or a layer that
cannot hear the button that should open it.

## The two axes

- **Place** — `inline` (opening it pushes the page down), `anchor` (over the trigger, the page
  does not move), `viewport` (over everything, at a named edge, with a mask).
- **State** — `toggle` (a local switch: the trigger opens, a click outside / Esc closes) or
  `source` (the panel mirrors a bound slot: non-empty means on screen, ADR-0012's "empty is
  absent", and dismissing clears that slot).

Those are four cells. Three existed; the fourth — `viewport` + `toggle` — is the drawer, and it is
now spelled. A frame says `panel` and two attributes; which of the four it gets is a consequence,
not a vocabulary choice.

## The three names are gone from the vocabulary

`fold`, `popup` and `layer` are no longer node types: a frame cannot write them, and the tests that
did now say `panel`. Nothing in this repo used them by the time this landed, so removing them broke
no frame.

Their IMPLEMENTATIONS are still in `components/` and `panel` converts itself into their shapes
before handing over. That is deliberate: those three have been exercised in production shapes, and
rewriting working code in the same change that renames it makes any regression impossible to
attribute. Moving them into `panel.rs` is a separate, mechanical change with no vocabulary in it.

## There is no portal, and no mount points

`portal` (ADR 0014) moved a subtree into another DOM element found by selector, and this decision
had started giving those targets names. It is gone instead, because the problem it solved does not
exist here: in Vue a component's state is trapped inside the component, so rendering it elsewhere
needs DOM surgery; here a slot is global. The server writes a slot, whatever binds that slot draws
it — anywhere in the frame. A notice from an island appears in the frame's drawer because the drawer
binds the notice slot, not because anything travelled. The only thing portal still did — the same
subtree drawn twice — is two components binding the same path.

That leaves ONE binding mechanism to keep honest: a path with an owner (see the follow-up on
unifying `source` and `local`). Content does not move; it is read from where it already lives.
