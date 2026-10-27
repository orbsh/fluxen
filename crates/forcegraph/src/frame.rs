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
    /// One camera-facing quad of light per node, drawn with a radial falloff
    /// texture: the halo the ball sits in. Derived from `scene.glow_clock`, so a
    /// frame is still a pure function of the scene.
    pub haloes: Vec<Instance>,
    /// The hot centre on the ball itself. The halo is the spill around it; a light
    /// with only spill reads as a soft disc, not as a source.
    pub cores: Vec<Instance>,
    /// The glass over the selected node: a very faint filled disc. It is the body
    /// of the bubble; the edge of the glass is `dome_rims`.
    pub domes: Vec<Instance>,
    /// The rim of that glass: a thin circle just outside the node's light, which is
    /// what actually makes it read as a cover rather than as more glow.
    pub dome_rims: Vec<Instance>,
    /// Whether the halo adds light (`true`) or tints the surface (`false`).
    /// Light only exists on a dark panel — adding to white leaves white — so this
    /// follows the theme, and the light theme gets a tint instead.
    pub glow_additive: bool,

    /// Viewport in CSS pixels: what the pointer, the picking and the labels use.
    pub viewport: (f32, f32),
}

/// Halo radius as a multiple of the node's own radius: the light reaches much
/// further than the ball it surrounds.
const HALO_SCALE: f32 = 4.6;

/// How far the halo swells and shrinks around `HALO_SCALE` over one breath, and how
/// much its brightness rides along: size alone reads as a slow zoom, size plus
/// light reads as breathing.
const HALO_BREATH: f32 = 0.26;
const HALO_PULSE: f32 = 0.45;

/// Opacity of a resting halo, and of the one that marks the selection. The light
/// is additive on a dark panel, so these are how bright it burns, not how opaque
/// it is.
/// A second, much tighter light right on the ball: the halo is the spill, this is
/// the source. Without a hot centre the whole thing reads as one soft disc.
const CORE_SCALE: f32 = 1.7;

/// The halo's alpha comes from `style.glow_alpha` — one knob for "how much light",
/// which the core then doubles.
const CORE_GAIN: f32 = 2.0;

/// How much the paths reachable from a highlight stand out. Brightness, not
/// darkness: the rest of the graph is left exactly as it was.
const REACH_GAIN: f32 = 1.7;
const REACH_SCALE: f32 = 1.12;
const REACH_INK: f32 = 1.5;
const REACH_WIDTH: f32 = 1.45;
const HALO_SELECTED_GAIN: f32 = 1.6;

/// The glass over the selection, as a multiple of the selection's own halo. Just
/// inside the halo's edge rather than beyond it: the ring is a frame around the
/// light, and drawn any wider it starts to read as a separate object.
const DOME_SCALE: f32 = 0.95;

/// The body of the glass. Almost nothing — a cover is mostly what it lets through.
const DOME_ALPHA: f32 = 0.05;

/// Its rim, which is the whole effect: glass is visible at the edge, where the
/// surface turns away, and nowhere else.
const DOME_RIM_ALPHA: f32 = 0.30;

