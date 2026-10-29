//! Fluxen facade — the rendering core, under the one name a consumer depends on.
//!
//! It is the CORE, always: a boot belongs to a host (ours lives at
//! `examples/renderer`), so nothing here re-exports one or hides it behind a
//! feature. See `docs/decisions/0021-external-integration.md`.
pub use ui_core::*;
