//! What to draw, decided once, as plain numbers.
//!
//! This module is the seam between the layout and the graphics API. `build` is a
//! pure function: it takes the camera, the scene and the style, and returns a
//! `Frame` — a viewer (position, view and projection matrices, viewport) plus
//! per-instance transforms for nodes and for links, with every decision already
//! made (size in world units, colour, dimming, link width). The backend's only
//! job is to upload a `Frame` and draw two batches, which means:
//!
//! - a reviewer reads `build` to see *what the viewer is shown*, instead of
//!   tracing GL state through a renderer;
//! - every rule here is testable on the host, no GPU and no browser;
//! - swapping the backend cannot change the picture, because no decision lives
//!   in the backend.
//!
//! Links are geometry in the scene, never 2D pixels painted over it. Each link
//! is a parallelogram whose across-the-link axis is computed in world units from
//! the pixel width the style asks for at that link's depth: constant on-screen
//! width, real depth, real occlusion. Geometry in the scene is what glow, flow
//! and curvature are made of later.

use crate::camera::{Basis, Camera, Z_FAR, Z_NEAR};
use crate::math::Vec3;
use crate::scene::Scene;
use crate::style::{self, Style};

/// Everything the backend needs to know about the eye. Matrices are
/// column-major, ready for the GPU.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub position: Vec3,
    pub view: [f32; 16],
    pub projection: [f32; 16],
    /// Drawing-buffer size in device pixels.
    pub viewport_px: (u32, u32),
    pub z_near: f32,
    pub z_far: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            view: [0.0; 16],
            projection: [0.0; 16],
            viewport_px: (1, 1),
            z_near: Z_NEAR,
            z_far: Z_FAR,
        }
    }
}

/// One instance: where it goes (column-major transform), what colour (sRGB with
/// alpha).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instance {
    pub transform: [f32; 16],
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub view: View,
    pub nodes: Vec<Instance>,
    pub links: Vec<Instance>,
    /// Viewport in CSS pixels: what the pointer, the picking and the labels use.
    pub viewport: (f32, f32),
}

/// Decide the frame. No GL, no DOM, no clock.
///
/// `viewport` is CSS pixels; `dpr` is the device pixel ratio, so the render
/// target is sized in device pixels while every pixel-sized decision stays in
/// CSS pixels.
pub fn build(
    camera: &Camera,
    viewport: (f32, f32),
    dpr: f32,
    scene: &Scene,
    style: &Style,
) -> Frame {
    let focused = scene.focus.is_some() && !scene.focus_points().is_empty();
    let max_weight = scene.max_weight();
    let basis = camera.basis();

    let mut nodes = Vec::with_capacity(scene.bodies.len());
    for (index, body) in scene.bodies.iter().enumerate() {
        let Some(screen) = camera.project(viewport, body.pos) else {
            // Behind the near plane: a sphere there would be a page-filling
            // blob, so it is not in the frame at all.
            continue;
        };
        let radius = style.radius_px(body.weight, max_weight) / screen.px_per_unit.max(1e-6);
        let mut color = node_color(scene, style, index);
        if focused && !scene.is_focused(index) {
            for channel in &mut color {
                *channel *= 0.35;
            }
        }
        nodes.push(Instance {
            transform: scale_translate(body.pos, radius),
            color: [color[0], color[1], color[2], 1.0],
        });
    }

    Frame {
        view: View {
            position: camera.eye(),
            view: camera.view_matrix(),
            projection: camera.projection_matrix(viewport),
            viewport_px: (
                (viewport.0 * dpr).round().max(1.0) as u32,
                (viewport.1 * dpr).round().max(1.0) as u32,
            ),
            z_near: Z_NEAR,
            z_far: Z_FAR,
        },
        nodes,
        links: build_links(camera, viewport, &basis, scene, style, focused),
        viewport,
    }
}