/// The selected halo is also a little wider, so "marked" survives a frame caught
/// at the bottom of the breath.
const HALO_SELECTED_SCALE: f32 = 1.18;

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
    let reach = if style.highlight_reach && focused {
        reachable(scene, style.reach_hops)
    } else {
        (Default::default(), Default::default())
    };
    let max_weight = scene.max_weight();
    let basis = camera.basis();

    let mut nodes = Vec::with_capacity(scene.bodies.len());
    let mut haloes = Vec::new();
    let mut cores = Vec::new();
    let mut domes = Vec::new();
    let mut dome_rims = Vec::new();
    for (index, body) in scene.bodies.iter().enumerate() {
        let Some(screen) = camera.project(viewport, body.pos) else {
            // Behind the near plane: a sphere there would be a page-filling
            // blob, so it is not in the frame at all.
            continue;
        };
        let radius = style.radius_px(body.weight, max_weight) / screen.px_per_unit.max(1e-6);
        // A focus directive names what to look at; it does not black out the rest.
        // Dimming everything else made the panel read as "one thing matters, the
        // graph is decoration" — the graph is the point.
        let color = node_color(scene, style, index);
        // The node itself is a ball with a surface — shaded, so it has a bright
        // side and a rim — and the light around it is a separate billboard, which
        // is the only way to get a soft falloff: a sphere has an edge, light does
        // not.
        nodes.push(Instance {
            transform: scale_translate(body.pos, radius),
            color: [color[0], color[1], color[2], 1.0],
        });
        let wave = (scene.breath_phase(index) * std::f32::consts::TAU).sin();
        let breath = 1.0 + HALO_BREATH * wave;
        let pulse = 1.0 + HALO_PULSE * wave;
        let selected = scene.is_highlighted(index);
        let on_path = reach.0.contains(&index);
        let mut halo_scale = HALO_SCALE * breath;
        let mut halo_alpha = style.glow_alpha;
        halo_alpha *= pulse;
        if on_path && !selected {
            // On a path from the selection: brighter and a shade wider. Nothing is
            // taken away from the rest of the graph to make this visible.
            halo_alpha = (halo_alpha * REACH_GAIN).min(1.0);
            halo_scale *= REACH_SCALE;
        }
        if selected {
            halo_scale *= HALO_SELECTED_SCALE;
            halo_alpha = style.glow_alpha * HALO_SELECTED_GAIN * pulse;
        }
        if style.theme == crate::style::Theme::Light {
            // Light cannot be added to a white page: the halo becomes a tint, and
            // a faint tint of a muted colour over white is invisible, so it takes
            // more of it.
            halo_alpha *= 1.5;
        }
        haloes.push(Instance {
            // `billboard` takes the square's full side, and the quad runs
            // -0.5..0.5, so the side is twice the halo radius.
            transform: billboard(body.pos, &basis, 2.0 * radius * halo_scale),
            color: [color[0], color[1], color[2], halo_alpha.min(1.0)],
        });
        // The core is the light source: brighter than its own colour, tighter than
        // the halo, riding the same breath. A hot centre is what makes the thing
        // read as a source instead of a soft disc, and it is exactly what a bloom
        // pass would have to extract if we had one.
        //
        // Dark theme only. On a light background there is nothing for the core to be
        // brighter than — a white centre over white is invisible, and darkening it
        // instead just makes a second ball.
        if style.theme == crate::style::Theme::Dark {
            let core_alpha = (style.glow_alpha * CORE_GAIN * pulse).min(1.0);
            let core_scale = if selected { CORE_SCALE * 1.15 } else { CORE_SCALE };
            cores.push(Instance {
                transform: billboard(body.pos, &basis, 2.0 * radius * core_scale),
                color: [
                    (color[0] * 0.55 + 0.45).min(1.0),
                    (color[1] * 0.55 + 0.45).min(1.0),
                    (color[2] * 0.55 + 0.45).min(1.0),
                    core_alpha,
                ],
            });
        }
        if selected {
            // The glass does not breathe: it is a frame the light moves inside.
            let dome_side = 2.0 * radius * HALO_SCALE * HALO_SELECTED_SCALE * DOME_SCALE;
            domes.push(Instance {
                transform: billboard(body.pos, &basis, dome_side),
                color: [color[0], color[1], color[2], halo_alpha * DOME_ALPHA],
            });
            dome_rims.push(Instance {
                transform: billboard(body.pos, &basis, dome_side),
                color: [color[0], color[1], color[2], DOME_RIM_ALPHA.min(1.0)],
            });
        }
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
        links: build_links(camera, viewport, &basis, scene, style),
        haloes,
        cores,
        domes,
        dome_rims,
        glow_additive: matches!(style.theme, crate::style::Theme::Dark),
        viewport,
    }
}

