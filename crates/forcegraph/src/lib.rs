//! Force-directed 3D graph core: camera, layout, picking.
//!
//! The renderer is ours, not a wrapped JS library: this crate holds the
//! maths that a graph view needs and nothing else. It knows no framework,
//! no DSL and no protocol — it takes world positions in, gives screen
//! coordinates out, and can be exercised on the host (no WebGL needed).
//!
//! Layout model:
//! - [`Camera`] is an orbit camera around a target; [`Camera::project`] maps a
//!   world point to viewport pixels and reports the local world→pixel scale,
//!   which is how node and link sizes stay constant in CSS pixels regardless
//!   of camera distance.
//! - [`force::step`] advances a 3D force layout (charge repulsion, link
//!   springs, centring, damping). Plain O(n²): the visible node budget is
//!   tens, and a Barnes-Hut octree stays out until a measurement asks for it.
//! - [`Scene`] turns a payload snapshot into bodies plus links, preserving the
//!   position of every node whose id survives — an append must not re-shuffle
//!   the layout, because spatial memory is what makes the view navigable.
//! - [`pick`] resolves a pointer position to a node by projecting, not by
//!   reading pixels back from the GPU.

pub mod camera;
pub mod force;
pub mod math;
pub mod pick;
pub mod scene;
pub mod style;

pub mod render;
pub mod view;

pub use view::View;

pub use camera::{Camera, Screen};
pub use math::Vec3;
pub use style::Style;
pub use scene::{Diff, Focus, GraphLink, Node, Scene, Snapshot};