/// One instance per link, mapping the unit quad (u along the link, v across it)
/// onto the world-space parallelogram the link occupies.
fn build_links(
    camera: &Camera,
    viewport: (f32, f32),
    basis: &Basis,
    scene: &Scene,
    style: &Style,
    focused: bool,
) -> Vec<Instance> {
    let half_px = (style.link_width_px * 0.5).max(0.4);
    let alpha = style.link_opacity.clamp(0.05, 1.0);
    let mut out = Vec::with_capacity(scene.links.len());
    for link in &scene.links {
        let (Some(a), Some(b)) = (scene.bodies.get(link.a), scene.bodies.get(link.b)) else {
            continue;
        };
        let (Some(sa), Some(sb)) = (
            camera.project(viewport, a.pos),
            camera.project(viewport, b.pos),
        ) else {
            continue;
        };
        let (dx, dy) = (sb.x - sa.x, sb.y - sa.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.5 {
            // Both ends land on the same pixel: a quad there is a dot, not a
            // line, and it only adds noise.
            continue;
        }
        // The perpendicular of the projected segment, taken back along the
        // camera's right/up so it is a real world direction; scaling it by the
        // pixel width at the link's midpoint depth keeps the on-screen width
        // constant.
        let (ux, uy) = (dx / len, dy / len);
        let across = basis.right * -uy + basis.up * ux;
        let mid_depth = (sa.depth + sb.depth) * 0.5;
        let px_per_unit = camera.px_for(viewport, 1.0, mid_depth).max(1e-6);
        let width = (2.0 * half_px) / px_per_unit;
        let side = across.unit().unwrap_or(Vec3::ZERO) * width;

        let mut color = node_color(scene, style, link.a);
        if focused && !(scene.is_focused(link.a) && scene.is_focused(link.b)) {
            for channel in &mut color {
                *channel *= 0.3;
            }
        }
        out.push(Instance {
            transform: [
                // u axis: the link itself.
                b.pos.x - a.pos.x,
                b.pos.y - a.pos.y,
                b.pos.z - a.pos.z,
                0.0,
                // v axis: across the link, in world units.
                side.x,
                side.y,
                side.z,
                0.0,
                // the quad is flat, so the third axis only has to be a unit
                // vector for the instance matrix to stay invertible.
                0.0,
                0.0,
                1.0,
                0.0,
                // origin.
                a.pos.x,
                a.pos.y,
                a.pos.z,
                1.0,
            ],
            color: [color[0], color[1], color[2], alpha],
        });
    }
    out
}

/// Column-major `translate(pos) * scale(radius)`.
fn scale_translate(pos: Vec3, radius: f32) -> [f32; 16] {
    [
        radius, 0.0, 0.0, 0.0, //
        0.0, radius, 0.0, 0.0, //
        0.0, 0.0, radius, 0.0, //
        pos.x, pos.y, pos.z, 1.0,
    ]
}

fn node_color(scene: &Scene, style: &Style, index: usize) -> [f32; 3] {
    scene
        .nodes
        .get(index)
        .and_then(|n| n.color.as_deref())
        .and_then(style::parse_hex)
        .unwrap_or_else(|| {
            scene
                .nodes
                .get(index)
                .map(|n| style.color_for(n.kind.as_deref().unwrap_or("")))
                .unwrap_or([0.3, 0.3, 0.3])
        })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::scene::Snapshot;

    fn scene_of(nodes: usize, links: &[(&str, &str)]) -> Scene {
        let mut scene = Scene::default();
        scene.apply(Snapshot {
            nodes: (0..nodes)
                .map(|i| crate::scene::Node {
                    id: format!("n{i}"),
                    name: Some(format!("n{i}")),
                    weight: Some(4.0),
                    ..Default::default()
                })
                .collect(),
            links: links
                .iter()
                .map(|(a, b)| crate::scene::GraphLink {
                    source: (*a).to_string(),
                    target: (*b).to_string(),
                    ..Default::default()
                })
                .collect(),
            focus: None,
        });
        scene
    }

    fn scale_of(transform: &[f32; 16]) -> f32 {
        (transform[0] * transform[0] + transform[1] * transform[1] + transform[2] * transform[2])
            .sqrt()
    }

    /// One instance per visible node, one per link: the backend draws exactly
    /// what the frame says, and nothing else.
    #[test]
    fn a_frame_has_one_instance_per_node_and_per_link() {
        let scene = scene_of(3, &[("n0", "n1"), ("n1", "n2")]);
        let frame = build(&Camera::default(), (800.0, 600.0), 1.0, &scene, &Style::default());
        assert_eq!(frame.nodes.len(), 3);
        assert_eq!(frame.links.len(), 2);
    }

    /// Node size is a pixel size the style asked for; the frame converts it to
    /// world units at that node's own depth, so zooming cannot change how big a
    /// node looks.
    #[test]
    fn node_size_is_a_pixel_size_converted_at_its_own_depth() {
        let scene = scene_of(2, &[]);
        let camera = Camera::default();
        let style = Style::default();
        let frame = build(&camera, (800.0, 600.0), 1.0, &scene, &style);
        for (index, instance) in frame.nodes.iter().enumerate() {
            let screen = camera
                .project((800.0, 600.0), scene.bodies[index].pos)
                .expect("visible");
            let as_px = scale_of(&instance.transform) * screen.px_per_unit;
            let wanted = style.radius_px(scene.bodies[index].weight, scene.max_weight());
            assert!((as_px - wanted).abs() < 0.01, "{as_px} vs {wanted}");
        }
    }

    /// A link is a world-space quad: its across-the-link axis has a real length
    /// in world units, and that length projects to the pixel width asked for. A
    /// clip-space quad would have no such world size to check.
    #[test]
    fn link_width_is_world_space_and_projects_to_the_pixel_width_asked_for() {
        let scene = scene_of(2, &[("n0", "n1")]);
        let camera = Camera::default();
        let style = Style::default();
        let frame = build(&camera, (800.0, 600.0), 1.0, &scene, &style);
        let link = frame.links[0];
        let across = Vec3::new(link.transform[4], link.transform[5], link.transform[6]);

        let (a, b) = (scene.bodies[0].pos, scene.bodies[1].pos);
        let mid = (a + b) * 0.5;
        let depth = camera
            .project((800.0, 600.0), mid)
            .expect("visible")
            .depth;
        let px = camera.px_for((800.0, 600.0), across.len(), depth);
        assert!(
            (px - style.link_width_px).abs() < 0.2,
            "width {px} px, wanted {}",
            style.link_width_px
        );
    }

    /// A focus directive dims what it does not name — decided here, not in the
    /// backend, so every backend dims the same way.
    #[test]
    fn focus_dims_the_nodes_and_links_it_does_not_name() {
        let mut scene = scene_of(2, &[("n0", "n1")]);
        scene.focus = Some(crate::scene::Focus {
            ids: vec!["n0".to_string()],
            ..Default::default()
        });
        let frame = build(&Camera::default(), (800.0, 600.0), 1.0, &scene, &Style::default());
        let lit = frame.nodes[0].color[0];
        let dim = frame.nodes[1].color[0];
        assert!(dim <= lit, "focused {lit} vs dimmed {dim}");
        assert!(frame.links[0].color[3] < 1.0, "links keep their alpha");
    }

    /// The render target follows the device pixel ratio, while the CSS-pixel
    /// size stays what the pointer and the labels use.
    #[test]
    fn the_viewport_carries_both_css_and_device_pixels() {
        let scene = scene_of(1, &[]);
        let frame = build(&Camera::default(), (800.0, 600.0), 2.0, &scene, &Style::default());
        assert_eq!(frame.view.viewport_px, (1600, 1200));
        assert_eq!(frame.viewport, (800.0, 600.0));
    }
}