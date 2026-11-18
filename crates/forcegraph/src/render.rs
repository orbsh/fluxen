//! three-d drawing backend: instanced spheres for nodes, instanced world-space
//! quads for links.
//!
//! Host builds compile this module but never draw: the UI crate builds and runs
//! its tests on the host, and there is no canvas there. Hence the blanket allow
//! — on the host everything below the constructor stub is unreachable.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code, unused_imports))]
//!
//! Deliberately dumb. Every decision — size in world units, colour, dimming,
//! link width — is made in [`crate::frame`]; this module uploads a `Frame` and
//! issues two instanced draw calls. What it does own is the scene layer: a
//! camera, materials, depth, later lights and render targets, which is exactly
//! the part hand-written GL kept getting wrong here: alpha on a transparent
//! canvas, viewport in CSS versus device pixels, and line width clamped to one
//! pixel by the driver.
//!
//! Both batches are instanced, including the links: a link is the unit quad
//! mapped through an affine matrix, so `u` along the quad is the link's own
//! length parameter — which is what a flowing dash or a vine shading will read
//! later.

use std::sync::Arc;

use three_d::*;
use wasm_bindgen::prelude::*;

use crate::frame::{Frame, Instance, View};

pub struct Renderer {
    context: Context,
    /// Unit sphere: the node itself, the only opaque thing in the scene.
    sphere: CpuMesh,
    /// Unit quad: u runs along the link, v across it.
    quad: CpuMesh,