/// Column-major transform for a camera-facing square of side `size` at `center`.
/// Facing the camera is computed on the CPU because the halo's whole job is to
/// stay round on screen: a sphere-shaped glow would change width as the camera
/// turns, a billboard does not.
fn billboard(center: Vec3, basis: &Basis, size: f32) -> [f32; 16] {
    let right = basis.right * size;
    let up = basis.up * size;
    [
        right.x, right.y, right.z, 0.0, //
        up.x, up.y, up.z, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        center.x, center.y, center.z, 1.0,
    ]
}

/// The nodes and links reachable from whatever is highlighted (selected or
/// focused), out to `hops` edges. Highlighting a node's reach is how the panel says
/// "this is connected to that" without dimming everything else — the paths light
/// up, the rest of the graph simply stays itself.
pub(crate) fn reachable(scene: &Scene, hops: u32) -> (std::collections::HashSet<usize>, std::collections::HashSet<usize>) {
    use std::collections::{HashSet, VecDeque};
    let mut nodes: HashSet<usize> = HashSet::new();
    let mut links: HashSet<usize> = HashSet::new();
    let mut frontier: VecDeque<(usize, u32)> = scene
        .bodies
        .iter()
        .enumerate()
        .filter(|(index, _)| scene.is_highlighted(*index))
        .map(|(index, _)| (index, 0))
        .collect();
    while let Some((node, depth)) = frontier.pop_front() {
        if !nodes.insert(node) {
            continue;
        }
        // The node at the limit is still highlighted; it just does not expand.
        if depth >= hops {
            continue;
        }
        for (index, link) in scene.links.iter().enumerate() {
            let other = if link.a == node {
                Some(link.b)
            } else if link.b == node {
                Some(link.a)
            } else {
                None
            };
            if let Some(other) = other {
                links.insert(index);
                frontier.push_back((other, depth + 1));
            }
        }
    }
    (nodes, links)
}

/// Where a link leaves a node. `Straight` runs centre to centre; the two port
/// shapes send the link out of one of a fixed set of directions instead, chosen
/// per link by which one points most nearly at the other node — the four of a
/// tetrahedron, or the six of a coordinate frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkShape {
    #[default]
    Straight,
    Axes,
    Tetra,
}

const AXIS_PORTS: [Vec3; 6] = [
    Vec3 { x: 1.0, y: 0.0, z: 0.0 },
    Vec3 { x: -1.0, y: 0.0, z: 0.0 },
    Vec3 { x: 0.0, y: 1.0, z: 0.0 },
    Vec3 { x: 0.0, y: -1.0, z: 0.0 },
    Vec3 { x: 0.0, y: 0.0, z: 1.0 },
    Vec3 { x: 0.0, y: 0.0, z: -1.0 },
];

const TETRA_PORTS: [Vec3; 4] = [
    Vec3 { x: 0.577, y: 0.577, z: 0.577 },
    Vec3 { x: 0.577, y: -0.577, z: -0.577 },
    Vec3 { x: -0.577, y: 0.577, z: -0.577 },
    Vec3 { x: -0.577, y: -0.577, z: 0.577 },
];

/// Segments per curved link. Enough that the bend reads as a curve at graph
/// distances, few enough that a 44-link graph is still one instanced batch.
const CURVE_SEGMENTS: usize = 12;

/// How far the control points reach along their port direction, as a fraction of
/// the straight-line distance. Below ~0.3 the curve is a straight line with a
/// wobble; far above it the link loops before it leaves the node.
const CURVE_REACH: f32 = 0.45;

/// The port of this shape that points most nearly along `direction`.
fn nearest_port(shape: LinkShape, direction: Vec3) -> Vec3 {
    let candidates: &[Vec3] = match shape {
        LinkShape::Axes => &AXIS_PORTS,
        LinkShape::Tetra => &TETRA_PORTS,
        LinkShape::Straight => return direction.unit().unwrap_or(Vec3::ZERO),
    };
    candidates
        .iter()
        .copied()
        .fold(None::<Vec3>, |best, port| match best {
            Some(current) if current.dot(direction) >= port.dot(direction) => Some(current),
            _ => Some(port),
        })
        .unwrap_or(Vec3::ZERO)
}

