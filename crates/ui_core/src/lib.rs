//! ui-core — fluxen's rendering core: the Accrete tree becomes DOM, and nothing else.
//!
//! This crate is what a HOST reuses. It knows three things and refuses the rest:
//!
//! - the vocabulary (`accrete` + `content`): nodes, the eight actions, `Message` / `Outflow`;
//! - the seam (`transport::Transport`): an object that emits frames and yields raw bytes —
//!   the core never learns which socket, which codec negotiation, or which URL;
//! - the rendering itself: `Ctx` (layout / data / list / vals / drag signals), `render::dispatch`
//!   and the components.
//!
//! It deliberately does NOT know: the page shell (`#main` attributes, `window.fluxen.send`), any
//! concrete transport implementation, or the application's layout. Those are the boot's job and
//! live in `examples/renderer` — the reference host — so that a host wrapping this core in its own
//! binary (prism) sits in exactly the same position as our own bundle. See
//! `docs/decisions/0021-external-integration.md` and `docs/external-integration.md`.

// Unit tests may use the unwrap/panic vocabulary the production targets are
// denied (a panicking test is a failing test). `cfg(test)` is on only for this
// crate's own test build; `tests/` crates set the same line at their own root.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn))]
pub mod components;
pub mod ctx;
pub mod dom;
pub mod hooks;
pub mod render;
pub mod widgets;

pub use ctx::Ctx;