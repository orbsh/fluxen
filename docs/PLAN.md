# Fluxen Plan

## Known defects

### Whole-map notification fan-out

`data` / `list` are single signals holding a whole HashMap. With `Arc`-wrapped
entries the clone cost is gone (a read copies the map shell plus refcounts),
but any `Set`/`Join` frame still notifies every widget bound to any key, and
each notified re-run re-clones the shell.

Fix (deferred until scale demands it): per-key signals — the outer map holds
`Arc<RwSignal<...>>` per source name (key set nearly static), so an update
notifies only its own readers.
