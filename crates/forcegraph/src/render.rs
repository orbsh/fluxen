//! WebGL2 drawing: instanced spheres for nodes, screen-space quads for links.
//!
//! Deliberately small. There is no scene graph, no material system and no
//! renderer abstraction — the graph is two draw calls, and the point of owning
//! this code is that a custom behaviour (dimming what is not focused, a
//! different node shape for a kind) is a shader edit away instead of a fight
//! with someone else's API.
//!
//! Two sizing contracts:
//! - node radii arrive as **CSS pixels** and become world units through the
//!   camera's own `px_per_unit`, so a sphere keeps its on-screen size at any
//!   zoom;
//! - links are **screen-space quads** (two triangles each) whose width is also
//!   CSS pixels. `gl.LINES` was the first attempt and it is not good enough:
//!   WebGL clamps line width to 1 device pixel on every real driver, so on a
//!   HiDPI screen an edge is a hairline you cannot see.
//!
//! The GL viewport is set from the drawing buffer (CSS size × device pixel
//! ratio) while all the maths runs in CSS pixels: mixing the two is how a HiDPI
//! screen ends up with the graph painted into one quarter of the canvas.

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram, WebGlShader, WebGlUniformLocation,
};

use crate::camera::Camera;
use crate::scene::Scene;
use crate::style::Style;

const NODE_VS: &str = r#"#version 300 es
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec3 a_offset;
layout(location = 3) in float a_radius;
layout(location = 4) in vec3 a_color;
uniform mat4 u_view_proj;
out vec3 v_normal;
out vec3 v_color;
void main() {
    vec3 world_pos = a_pos * a_radius + a_offset;
    gl_Position = u_view_proj * vec4(world_pos, 1.0);
    v_normal = normalize(world_pos - a_offset);
    v_color = a_color;
}"#;

const NODE_FS: &str = r#"#version 300 es
precision mediump float;
in vec3 v_normal;
in vec3 v_color;
out vec4 out_color;
void main() {
    vec3 n = normalize(v_normal);
    vec3 light = normalize(vec3(0.35, 0.75, 0.55));
    float diffuse = max(dot(n, light), 0.0);
    float rim = pow(1.0 - clamp(n.z, 0.0, 1.0), 2.0) * 0.22;
    out_color = vec4(v_color * (0.42 + 0.58 * diffuse) + rim, 1.0);
}"#;

const LINK_VS: &str = r#"#version 300 es
layout(location = 0) in vec3 a_ndc;   // x/y already in clip space, z = 0..1
layout(location = 1) in vec3 a_color;
out vec3 v_color;
void main() {
    gl_Position = vec4(a_ndc.xy, a_ndc.z, 1.0);
    v_color = a_color;
}"#;

const LINK_FS: &str = r#"#version 300 es
precision mediump float;
in vec3 v_color;
uniform float u_alpha;
out vec4 out_color;
void main() {
    out_color = vec4(v_color, u_alpha);
}"#;

pub struct Renderer {
    gl: Gl,
    node_program: WebGlProgram,
    link_program: WebGlProgram,
    sphere_vertices: i32,
    /// Node VAO: unit-sphere attribs (0,1) plus the instanced streams (2-4).
    node_vao: web_sys::WebGlVertexArrayObject,
    instance_positions: WebGlBuffer,
    instance_colors: WebGlBuffer,
    link_vao: web_sys::WebGlVertexArrayObject,
    link_buffer: WebGlBuffer,
    node_view_proj: Option<WebGlUniformLocation>,
    link_alpha: Option<WebGlUniformLocation>,
    positions: Vec<f32>,
    colors: Vec<f32>,
    link_vertices: Vec<f32>,

}