/// The points a link is drawn through: two for a straight line, a sampled cubic
/// for a curve. The curve is a cubic with its control points pushed along each
/// node's port, so it leaves and arrives along the port instead of merely being
/// bent between two centres.
fn link_path(a: Vec3, b: Vec3, a_radius: f32, b_radius: f32, shape: LinkShape) -> Vec<Vec3> {
    if shape == LinkShape::Straight {
        return vec![a, b];
    }
    let a_port = nearest_port(shape, b - a);
    let b_port = nearest_port(shape, a - b);
    let start = a + a_port * a_radius;
    let end = b + b_port * b_radius;
    let reach = (end - start).len() * CURVE_REACH;
    let c1 = start + a_port * reach;
    let c2 = end + b_port * reach;
    (0..=CURVE_SEGMENTS)
        .map(|step| {
            let t = step as f32 / CURVE_SEGMENTS as f32;
            let u = 1.0 - t;
            start * (u * u * u)
                + c1 * (3.0 * u * u * t)
                + c2 * (3.0 * u * t * t)
                + end * (t * t * t)
        })
        .collect()
}

/// One instance per link segment: each maps the unit quad (u along the segment, v
/// across it) onto the world-space parallelogram that segment occupies. A straight
/// link is one segment; a curved one is `CURVE_SEGMENTS` of them.
fn build_links(
    camera: &Camera,
    viewport: (f32, f32),
    basis: &Basis,
    scene: &Scene,
    style: &Style,
) -> Vec<Instance> {
    let half_px = (style.link_width_px * 0.5).max(0.4);
    // The tube is opaque, so the old alpha knob becomes ink density: a muted link
    // is a darker line, not a see-through one.
    let ink = 0.25 + 0.75 * style.link_alpha();
    let max_weight = scene.max_weight();
    let mut out = Vec::with_capacity(scene.links.len());
    let reach = if style.highlight_reach && scene.is_highlighted_any() {
        reachable(scene, style.reach_hops).1
    } else {
        Default::default()
    };
    for (link_index, link) in scene.links.iter().enumerate() {
        let (Some(a), Some(b)) = (scene.bodies.get(link.a), scene.bodies.get(link.b)) else {
            continue;
        };
        let a_radius = style.radius_px(a.weight, max_weight)
            / camera
                .project(viewport, a.pos)
                .map(|p| p.px_per_unit)
                .unwrap_or(1.0)
                .max(1e-6);
        let b_radius = style.radius_px(b.weight, max_weight)
            / camera
                .project(viewport, b.pos)
                .map(|p| p.px_per_unit)
                .unwrap_or(1.0)
                .max(1e-6);
        let path = link_path(a.pos, b.pos, a_radius, b_radius, style.link_shape);

        let mut color = node_color(scene, style, link.a);
        if reach.contains(&link_index) {
            for channel in &mut color {
                *channel = (*channel * REACH_INK).min(1.0);
            }
        }

        for pair in path.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let along = to - from;
            let length = along.len();
            let Some(direction) = along.unit() else {
                continue;
            };
            let (Some(sa), Some(sb)) = (
                camera.project(viewport, from),
                camera.project(viewport, to),
            ) else {
                continue;
            };
            let (dx, dy) = (sb.x - sa.x, sb.y - sa.y);
            if (dx * dx + dy * dy).sqrt() < 0.5 {
                // Both ends land on the same pixel: a tube there is a dot, not a
                // line, and it only adds noise.
                continue;
            }
            let mid_depth = (sa.depth + sb.depth) * 0.5;
            let px_per_unit = camera.px_for(viewport, 1.0, mid_depth).max(1e-6);
            let on_path = reach.contains(&link_index);
            let radius = half_px / px_per_unit * if on_path { REACH_WIDTH } else { 1.0 };
            // A round cross-section needs no "up": the tube is rotationally
            // symmetric, which is exactly why a bent link keeps one width at every
            // angle. A flat quad does not — the same quad seen along its edge is a
            // line and seen face-on is a band.
            let right = basis.right.unit().unwrap_or(Vec3 { x: 1.0, y: 0.0, z: 0.0 });
            let up = direction.cross(right).unit().unwrap_or(basis.up);
            let right = up.cross(direction).unit().unwrap_or(right);
            // `CpuMesh::cylinder` runs along x over [0,1] with radius 1, so the
            // instance maps x onto the segment and y/z onto the radius.
            out.push(Instance {
                transform: [
                    direction.x * length,
                    direction.y * length,
                    direction.z * length,
                    0.0,
                    right.x * radius,
                    right.y * radius,
                    right.z * radius,
                    0.0,
                    up.x * radius,
                    up.y * radius,
                    up.z * radius,
                    0.0,
                    from.x,
                    from.y,
                    from.z,
                    1.0,
                ],
                color: [color[0] * ink, color[1] * ink, color[2] * ink, 1.0],
            });
        }
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

    /// One instance per visible node, and one per drawn link segment: the backend
    /// draws exactly what the frame says, and nothing else.
    #[test]
    fn a_frame_has_one_instance_per_node_and_per_link() {
        let scene = scene_of(3, &[("n0", "n1"), ("n1", "n2")]);
        let mut style = Style::default();
        style.link_shape = LinkShape::Straight;
        let frame = build(&Camera::default(), (800.0, 600.0), 1.0, &scene, &style);
        assert_eq!(frame.nodes.len(), 3);
        assert_eq!(frame.links.len(), 2);

        // Routed links are sampled, so the batch grows with the curve, not with
        // the link count.
        style.link_shape = LinkShape::Axes;
        let frame = build(&Camera::default(), (800.0, 600.0), 1.0, &scene, &style);
        // Sampled, not one-per-link. The exact count depends on how many segments
        // land on more than a pixel, which is a property of the positions, not of
        // the routing.
        assert!(
            frame.links.len() > 2,
            "routed links should be sampled into several quads, got {}",
            frame.links.len()
        );
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
            // The instance is the ball, so its scale is the picking radius: what
            // the pointer hits and what the eye sees are the same circle.
            let wanted = style.radius_px(scene.bodies[index].weight, scene.max_weight());
            assert!((as_px - wanted).abs() < 0.01, "{as_px} vs {wanted}");
        }
    }

    /// A link is a world-space tube: its radius has a real length in world units,
    /// and that length projects to half the pixel width asked for — the diameter is
    /// the width, the instance only carries the radius.
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
        let diameter_px = 2.0 * camera.px_for((800.0, 600.0), across.len(), depth);
        assert!(
            (diameter_px - style.link_width_px).abs() < 0.2,
            "width {diameter_px} px, wanted {}",
            style.link_width_px
        );
    }

    /// Every node sits in a halo, the halo is a billboard, and the selection is
    /// the one that burns brighter and wider — that difference is the only thing
    /// saying "this node".
    #[test]
    fn every_node_sits_in_a_halo_and_the_selection_burns_brighter() {
        let camera = Camera::default();
        let mut scene = scene_of(3, &[]);
        let style = Style::default();
        let frame = build(&camera, (800.0, 600.0), 1.0, &scene, &style);
        assert_eq!(frame.haloes.len(), scene.bodies.len(), "one halo per node");
        let rest = frame.haloes[0].color[3];

        // The halo is a billboard: its x axis is the camera's right.
        let x = Vec3::new(
            frame.haloes[0].transform[0],
            frame.haloes[0].transform[1],
            frame.haloes[0].transform[2],
        )
        .unit()
        .expect("unit");
        assert!(
            x.dot(camera.basis().right).abs() > 0.99,
            "halo is not camera-facing: {x:?}"
        );

        scene.select(Some(1));
        let frame = build(&camera, (800.0, 600.0), 1.0, &scene, &style);
        let lit = frame.haloes[1];
        assert!(lit.color[3] > rest, "the selection is not brighter");
        assert!(
            scale_of(&lit.transform) > scale_of(&frame.haloes[0].transform),
            "the selection is not wider"
        );
        assert!(frame.haloes[0].color[3] <= rest + 1e-6, "the rest changed");
        // And it is the only node with a shield around it.
        assert_eq!(frame.domes.len(), 1, "no glass over the selection");
        assert_eq!(frame.dome_rims.len(), 1, "glass without an edge is haze");
        let glass = scale_of(&frame.domes[0].transform);
        // The ring frames the light: at the halo's edge, never wider than it and
        // never smaller than the ball.
        let halo = scale_of(&lit.transform);
        assert!(glass < halo, "the ring sits outside the halo: {glass} vs {halo}");
        assert!(glass > halo * 0.8, "the ring is not at the edge of the light");
        let ball = scale_of(&frame.nodes[1].transform);
        assert!(ball * 2.0 < glass, "the ball is bigger than the ring");
        assert_eq!(
            glass,
            scale_of(&frame.dome_rims[0].transform),
            "the rim does not sit on the glass"
        );
        // Glass is the edge; the body is nearly nothing.
        assert!(frame.domes[0].color[3] < frame.dome_rims[0].color[3] * 0.25);
        scene.select(None);
        let frame = build(&camera, (800.0, 600.0), 1.0, &scene, &style);
        assert!(frame.domes.is_empty());
        assert!(frame.dome_rims.is_empty());
    }

    /// The haloes must not breathe in step: each node's phase and tempo come from
    /// its own id, so the graph shimmers instead of pulsing like one lamp.
    #[test]
    fn the_nodes_do_not_breathe_in_step() {
        let scene = scene_of(4, &[]);
        let phases: Vec<f32> = (0..4).map(|i| scene.breath_phase(i)).collect();
        for (i, a) in phases.iter().enumerate() {
            for b in phases.iter().skip(i + 1) {
                assert!((a - b).abs() > 1e-3, "two nodes share a phase: {phases:?}");
            }
        }
        // And the offsets do not drift: the same clock, the same answer.
        assert_eq!(phases, (0..4).map(|i| scene.breath_phase(i)).collect::<Vec<_>>());
    }

    /// A link between two nodes on the x axis, in a shape with ports, leaves
    /// through the port that points at the other node — and arrives the same way.
    /// A straight link is one quad; a routed one is a run of them.
    #[test]
    fn a_routed_link_leaves_through_the_nearest_port() {
        let mut scene = scene_of(2, &[("n0", "n1")]);
        scene.bodies[0].pos = Vec3::new(0.0, 0.0, 0.0);
        scene.bodies[1].pos = Vec3::new(120.0, 0.0, 0.0);
        let camera = Camera::default();

        let mut straight = Style::default();
        straight.link_shape = LinkShape::Straight;
        let straight_links = build(&camera, (800.0, 600.0), 1.0, &scene, &straight).links;
        assert_eq!(straight_links.len(), 1, "a straight link is one quad");

        let mut routed = Style::default();
        routed.link_shape = LinkShape::Axes;
        let links = build(&camera, (800.0, 600.0), 1.0, &scene, &routed).links;
        assert_eq!(links.len(), CURVE_SEGMENTS, "a curve is sampled");

        // The first segment starts off the centre, along +x — the port that faces
        // the other node — and not towards y or z.
        let first = links[0].transform;
        assert!(first[12] > 0.0, "the link starts behind the node");
        assert!(first[13].abs() < 1.0 && first[14].abs() < 1.0);
        // It heads towards the other node, not away from it.
        assert!(first[0] > 0.0, "the first segment does not point at the far node");

        // The tetrahedron shape has no axis ports, so the same pair leaves on a
        // diagonal: all three components are non-zero.
        routed.link_shape = LinkShape::Tetra;
        let links = build(&camera, (800.0, 600.0), 1.0, &scene, &routed).links;
        assert_eq!(links.len(), CURVE_SEGMENTS);
        let first = links[0].transform;
        assert!(
            first[12].abs() > 0.1 && first[13].abs() > 0.1 && first[14].abs() > 0.1,
            "a tetra port is a body diagonal"
        );
    }

    /// Everything drawn around a node is a fixed multiple of that node's own
    /// radius, so the proportions never change with size and the ball is never the
    /// biggest thing in the frame.
    #[test]
    fn light_is_a_constant_multiple_of_the_ball() {
        let mut scene = scene_of(2, &[]);
        scene.bodies[0].weight = 1.0;
        scene.bodies[1].weight = 9.0;
        let frame = build(&Camera::default(), (800.0, 600.0), 1.0, &scene, &Style::default());
        let mut ratios = Vec::new();
        for index in 0..2 {
            let ball = scale_of(&frame.nodes[index].transform);
            let halo = scale_of(&frame.haloes[index].transform);
            let core = scale_of(&frame.cores[index].transform);
            assert!(halo > ball * 2.0, "the ball outgrew its light: {ball} vs {halo}");
            // The core is the bright thing: bigger than the ball, much smaller
            // than the halo it sits inside.
            assert!(core > ball && core < halo * 0.5, "core {core} vs ball {ball}");
            ratios.push(halo / ball);
        }
        // The halo also breathes, and each node breathes on its own phase, so the
        // ratio is a constant *envelope* — `2 × HALO_SCALE` breathing by
        // `HALO_BREATH` — not one number.
        let nominal = 2.0 * HALO_SCALE;
        let swing = nominal * HALO_BREATH;
        for ratio in &ratios {
            assert!(
                (*ratio - nominal).abs() <= swing + 1e-3,
                "ratio {ratio} is outside {nominal} ± {swing}: {ratios:?}"
            );
        }
        assert!(
            (ratios[0] - ratios[1]).abs() <= 2.0 * swing,
            "the two haloes are not the same multiple: {ratios:?}"
        );
        // And the two balls really are different sizes, or the test proves nothing.
        assert!(
            scale_of(&frame.nodes[1].transform) > scale_of(&frame.nodes[0].transform) * 1.1
        );
    }

    /// The hot centre is a dark-theme thing, and the single `glow_alpha` knob is
    /// what drives both the halo and the core's brightness.
    #[test]
    fn the_core_is_dark_only_and_follows_the_glow_knob() {
        let scene = scene_of(2, &[]);
        let camera = Camera::default();

        let mut light = Style::default();
        light.theme = crate::style::Theme::Light;
        assert!(
            build(&camera, (800.0, 600.0), 1.0, &scene, &light)
                .cores
                .is_empty(),
            "a white core over a white page is invisible"
        );

        let bright = Style::default();
        let dim = Style {
            glow_alpha: bright.glow_alpha * 0.5,
            ..Style::default()
        };
        let lit = build(&camera, (800.0, 600.0), 1.0, &scene, &bright).cores[0].color[3];
        let dull = build(&camera, (800.0, 600.0), 1.0, &scene, &dim).cores[0].color[3];
        assert!(lit > dull * 1.5, "the knob does not reach the core: {lit} vs {dull}");
        assert!(lit > 0.6, "the core is not the brightest thing: {lit}");
    }

    /// The halo breathes — it has to grow back as slowly as it shrank. A clock that
    /// wraps at the nominal period makes every node whose tempo is not exactly one
    /// snap from the smallest breath to the largest, so the sizes are sampled along
    /// the clock and no step may jump.
    #[test]
    fn the_halo_breathes_without_jumping() {
        let camera = Camera::default();
        let mut scene = scene_of(2, &[]);
        let style = Style::default();
        let side = |scene: &crate::scene::Scene| {
            scale_of(&build(&camera, (800.0, 600.0), 1.0, scene, &style).haloes[0].transform)
        };
        let step = 0.02_f32;
        let sizes: Vec<f32> = (0..=600)
            .map(|i| {
                scene.glow_clock = f64::from(i as f32 * step);
                side(&scene)
            })
            .collect();
        let max = sizes.iter().copied().fold(f32::MIN, f32::max);
        let min = sizes.iter().copied().fold(f32::MAX, f32::min);
        assert!(max / min > 1.2, "the halo barely breathes: {min}..{max}");
        for pair in sizes.windows(2) {
            assert!(
                (pair[1] - pair[0]).abs() < max * 0.05,
                "the breath jumps: {} -> {}",
                pair[0],
                pair[1]
            );
        }
    }

    /// The same halo marks what a focus directive names: two ways of saying
    /// "look here" must not look like two different things.
    #[test]
    fn a_focus_directive_haloes_what_it_names() {
        let mut scene = scene_of(3, &[]);
        scene.focus = Some(crate::scene::Focus {
            ids: vec!["n2".to_string()],
            ..Default::default()
        });
        let frame = build(&Camera::default(), (800.0, 600.0), 1.0, &scene, &Style::default());
        assert_eq!(frame.domes.len(), 1);
        assert_eq!(
            frame.domes[0].transform[12],
            scene.bodies[2].pos.x,
            "the shield sits on the focused node"
        );
    }

    /// A focus directive names what to look at; it does not take anything away from
    /// the rest of the graph. What it can do instead — when `highlight_reach` is on
    /// — is light up the paths that lead out of it.
    #[test]
    fn focus_does_not_dim_and_reach_lights_the_paths() {
        let mut scene = scene_of(3, &[("n0", "n1"), ("n1", "n2")]);
        let camera = Camera::default();
        let plain = build(&camera, (800.0, 600.0), 1.0, &scene, &Style::default());

        scene.focus = Some(crate::scene::Focus {
            ids: vec!["n0".to_string()],
            ..Default::default()
        });
        let mut style = Style::default();
        style.highlight_reach = false;
        let dim_off = build(&camera, (800.0, 600.0), 1.0, &scene, &style);
        // Node 0 is the highlighted one and legitimately brighter; the rest must
        // be untouched.
        for index in 1..3 {
            assert_eq!(
                plain.nodes[index].color, dim_off.nodes[index].color,
                "focus changed an unrelated node's colour at {index}"
            );
            assert_eq!(
                plain.haloes[index].color, dim_off.haloes[index].color,
                "focus changed an unrelated node's light at {index}"
            );
        }

        // Reach on: n1 is one edge from the highlight and gets brighter, n2 is two
        // edges away and stays where it was.
        style.highlight_reach = true;
        style.reach_hops = 1;
        let reach = build(&camera, (800.0, 600.0), 1.0, &scene, &style);
        assert!(
            reach.haloes[1].color[3] > dim_off.haloes[1].color[3] * 1.2,
            "the neighbour on the path is not lit up: {} vs {}",
            reach.haloes[1].color[3],
            dim_off.haloes[1].color[3]
        );
        assert!((reach.haloes[2].color[3] - dim_off.haloes[2].color[3]).abs() < 1e-3);

        // The path's own link is drawn heavier; the link beyond it is not.
        let radius = |transform: &[f32; 16]| {
            (transform[4] * transform[4] + transform[5] * transform[5]
                + transform[6] * transform[6])
                .sqrt()
        };
        let on = radius(&reach.links[0].transform);
        let off = radius(&dim_off.links[0].transform);
        assert!(on > off * 1.2, "the path link is not heavier: {on} vs {off}");
        let beyond_on = radius(&reach.links[reach.links.len() - 1].transform);
        let beyond_off = radius(&dim_off.links[dim_off.links.len() - 1].transform);
        assert!((beyond_on - beyond_off).abs() < 1e-3);
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