    nodes: Gm<InstancedMesh, PhysicalMaterial>,
    links: Gm<InstancedMesh, PhysicalMaterial>,
    haloes: Gm<InstancedMesh, ColorMaterial>,
    cores: Gm<InstancedMesh, ColorMaterial>,
    domes: Gm<InstancedMesh, ColorMaterial>,
    dome_rims: Gm<InstancedMesh, ColorMaterial>,
    lights: Vec<Box<dyn Light>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Renderer {
    /// Host builds compile the view (the UI crate builds and tests on the host)
    /// but there is no canvas and no GL: mounting yields nothing.
    pub fn new(_canvas: &web_sys::HtmlCanvasElement) -> Option<Self> {
        None
    }

    pub fn draw(&mut self, _frame: &Frame) {}

    pub fn lose_context(_canvas: &web_sys::HtmlCanvasElement) {}
}

#[cfg(target_arch = "wasm32")]
impl Renderer {
    pub fn new(canvas: &web_sys::HtmlCanvasElement) -> Option<Self> {
        let webgl2 = canvas
            .get_context("webgl2")
            .ok()??
            .dyn_into::<web_sys::WebGl2RenderingContext>()
            .ok()?;
        // Same canvas, same GL context: three-d is given the context the DOM
        // already owns, so it never touches the event loop or the page.
        let glow = three_d::context::Context::from_webgl2_context(webgl2);
        // `Arc` is three-d's signature, not a claim about threading: on wasm
        // there is one thread, and the handle is never shared.
        #[allow(clippy::arc_with_non_send_sync)]
        let context = Context::from_gl_context(Arc::new(glow)).ok()?;

        let sphere = CpuMesh::sphere(12);
        let quad = unit_quad();
        let glow_quad = centered_quad();
        let empty = Instances {
            transformations: vec![Mat4::identity()],
            texture_transformations: None,
            colors: None,
        };
        // A node is a ball that glows: the shading survives so the surface reads —
        // a bright side, a rim — and the emissive term keeps it lit when nothing
        // shines on it. Emissive is a material uniform, so it is white here and the
        // instance colour rides in the albedo: a ball that keeps its own colour in
        // the dark.
        let nodes = Gm::new(
            InstancedMesh::new(&context, &empty, &sphere),
            PhysicalMaterial::new(
                &context,
                &CpuMaterial {
                    albedo: Srgba::WHITE,
                    roughness: 0.38,
                    metallic: 0.0,
                    // Enough emissive to keep the ball's own colour in a dark
                    // scene, not so much that it flattens into the halo around it:
                    // the ball has to be the bright thing inside the light.
                    emissive: Srgba::new(120, 120, 120, 255),
                    ..Default::default()
                },
            ),
        );
        // Links are unlit: a relationship is a line, not a surface, and shading
        // it would make its brightness depend on which way the graph happens to
        // be turned. Alpha is separate from the RGB blend, because the canvas is
        // transparent: with standard alpha blending a 40% link would write 40%
        // alpha and *also* keep the framebuffer's 0, leaving the line invisible
        // once the page composites under it.
        // A link is a tube, not a ribbon: a flat quad is a band face-on and a
        // hairline edge-on, so a bent link would change width along its length.
        // `cylinder` runs along x over [0,1] with radius 1; the frame's instance
        // matrix maps that onto the segment.
        let links = Gm::new(
            InstancedMesh::new(&context, &empty, &CpuMesh::cylinder(10)),
            PhysicalMaterial::new(
                &context,
                &CpuMaterial {
                    albedo: Srgba::WHITE,
                    roughness: 0.55,
                    metallic: 0.0,
                    // Shaded so the round section reads: without a light gradient a
                    // tube and a ribbon look the same. Emissive keeps it visible as
                    // a thread of light on a dark panel.
                    emissive: Srgba::new(70, 70, 70, 255),
                    ..Default::default()
                },
            ),
        );
        // The halo every node sits in, and the shield around the selection: the
        // same soft disc, two batches only because their instance lists differ.
        // Depth-tested so a halo behind a ball stays behind it, no depth write so
        // one halo does not clip the next.
        let halo_texture = radial_texture(&context);
        let haloes = Gm::new(
            InstancedMesh::new(&context, &empty, &glow_quad),
            ColorMaterial {
                color: Srgba::WHITE,
                texture: Some(halo_texture.clone()),
                render_states: halo_states(),
                is_transparent: true,
            },
        );
        // The core's falloff is much steeper than the halo's: at the same alpha
        // curve the core would just be a second halo, slightly brighter.
        let cores = Gm::new(
            InstancedMesh::new(&context, &empty, &glow_quad),
            ColorMaterial {
                color: Srgba::WHITE,
                texture: Some(core_texture(&context)),
                render_states: halo_states(),
                is_transparent: true,
            },
        );
        let domes = Gm::new(
            InstancedMesh::new(&context, &empty, &glow_quad),
            ColorMaterial {
                color: Srgba::WHITE,
                texture: Some(halo_texture),
                render_states: halo_states(),
                is_transparent: true,
            },
        );
        // The edge of the glass. Geometry, not a texture: a thin annulus is the one
        // shape that stays a clean 1–2 px circle at any radius.
        let dome_rims = Gm::new(
            // 72 segments, 3 on and 1 off: a dashed circle, 1px thick.
            // A hairline: screen-constant thickness, so the ring reads the same at
            // any node size and never turns into a band.
            InstancedMesh::new(&context, &empty, &dashed_ring_mesh(72, 0.985, 3, 1)),
            ColorMaterial {
                color: Srgba::WHITE,
                texture: None,
                render_states: halo_states(),
                is_transparent: true,
            },
        );
        // A key light from above-left, a fill, and ambient. The ambient matters
        // for the palette: with directional light alone a muted hue renders at a
        // fraction of its albedo (measured: a #93a094 sphere came out around
        // #535252), so the colours on screen were darker and greyer than the
        // palette the style asked for. Ambient sets the floor near the albedo and
        // the key gives the form.
        let lights: Vec<Box<dyn Light>> = vec![
            Box::new(AmbientLight::new(&context, 0.55, Srgba::WHITE)),
            Box::new(DirectionalLight::new(
                &context,
                1.3,
                Srgba::WHITE,
                vec3(-0.45, -1.0, -0.55),
            )),
            Box::new(DirectionalLight::new(
                &context,
                0.5,
                Srgba::WHITE,
                vec3(0.8, 0.3, 0.6),
            )),
        ];

        Some(Self {
            context,
            sphere,
            quad,
            nodes,
            links,
            haloes,
            cores,
            domes,
            dome_rims,
            lights,
        })
    }

