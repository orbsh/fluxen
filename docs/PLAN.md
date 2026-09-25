# Fluxen Plan

## Known defects

### Keyed list rendering in `rack_`

`ui_leptos/src/components/rack.rs` computes a `key` per child and discards it
(`let _ = key;`). Every frame appended to `ctx.list[source]` rebuilds the whole
`Vec<AnyView>`, so DOM nodes are destroyed and recreated:

- input focus is lost when a chat stream pushes a new frame
- scroll position resets (the `scroll` hack only compensates partially)

Fix: render the list with `For` + stable key (`Brick::get_id()`), so each item
is a separately-tracked keyed view and appending only mounts new nodes.

### Whole-map notification fan-out

`data` / `list` are single signals holding a whole HashMap. With `Arc`-wrapped
entries the clone cost is gone (a read copies the map shell plus refcounts),
but any `Set`/`Join` frame still notifies every widget bound to any key, and
each notified re-run re-clones the shell.

Fix (deferred until scale demands it): per-key signals — the outer map holds
`Arc<RwSignal<...>>` per source name (key set nearly static), so an update
notifies only its own readers.
