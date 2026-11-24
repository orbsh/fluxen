//! Entry point for container and leaf components. The `gen_dispatch!` macro dispatches to `crate::components::*_`,
//! so every component function is re-exported here, making them visible in the `crate::components` namespace.
pub mod frame;
pub use frame::frame_;
pub mod canvas;
pub use canvas::canvas_;
pub mod forcegraph;
pub use forcegraph::forcegraph_;
pub mod bundle;
pub use bundle::bundle_;

pub mod panel;
pub use panel::panel_;

pub mod tiles;
pub use tiles::tiles_;
pub mod entry;
pub use entry::entry_;
pub mod render;
pub use render::stamp_;
pub mod svg;
pub use svg::{path_, svg_};

pub use crate::widgets::*;