impl Renderer {
    pub fn new(gl: Gl) -> Option<Self> {
        let node_program = program(&gl, NODE_VS, NODE_FS)?;
        let link_program = program(&gl, LINK_VS, LINK_FS)?;

        let (sphere_vertices, sphere_data) = sphere_mesh(10, 16);
        let node_vao = gl.create_vertex_array()?;
        gl.bind_vertex_array(Some(&node_vao));
        let sphere_buffer = gl.create_buffer()?;
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&sphere_buffer));
        gl.buffer_data_with_array_buffer_view(
            Gl::ARRAY_BUFFER,
            &js_sys::Float32Array::from(&sphere_data[..]),
            Gl::STATIC_DRAW,
        );
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(0, 3, Gl::FLOAT, false, 24, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_with_i32(1, 3, Gl::FLOAT, false, 24, 12);

        // Instance streams: offset+radius (stride 16) and colour (stride 12).
        let instance_positions = gl.create_buffer()?;
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&instance_positions));
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_with_i32(2, 3, Gl::FLOAT, false, 16, 0);
        gl.vertex_attrib_divisor(2, 1);
        gl.enable_vertex_attrib_array(3);
        gl.vertex_attrib_pointer_with_i32(3, 1, Gl::FLOAT, false, 16, 12);
        gl.vertex_attrib_divisor(3, 1);

        let instance_colors = gl.create_buffer()?;
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&instance_colors));
        gl.enable_vertex_attrib_array(4);
        gl.vertex_attrib_pointer_with_i32(4, 3, Gl::FLOAT, false, 12, 0);
        gl.vertex_attrib_divisor(4, 1);

        let link_vao = gl.create_vertex_array()?;
        gl.bind_vertex_array(Some(&link_vao));
        let link_buffer = gl.create_buffer()?;
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&link_buffer));
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(0, 3, Gl::FLOAT, false, 24, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_with_i32(1, 3, Gl::FLOAT, false, 24, 12);
        gl.bind_vertex_array(None);

        Some(Self {
            node_view_proj: gl.get_uniform_location(&node_program, "u_view_proj"),
            link_alpha: gl.get_uniform_location(&link_program, "u_alpha"),
            gl,
            node_program,
            link_program,
            sphere_vertices,
            node_vao,
            instance_positions,
            instance_colors,
            link_vao,
            link_buffer,
            positions: Vec::new(),
            colors: Vec::new(),
            link_vertices: Vec::new(),

        })
    }

    /// `viewport_css` is CSS pixels — the same space the camera and the pointer
    /// use.
    pub fn draw(
        &mut self,
        camera: &Camera,
        viewport_css: (f32, f32),
        scene: &Scene,
        style: &Style,
    ) {
        let gl = self.gl.clone();
        gl.viewport(0, 0, gl.drawing_buffer_width(), gl.drawing_buffer_height());
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(Gl::COLOR_BUFFER_BIT | Gl::DEPTH_BUFFER_BIT);
        gl.enable(Gl::BLEND);
        // Separate alpha: the canvas is transparent, so blending the alpha
        // channel with SRC_ALPHA would square it (0.35 → 0.12) and a link drawn
        // at 35% ends up 98% washed out once the page composites under it. With
        // ONE/ONE_MINUS_SRC_ALPHA the framebuffer alpha stays 0.35 and the line
        // looks like a 35% stroke on the page.
        gl.blend_func_separate(
            Gl::SRC_ALPHA,
            Gl::ONE_MINUS_SRC_ALPHA,
            Gl::ONE,
            Gl::ONE_MINUS_SRC_ALPHA,
        );

        let view_proj = camera.view_proj(viewport_css);
        let focused = scene.focus.is_some() && !scene.focus_points().is_empty();

        // ---- links -----------------------------------------------------------
        self.build_links(camera, viewport_css, scene, style, focused);
        if !self.link_vertices.is_empty() {
            gl.use_program(Some(&self.link_program));
            gl.disable(Gl::DEPTH_TEST);
            gl.bind_vertex_array(Some(&self.link_vao));
            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.link_buffer));
            gl.buffer_data_with_array_buffer_view(
                Gl::ARRAY_BUFFER,
                &js_sys::Float32Array::from(&self.link_vertices[..]),
                Gl::DYNAMIC_DRAW,
            );
            let alpha = style.link_opacity.clamp(0.05, 1.0);
            gl.uniform1f(self.link_alpha.as_ref(), alpha);
            gl.draw_arrays(Gl::TRIANGLES, 0, (self.link_vertices.len() / 6) as i32);
        }

        // ---- nodes -----------------------------------------------------------
        gl.enable(Gl::DEPTH_TEST);
        gl.depth_func(Gl::LEQUAL);
        self.positions.clear();
        self.colors.clear();
        let mut visible = 0_i32;
        let max_weight = scene.max_weight();
        for (index, body) in scene.bodies.iter().enumerate() {
            let Some(screen) = camera.project(viewport_css, body.pos) else {
                continue;
            };
            let radius_px = style.radius_px(body.weight, max_weight);
            let radius_world = radius_px / screen.px_per_unit.max(1e-6);
            self.positions.extend_from_slice(&[
                body.pos.x,
                body.pos.y,
                body.pos.z,
                radius_world,
            ]);
            let mut color = node_color(scene, style, index);
            if focused && !scene.is_focused(index) {
                color = [color[0] * 0.35, color[1] * 0.35, color[2] * 0.35];
            }
            self.colors.extend_from_slice(&color);
            visible += 1;
        }
        if visible > 0 {
            gl.use_program(Some(&self.node_program));
            gl.bind_vertex_array(Some(&self.node_vao));
            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.instance_positions));
            gl.buffer_data_with_array_buffer_view(
                Gl::ARRAY_BUFFER,
                &js_sys::Float32Array::from(&self.positions[..]),
                Gl::DYNAMIC_DRAW,
            );
            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.instance_colors));
            gl.buffer_data_with_array_buffer_view(
                Gl::ARRAY_BUFFER,
                &js_sys::Float32Array::from(&self.colors[..]),
                Gl::DYNAMIC_DRAW,
            );
            gl.uniform_matrix4fv_with_f32_array(self.node_view_proj.as_ref(), false, &view_proj);
            gl.draw_arrays_instanced(Gl::TRIANGLES, 0, self.sphere_vertices, visible);
        }
        gl.bind_vertex_array(None);
    }

    /// Two triangles per link, expanded perpendicular to the projected segment
    /// in CSS pixels, then converted to clip space.
    fn build_links(
        &mut self,
        camera: &Camera,
        viewport: (f32, f32),
        scene: &Scene,
        style: &Style,
        focused: bool,
    ) {
        self.link_vertices.clear();
        let half = (style.link_width_px * 0.5).max(0.4);
        let to_clip = |x: f32, y: f32| -> [f32; 3] {
            [
                2.0 * x / viewport.0.max(1.0) - 1.0,
                1.0 - 2.0 * y / viewport.1.max(1.0),
                0.0,
            ]
        };
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
                continue;
            }
            let (nx, ny) = (-dy / len * half, dx / len * half);
            let mut color = node_color(scene, style, link.a);
            if focused && !(scene.is_focused(link.a) && scene.is_focused(link.b)) {
                color = [color[0] * 0.3, color[1] * 0.3, color[2] * 0.3];
            }
            let p1 = to_clip(sa.x + nx, sa.y + ny);
            let p2 = to_clip(sa.x - nx, sa.y - ny);
            let p3 = to_clip(sb.x + nx, sb.y + ny);
            let p4 = to_clip(sb.x - nx, sb.y - ny);
            for corner in [p1, p2, p3, p2, p3, p4] {
                self.link_vertices.extend_from_slice(&corner);
                self.link_vertices.extend_from_slice(&color);
            }
        }
    }
}