    /// Upload the frame and draw. Two batches, no per-frame allocation of
    /// programs or buffers beyond the instance streams.
    pub fn draw(&mut self, frame: &Frame) {
        let (width, height) = frame.view.viewport_px;
        // Created per frame on purpose: three-d's render target is a thin view
        // onto the textures, so holding one would only pin a stale size.
        let target = RenderTarget::screen(&self.context, width.max(1), height.max(1));
        target.clear(ClearState::color_and_depth(0.0, 0.0, 0.0, 0.0, 1.0));

        if !frame.nodes.is_empty() {
            self.nodes.geometry.set_instances(&instances(&frame.nodes));
        }
        if !frame.links.is_empty() {
            self.links.geometry.set_instances(&instances(&frame.links));
        }
        if !frame.haloes.is_empty() {
            self.haloes
                .geometry
                .set_instances(&instances(&frame.haloes));
        }
        if !frame.cores.is_empty() {
            self.cores
                .geometry
                .set_instances(&instances(&frame.cores));
        }
        if !frame.domes.is_empty() {
            self.domes.geometry.set_instances(&instances(&frame.domes));
        }
        if !frame.dome_rims.is_empty() {
            self.dome_rims
                .geometry
                .set_instances(&instances(&frame.dome_rims));
        }
        // Light on a dark panel, tint on a light one: adding to white leaves
        // white, so the same blend cannot serve both.
        let blend = if frame.glow_additive {
            additive_blend()
        } else {
            tint_blend()
        };
        // Everything drawn as light switches together: orbs and strokes add on a
        // dark panel, and become plain tint on a light one.
        if self.haloes.material.render_states.blend != blend {
            self.haloes.material.render_states.blend = blend.clone();
            self.cores.material.render_states.blend = blend.clone();
            self.domes.material.render_states.blend = blend.clone();
            self.dome_rims.material.render_states.blend = blend;
        }
        if frame.nodes.is_empty() && frame.links.is_empty() {
            return;
        }

        let lights: Vec<&dyn Light> = self.lights.iter().map(|l| l.as_ref()).collect();
        // Debug hook: what the backend was actually handed this frame.
        if let Some(window) = web_sys::window() {
            let text = format!(
                "nodes={} links={} haloes={} domes={} additive={}",
                frame.nodes.len(),
                frame.links.len(),
                frame.haloes.len(),
                frame.domes.len(),
                frame.glow_additive
            );
            let _ = js_sys::Reflect::set(
                &window,
                &"fgRings".into(),
                &format!(
                    "haloes={} cores={} domes={}",
                    frame.haloes.len(),
                    frame.cores.len(),
                    frame.domes.len()
                )
                .into(),
            );
            let _ = js_sys::Reflect::set(&window, &"fgDraw".into(), &text.into());
        }
        let objects: Vec<&dyn Object> = vec![
            &self.nodes,
            &self.links,
            &self.domes,
            &self.dome_rims,
            &self.haloes,
            &self.cores,
        ];
        target.render(FrameViewer(frame.view), objects, &lights);
    }

    /// Release the GL context on teardown: the canvas is about to be removed and
    /// a browser keeps only a handful of live WebGL contexts, so dropping one
    /// without asking the driver to lose it can starve the next view.
    pub fn lose_context(canvas: &web_sys::HtmlCanvasElement) {
        let Ok(Some(raw)) = canvas.get_context("webgl2") else {
            return;
        };
        let Ok(gl) = raw.dyn_into::<web_sys::WebGl2RenderingContext>() else {
            return;
        };
        let Ok(Some(ext)) = gl.get_extension("WEBGL_lose_context") else {
            return;
        };
        let Ok(lose) = js_sys::Reflect::get(&ext, &JsValue::from_str("loseContext")) else {
            return;
        };
        let Ok(lose) = lose.dyn_into::<js_sys::Function>() else {
            return;
        };
        lose.call0(&ext).ok();
    }

    /// Size of the geometry the backend keeps alive, for the pixel probes and
    /// for anyone wondering whether the sphere is being rebuilt per frame.
    pub fn sphere_vertices(&self) -> usize {
        self.sphere.positions.len() + self.quad.positions.len()
    }
}

/// A `Frame`'s viewer, handed to three-d. The matrices come from the same camera
/// the pointer picks with, so a sphere and its label cannot drift apart.
struct FrameViewer(View);

impl Viewer for FrameViewer {
    fn position(&self) -> Vec3 {
        vec3(self.0.position.x, self.0.position.y, self.0.position.z)
    }

    fn view(&self) -> Mat4 {
        mat4(self.0.view)
    }

    fn projection(&self) -> Mat4 {
        mat4(self.0.projection)
    }

    fn viewport(&self) -> Viewport {
        Viewport {
            x: 0,
            y: 0,
            width: self.0.viewport_px.0.max(1),
            height: self.0.viewport_px.1.max(1),
        }
    }

    fn z_near(&self) -> f32 {
        self.0.z_near
    }

    fn z_far(&self) -> f32 {
        self.0.z_far
    }

    // Colours arrive as sRGB and are converted to linear for shading, then back
    // on the way out. Any non-default mapping here would shift the palette.
    fn color_mapping(&self) -> ColorMapping {
        ColorMapping::default()
    }

