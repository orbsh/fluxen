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
    /// Unit sphere, shared by every node instance.
    sphere: CpuMesh,
    /// Unit quad: u runs along the link, v across it.
    quad: CpuMesh,
    nodes: Gm<InstancedMesh, PhysicalMaterial>,
    links: Gm<InstancedMesh, ColorMaterial>,
    lights: Vec<DirectionalLight>,
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
        let empty = Instances {
            transformations: vec![Mat4::identity()],
            texture_transformations: None,
            colors: None,
        };
        let nodes = Gm::new(
            InstancedMesh::new(&context, &empty, &sphere),
            PhysicalMaterial::new(
                &context,
                &CpuMaterial {
                    albedo: Srgba::WHITE,
                    roughness: 0.42,
                    metallic: 0.0,
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
        let links = Gm::new(
            InstancedMesh::new(&context, &empty, &quad),
            ColorMaterial {
                color: Srgba::WHITE,
                texture: None,
                render_states: RenderStates {
                    blend: Blend::Enabled {
                        source_rgb_multiplier: BlendMultiplierType::SrcAlpha,
                        source_alpha_multiplier: BlendMultiplierType::One,
                        destination_rgb_multiplier: BlendMultiplierType::OneMinusSrcAlpha,
                        destination_alpha_multiplier: BlendMultiplierType::OneMinusSrcAlpha,
                        rgb_equation: BlendEquationType::Add,
                        alpha_equation: BlendEquationType::Add,
                    },
                    depth_test: DepthTest::LessOrEqual,
                    ..Default::default()
                },
                is_transparent: true,
            },
        );
        // Two lights is enough for a sphere to read as a sphere: a key light
        // from above-left and a weak fill so the dark side is not black.
        let lights = vec![
            DirectionalLight::new(&context, 1.0, Srgba::WHITE, vec3(-0.45, -1.0, -0.55)),
            DirectionalLight::new(&context, 0.35, Srgba::WHITE, vec3(0.8, 0.3, 0.6)),
        ];

        Some(Self {
            context,
            sphere,
            quad,
            nodes,
            links,
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
        if frame.nodes.is_empty() && frame.links.is_empty() {
            return;
        }

        let lights: Vec<&dyn Light> = self.lights.iter().map(|l| l as &dyn Light).collect();
        let objects: Vec<&dyn Object> = vec![&self.nodes, &self.links];
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

/// The unit quad: x runs 0..1 along the link, y runs -0.5..0.5 across it, so a
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