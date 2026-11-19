//! 容器组件与叶子组件入口。宏 `gen_dispatch!` 分派到 `crate::components::*_`，
//! 故此处 re-export 所有组件函数，使它们在 `crate::components` 命名空间可见。
pub mod frame;
pub use frame::frame_;
pub mod canvas;
pub use canvas::canvas_;
pub mod forcegraph;
pub use forcegraph::forcegraph_;
pub mod dock;
pub use dock::dock_;
pub mod grab;
pub use grab::grab_;
pub mod layer;
pub use layer::layer_;
pub mod fold;
pub use fold::fold_;
pub mod bundle;
pub use bundle::bundle_;
pub mod popup;
pub use popup::popup_;

pub mod panel;
pub use panel::panel_;

pub mod tiles;
pub use tiles::tiles_;
pub mod record;
pub use record::record_;
pub mod render;
pub use render::template_;
pub mod svg;
pub use svg::{group_, path_, svg_};

pub use crate::widgets::*;