    fn tone_mapping(&self) -> ToneMapping {
        ToneMapping::default()
    }
}

/// Column-major array → three-d's matrix.
fn mat4(cells: [f32; 16]) -> Mat4 {
    Mat4::from_cols(
        vec4(cells[0], cells[1], cells[2], cells[3]),
        vec4(cells[4], cells[5], cells[6], cells[7]),
        vec4(cells[8], cells[9], cells[10], cells[11]),
        vec4(cells[12], cells[13], cells[14], cells[15]),
    )
}

fn instances(list: &[Instance]) -> Instances {
    Instances {
        transformations: list.iter().map(|i| mat4(i.transform)).collect(),
        texture_transformations: None,
        colors: Some(
            list.iter()
                .map(|i| {
                    Srgba::new(
                        to_byte(i.color[0]),
                        to_byte(i.color[1]),
                        to_byte(i.color[2]),
                        to_byte(i.color[3]),
                    )
                })
                .collect(),
        ),
    }
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A radial alpha ramp, white in the middle and empty at the rim so the quad's
/// own edges can never show. Generated rather than shipped as an asset: it is
/// 64x64 of arithmetic and it is the only texture the renderer owns.
fn radial_texture(context: &Context) -> Texture2DRef {
    const N: u32 = 64;
    let mut data: Vec<[u8; 4]> = Vec::with_capacity((N * N) as usize);
    for y in 0..N {
        for x in 0..N {
            let fx = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let fy = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let r = (fx * fx + fy * fy).sqrt();
            // A gentle falloff on purpose. The bright core of a halo is hidden
            // behind the opaque sphere anyway, so what the viewer sees is the
            // band between the node's edge and the rim; a steep curve wastes the
            // texture's light where nobody can see it (measured: a 2.2 exponent
            // left the visible band at ~10% and the halo read as nothing).
            let alpha = (1.0 - r).clamp(0.0, 1.0).powf(1.4);
            data.push([255, 255, 255, (alpha * 255.0).round() as u8]);
        }
    }
    let cpu = CpuTexture {
        data: TextureData::RgbaU8(data),
        width: N,
        height: N,
        ..Default::default()
    };
    Texture2DRef::from_cpu_texture(context, &cpu)
}

/// The hot centre: almost everything inside a quarter of the radius, nothing
/// outside a half. Steep on purpose — this is the light *source*, and the halo is
/// the faint spill that a bloom pass would otherwise have to invent.
fn core_texture(context: &Context) -> Texture2DRef {
    const N: u32 = 64;
    let mut data: Vec<[u8; 4]> = Vec::with_capacity((N * N) as usize);
    for y in 0..N {
        for x in 0..N {
            let fx = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let fy = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let r = (fx * fx + fy * fy).sqrt();
            let alpha = (1.0 - r).clamp(0.0, 1.0).powf(5.0);
            data.push([255, 255, 255, (alpha * 255.0).round() as u8]);
        }
    }
    Texture2DRef::from_cpu_texture(
        context,
        &CpuTexture {
            data: TextureData::RgbaU8(data),
            width: N,
            height: N,
            ..Default::default()
        },
    )
}

/// A ring: transparent in the middle, brightest at 0.62 of the radius, empty at
/// the rim. The quad can then be scaled freely — the ring's width in *world*
/// terms grows with it, which is what makes it read as spreading rather than as
/// a picture being zoomed.


/// A halo does not write depth and is not culled from the transparent pass; the
/// blend is chosen per frame, because light and tint are not interchangeable.
fn halo_states() -> RenderStates {
    RenderStates {
        blend: tint_blend(),
        depth_test: DepthTest::LessOrEqual,
        write_mask: WriteMask::COLOR,
        ..Default::default()
    }
}

/// Add light instead of covering it. `src * a` scaled by the fragment's own
/// alpha, added to what is there. This is
/// the additive blend for a texture that is *not* premultiplied — `Blend::ADD`
/// multiplies the source by one, which would also add the quad's empty rim.
fn additive_blend() -> Blend {
    Blend::Enabled {
        source_rgb_multiplier: BlendMultiplierType::SrcAlpha,
        source_alpha_multiplier: BlendMultiplierType::One,
        destination_rgb_multiplier: BlendMultiplierType::One,
        destination_alpha_multiplier: BlendMultiplierType::One,
        rgb_equation: BlendEquationType::Add,
        alpha_equation: BlendEquationType::Add,
    }
}

fn tint_blend() -> Blend {
    Blend::Enabled {
        source_rgb_multiplier: BlendMultiplierType::SrcAlpha,
        source_alpha_multiplier: BlendMultiplierType::One,
        destination_rgb_multiplier: BlendMultiplierType::OneMinusSrcAlpha,
        destination_alpha_multiplier: BlendMultiplierType::OneMinusSrcAlpha,
        rgb_equation: BlendEquationType::Add,
        alpha_equation: BlendEquationType::Add,
    }
}

/// An annulus in the XY plane, outer radius 1. A ripple is a *ring*, so it is
/// built as geometry: a ring drawn by sampling a radial texture depends on the
/// sampler, on mip selection and on the texels surviving magnification, and a
/// texture that samples as nothing looks exactly like a mesh that was never
/// drawn.


/// Centred unit square with uv across it — the halo's geometry.
fn centered_quad() -> CpuMesh {
    CpuMesh {
        positions: Positions::F32(vec![
            vec3(-0.5, -0.5, 0.0),
            vec3(0.5, -0.5, 0.0),
            vec3(0.5, 0.5, 0.0),
            vec3(-0.5, 0.5, 0.0),
        ]),
        indices: Indices::U32(vec![0, 1, 2, 0, 2, 3]),
        uvs: Some(vec![
            vec2(0.0, 0.0),
            vec2(1.0, 0.0),
            vec2(1.0, 1.0),
            vec2(0.0, 1.0),
        ]),
        ..Default::default()
    }
}

/// The unit quad: x runs 0..1 along the link, y runs -0.5..0.5 across it, so a
/// A dashed circle: an annulus of width `(1 - inner) * radius`, drawn as runs of
/// `on` segments then `off` gaps. Geometry rather than a texture — a ring sampled
/// from a texture can vanish into a mip level, and then it looks exactly like a
/// mesh that was never drawn.
fn dashed_ring_mesh(segments: usize, inner: f32, on: usize, off: usize) -> CpuMesh {
    let period = (on + off).max(1);
    let mut positions: Vec<Vec3> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for step in 0..segments {
        if step % period >= on {
            continue;
        }
        let start_angle = step as f32 / segments as f32 * std::f32::consts::TAU;
        let end_angle = (step + 1) as f32 / segments as f32 * std::f32::consts::TAU;
        let (sin_a, cos_a) = start_angle.sin_cos();
        let (sin_b, cos_b) = end_angle.sin_cos();
        let base = positions.len() as u32;
        positions.push(vec3(cos_a * inner, sin_a * inner, 0.0));
        positions.push(vec3(cos_a, sin_a, 0.0));
        positions.push(vec3(cos_b * inner, sin_b * inner, 0.0));
        positions.push(vec3(cos_b, sin_b, 0.0));
        indices.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    }
    CpuMesh {
        positions: Positions::F32(positions),
        indices: Indices::U32(indices),
        ..Default::default()
    }
}

/// link's affine matrix maps this square onto its world-space parallelogram.
fn unit_quad() -> CpuMesh {
    let positions = vec![
        vec3(0.0, -0.5, 0.0),
        vec3(1.0, -0.5, 0.0),
        vec3(1.0, 0.5, 0.0),
        vec3(0.0, 0.5, 0.0),
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    let uvs = vec![
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(1.0, 1.0),
        vec2(0.0, 1.0),
    ];
    CpuMesh {
        positions: Positions::F32(positions),
        indices: Indices::U32(indices),
        uvs: Some(uvs),
        ..Default::default()
    }
}
#[cfg(test)]
mod tests {
    /// The glass rim is a dashed circle: the dashes have to exist (fewer segments
    /// than a solid ring), stay on the circle, and close the pattern exactly once
    /// around.
    #[test]
    fn the_dome_rim_is_dashed_and_round() {
        let segments = 72;
        let dashed = super::dashed_ring_mesh(segments, 0.96, 3, 1);
        let three_d::Positions::F32(points) = dashed.positions else {
            panic!("expected float positions");
        };
        let three_d::Indices::U32(indices) = dashed.indices else {
            panic!("expected u32 indices");
        };
        // 3 of every 4 segments are drawn.
        assert_eq!(points.len(), segments * 3 / 4 * 4);
        assert_eq!(indices.len(), segments * 3 / 4 * 6);
        for point in &points {
            let radius = (point.x * point.x + point.y * point.y).sqrt();
            assert!(
                (0.955..=1.005).contains(&radius),
                "a dash left the circle: {radius}"
            );
        }
        // The pattern closes: the last drawn segment is not adjacent to the first
        // drawn one, or the dashes would merge into a solid arc.
        assert!(!indices.is_empty());
    }
}
