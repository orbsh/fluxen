//! 容器组件与叶子组件入口。宏 `gen_dispatch!` 分派到 `crate::components::*_`，
//! 故此处 re-export 所有组件函数，使它们在 `crate::components` 命名空间可见。
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