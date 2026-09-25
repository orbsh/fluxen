# Fluxen Plan

## Known defects (carried over from the dioxus port)

### Keyed list rendering in `rack_`

`ui_leptos/src/components/rack.rs` computes a `key` per child and discards it
(`let _ = key;`). Every frame appended to `ctx.list[source]` rebuilds the whole
`Vec<AnyView>`, so DOM nodes are destroyed and recreated:

- input focus is lost when a chat stream pushes a new frame
- scroll position resets (the `scroll` hack only compensates partially)

Fix: render the list with `For` + stable key (`Brick::get_id()`), so each item
is a separately-tracked keyed view and appending only mounts new nodes.

### Form signal passing via thread-local stack

`ui_leptos/src/hooks.rs` passes `FormState` signals from `form_` to nested
`input_`/`button_` through a thread-local `FORM_STACK`, because the
`BindVariant::Field` signal field was removed together with the dioxus
feature. It works only while `form_` renders children eagerly in the same
call; any lazy/streamed child rendering breaks the stack discipline.

Fix candidates (undecided): resolve form fields through `Ctx` (a field-signal
registry keyed by form node), or wire leptos context providers through the
dispatch path so children can look up the enclosing form.

### Whole-map clone on every source read

`use_source` / `use_source_list` / `rack_` call `.get()` on the `data` /
`list` signals, cloning the entire HashMap per widget per re-run. A single
`Set` frame re-runs every bound widget with a full copy.

Fix: subscribe with derived signals (`Signal::derive` per source name), or
split storage into per-key signals so a `Set` only notifies its own readers.