fn node_color(scene: &Scene, style: &Style, index: usize) -> [f32; 3] {
    scene
        .nodes
        .get(index)
        .and_then(|n| n.color.as_deref())
        .and_then(crate::style::parse_hex)
        .unwrap_or_else(|| {
            scene
                .nodes
                .get(index)
                .map(|n| style.color_for(n.kind.as_deref().unwrap_or("")))
                .unwrap_or([0.3, 0.3, 0.3])
        })
}

fn program(gl: &Gl, vertex_src: &str, fragment_src: &str) -> Option<WebGlProgram> {
    let program = gl.create_program()?;
    let vs = shader(gl, Gl::VERTEX_SHADER, vertex_src)?;
    let fs = shader(gl, Gl::FRAGMENT_SHADER, fragment_src)?;
    gl.attach_shader(&program, &vs);
    gl.attach_shader(&program, &fs);
    gl.link_program(&program);
    let ok = gl
        .get_program_parameter(&program, Gl::LINK_STATUS)
        .as_bool()
        .unwrap_or(false);
    if !ok {
        let log = gl
            .get_program_info_log(&program)
            .unwrap_or_else(|| "link failed".to_string());
        web_sys::console::warn_1(&JsValue::from_str(&format!("forcegraph: {log}")));
        return None;
    }
    Some(program)
}

fn shader(gl: &Gl, kind: u32, source: &str) -> Option<WebGlShader> {
    let shader = gl.create_shader(kind)?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    let ok = gl
        .get_shader_parameter(&shader, Gl::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false);
    if !ok {
        let log = gl
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| "compile failed".to_string());
        web_sys::console::warn_1(&JsValue::from_str(&format!("forcegraph: {log}")));
        return None;
    }
    Some(shader)
}

/// Unit sphere as a triangle list: `(vertex_count, interleaved pos+normal)`.
fn sphere_mesh(rings: usize, segments: usize) -> (i32, Vec<f32>) {
    let mut data = Vec::new();
    let mut push = |p: [f32; 3]| {
        data.extend_from_slice(&p);
        data.extend_from_slice(&p);
    };
    for ring in 0..rings {
        let v0 = ring as f32 / rings as f32;
        let v1 = (ring + 1) as f32 / rings as f32;
        let (t0, t1) = (v0 * std::f32::consts::PI, v1 * std::f32::consts::PI);
        for segment in 0..segments {
            let u0 = segment as f32 / segments as f32 * std::f32::consts::TAU;
            let u1 = (segment + 1) as f32 / segments as f32 * std::f32::consts::TAU;
            let a = unit(t0, u0);
            let b = unit(t0, u1);
            let c = unit(t1, u0);
            let d = unit(t1, u1);
            for point in [a, c, b, b, c, d] {
                push(point);
            }
        }
    }
    ((data.len() / 6) as i32, data)
}

fn unit(theta: f32, phi: f32) -> [f32; 3] {
    let (st, ct) = theta.sin_cos();
    [st * phi.cos(), ct, st * phi.sin()]
}

/// Grab a WebGL2 context from a canvas, checking the JS type so a missing
/// context is a value rather than an exception.
pub fn context(canvas: &web_sys::HtmlCanvasElement) -> Option<Gl> {
    canvas
        .get_context("webgl2")
        .ok()
        .flatten()
        .and_then(|obj| obj.dyn_into::<Gl>().ok())
}