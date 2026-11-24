//! 3dbrowser — an example implementation of the ADR 0007 module contract: an on-demand three-d capability module.
//!
//! Build (in this directory):
//! ```sh
//! cargo build --release
//! wasm-bindgen gen --target web -o pkg target/wasm32-unknown-unknown/browser3d.wasm
//! ```
//! Put `pkg/` + this directory's `index.js` on a CDN (or local assets); the Canvas component's
//! `url` points at `index.js`. The host only knows four exports (mount/update/resize/unmount);
//! the data-area semantics (`assets`/`primitives`) belong entirely to this module.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use js_sys::Uint8Array;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use three_d::*;
use wasm_bindgen::prelude::*;

/// Schema of the custom data area (module-private vocabulary; the core does not validate it).
///
/// ```json
/// {
///   "primitives": { "cube": {}, "sphere": {"color": [200,60,60]} },
///   "assets": { "duck": null, "fox": "https://cdn/fox.glb" },
///   "clear": [0.9, 0.92, 0.96, 1.0]
/// }
/// ```
/// null = placeholder (an ADR 0005 pointer can only replace an existing key, so when streaming a URL in,
/// hold a null placeholder first and replace it with the real value frame by frame); the module skips null and loads only non-null.
#[derive(Debug, Default, Deserialize)]
struct Spec {
    #[serde(default)]
    primitives: HashMap<String, PrimSpec>,
    #[serde(default)]
    assets: HashMap<String, Option<String>>,
    /// Clear color `[r, g, b, a]` (0..1). Default = fully transparent (alpha 0), so the canvas shows the host
    /// page background; for a fixed color, give the four-tuple explicitly in the data area.
    #[serde(default)]
    clear: Option<[f32; 4]>,
}

#[derive(Debug, Default, Deserialize)]
struct PrimSpec {
    #[serde(default)]
    color: Option<[u8; 3]>,
}

fn decode(data: &JsValue) -> Result<Spec, JsError> {
    let bytes = Uint8Array::new(data).to_vec();
    let spec: Spec = ciborium::de::from_reader(std::io::Cursor::new(&bytes[..]))
        .map_err(|e| JsError::new(&format!("3dbrowser: bad CBOR payload: {e}")))?;
    Ok(spec)
}

/// Forward panic messages to console.error: by default (panic=abort) wasm hands JS only a bare
/// `unreachable`, with no sign of "which model, why" — this module once hit a
/// shader link failure (glb missing tangent), and this one message is what located it.
fn install_panic_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            web_sys::console::error_1(&format!("3dbrowser panic: {info}").into());
        }));
    });
}

struct State {
    context: Context,
    camera: Camera,
    /// Three-light rig: key light (with shadow-map casting) + fill + rim,
    /// plus a low-intensity ambient as a fallback — a single direct light is the main cause of the original example's "plastic" look.
    ambient: AmbientLight,
    key_light: DirectionalLight,
    fill_light: DirectionalLight,
    rim_light: DirectionalLight,
    /// Built-in geometry (unlit material; rendered in a separate pass from gltf, since the material types differ).
    color_objs: Vec<Gm<Mesh, ColorMaterial>>,
    /// Loaded gltf models (PBR, lit by the lights).
    models: Vec<Model<PhysicalMaterial>>,
    seen: HashSet<String>,
    pending: VecDeque<(String, String)>,
    /// Currently loaded asset URL set (display semantics = one at a time: when the spec's assets set changes,
    /// the models are replaced wholesale rather than stacked up).
    current_assets: Vec<String>,
    /// Clear color (fully transparent by default, see `Spec::clear`).
    clear: [f32; 4],
    dirty: bool,
    size: (u32, u32),
    /// This instance's canvas: the loading overlay hangs on its parent container (see `overlay_build`).
    canvas: web_sys::HtmlCanvasElement,
    /// Loading overlay handle (spins in the container while assets are fetched). Idempotent: built when needed, removed
    /// when the queue is empty; three-d has no built-in progress, so this layer is the "download transition".
    overlay: Option<web_sys::Element>,
}

impl State {
    fn apply(&mut self, spec: Spec) {
        if let Some(c) = spec.clear {
            self.clear = c;
            self.dirty = true;
        }
        let mut next = 0u32;
        for (name, ps) in spec.primitives {
            if self.seen.insert(name.clone()) {
                let mut cpu = match name.as_str() {
                    "sphere" => CpuMesh::sphere(24),
                    "cylinder" => CpuMesh::cylinder(24),
                    _ => CpuMesh::cube(),
                };
                // Grid layout: each added primitive shifts one cell along X
                cpu.transform(Mat4::from_translation(vec3(next as f32 * 2.0 - 2.0, 0.0, 0.0)))
                    .ok();
                let material = ColorMaterial {
                    color: ps
                        .color
                        .map_or(Srgba::new_opaque(90, 140, 200), |c| Srgba::new_opaque(c[0], c[1], c[2])),
                    ..Default::default()
                };
                self.color_objs
                    .push(Gm::new(Mesh::new(&self.context, &cpu), material));
                next += 1;
                self.dirty = true;
            }
        }
        for (key, url) in spec.assets {
            let Some(url) = url else { continue };   // null placeholder = not here yet
            let id = format!("{key}:{url}");
            if self.seen.insert(id.clone()) {
                // Display semantics = one at a time: a change in the asset set means a wholesale replace. Old models
                // and old pending entries are cleared together; only the newest declared asset enters the load queue.
                self.models.clear();
                self.current_assets.clear();
                self.pending.clear();
                self.current_assets.push(id.clone());
                self.pending.push_back((id, url));
            }
        }
    }

    /// Handle one pending asset; the async caller wraps it in spawn_local.
    fn pump(&mut self) -> Option<(String, String)> {
        self.pending.pop_front()
    }

    fn add_model(&mut self, cpu: three_d_asset::Model) {
        // Compute tangents when missing: many glb files carry a normal texture but no tangent
        // attribute, while three-d's PhysicalMaterial, given a normal_texture, declares
        // `in vec3 tang/bitang` in the fragment shader — and the matching vertex shader only emits
        // them when the geometry has tangents; without them it is a shader link failure
        // ("FRAGMENT varying tang does not match any VERTEX varying") followed by a panic.
        // normals + uvs are enough to compute them; geometry missing a prerequisite is left as is (its material has no textures either).
        let mut cpu = cpu;
        for p in cpu.geometries.iter_mut() {
            if let three_d_asset::Geometry::Triangles(mesh) = &mut p.geometry {
                if mesh.tangents.is_none() && mesh.normals.is_some() && mesh.uvs.is_some() {
                    mesh.compute_tangents();
                }
            }
        }
        // Center it and normalize to a ~2-cell radius, so wildly varying external gltf scales don't matter
        let mut model = match Model::<PhysicalMaterial>::new(&self.context, &cpu) {
            Ok(m) => m,
            Err(e) => {
                web_sys::console::warn_1(&format!("3dbrowser: model failed: {e:?}").into());
                return;
            }
        };
        let mut aabb = AxisAlignedBoundingBox::EMPTY;
        for o in model.iter() {
            aabb.expand_with_aabb(o.aabb());
        }
        let c = -aabb.center();
        let sz = aabb.size();
        let diag = (sz.x * sz.x + sz.y * sz.y + sz.z * sz.z).sqrt();
        let s = if diag > 0.0 { 4.0 / diag } else { 1.0 };
        for m in model.iter_mut() {
            // Normalization must be **composed** with the glTF node transform, never overwrite it: three-d hangs each
            // primitive's node transform on the Gm (model.rs does
            // `set_transformation(primitive.transformation)` at construction), so a plain overwrite drops it.
            // The watch was the case in point: case back and hands align to the dial via node rotation (-90°X), and once
            // overwritten they lie on their own local plane, perpendicular to the dial ("the dial faces the wrong way").
            let node = m.transformation();
            m.set_transformation(Mat4::from_translation(c) * Mat4::from_scale(s) * node);
        }
        // Bottom height of the normalized model: min.y after scaling — the camera framing anchor sits on it.
        // No ground plane: the model standing on its own is enough (for a background color use the data area's clear).
        let bottom = (aabb.min().y + c.y) * s;
        // Frame the camera above the bottom-center so the first frame already faces the subject
        let cam_target = vec3(0.0, bottom + 1.0, 0.0);
        let pos = cam_target + vec3(2.8, 1.6, 7.0);
        self.camera.set_view(pos, cam_target, vec3(0.0, 1.0, 0.0));
        // Rebuild the key light's shadow map after every scene content change (models participate in casting)
        self.relight();
        self.models.push(model);
        self.dirty = true;
    }

    /// Rebuild the key light's shadow map from the current scene geometry.
    fn relight(&mut self) {
        let geoms = self
            .models
            .iter()
            .flat_map(|m| m.iter().map(|p| p as &dyn Geometry))
            .collect::<Vec<_>>();
        if !geoms.is_empty() {
            self.key_light.generate_shadow_map(2048, geoms).ok();
        }
        self.dirty = true;
    }

    fn render(&mut self) {
        let (w, h) = self.size;
        if w == 0 || h == 0 {
            return;
        }
        let rt = RenderTarget::screen(&self.context, w, h);
        let [r, g, b, a] = self.clear;
        rt.clear(ClearState::color_and_depth(r, g, b, a, 1.0));
        let lights: [&dyn Light; 4] = [
            &self.ambient,
            &self.key_light,
            &self.fill_light,
            &self.rim_light,
        ];
        rt.render(&self.camera, &self.color_objs, &lights);
        rt.render(
            &self.camera,
            self.models.iter().flat_map(|m| m.into_iter()),
            &lights,
        );
        self.dirty = false;
    }
}

thread_local! {
    static CTXS: RefCell<Vec<Option<Rc<RefCell<State>>>>> = const { RefCell::new(Vec::new()) };
    /// Staging area for models that finished async loading: the callback never touches CTXS (frame()'s borrow_mut
    /// is in progress, and borrowing directly panics on a RefCell conflict); the next frame takes them.
    static LOADED: RefCell<Vec<(u32, String, three_d_asset::Model)>> = const { RefCell::new(Vec::new()) };
}

fn with_ctx<T>(ctx: u32, f: impl FnOnce(&mut State) -> T) -> Result<T, JsError> {
    // Key point: CTXS's borrow_mut covers only the "take the Rc" step, never f's execution —
    // render/GPU calls inside f can re-enter JS (events, mount/frame from rAF),
    // and landing there while CTXS is borrowed is panic_already_borrowed. With a narrow borrow window,
    // concurrency safety rests on State's own RefCell alone, and other ctxs can still come and go during f.
    let rc = CTXS.with(|c| {
        c.borrow_mut()
            .get(ctx as usize)
            .and_then(|slot| slot.as_ref())
            .cloned()
            .ok_or_else(|| JsError::new("3dbrowser: stale ctx"))
    })?;
    let mut s = rc.borrow_mut();
    Ok(f(&mut s))
}

/// Loading overlay: a centered spinner in the host container. Styles ship with the module (it may be loaded from a CDN
/// so it cannot rely on the host page's CSS); the container is made a positioning ancestor so the overlay matches its size.
fn overlay_build(canvas: &web_sys::HtmlCanvasElement) -> Option<web_sys::Element> {
    let doc = web_sys::window()?.document()?;
    let host = canvas.parent_element()?;
    if let Ok(h) = host.clone().dyn_into::<web_sys::HtmlElement>() {
        let style = h.style();
        if style.get_property_value("position").unwrap_or_default().is_empty() {
            let _ = style.set_property("position", "relative");
        }
    }
    let el = doc.create_element("div").ok()?;
    el.set_class_name("browser3d-loading");
    el.set_inner_html(
        "<style>\
         .browser3d-loading{position:absolute;inset:0;display:grid;place-items:center;background:rgb(127 127 127 / 6%)}\
         .browser3d-loading>span{width:2.2rem;height:2.2rem;border-radius:50%;border:3px solid rgb(127 127 127 / 25%);border-top-color:rgb(70 140 240 / 95%);animation:browser3d-spin .9s linear infinite}\
         @keyframes browser3d-spin{to{transform:rotate(1turn)}}\
         </style><span></span>",
    );
    host.append_child(&el).ok()?;
    Some(el)
}

/// Build the overlay when needed (idempotent). Inside the borrow the handle is only cloned; DOM work happens outside
/// it — never hold a thread_local borrow across a callback (Pitfalls).
fn overlay_ensure(ctx: u32) {
    let canvas = with_ctx(ctx, |s| {
        if s.overlay.is_some() {
            None
        } else {
            Some(s.canvas.clone())
        }
    });
    if let Ok(Some(canvas)) = canvas {
        let el = overlay_build(&canvas);
        let _ = with_ctx(ctx, |s| {
            s.overlay = el;
        });
    }
}

/// Remove the overlay once the load queue is empty — no flicker midway when one payload carries several assets.
fn overlay_hide_if_idle(ctx: u32) {
    let el = with_ctx(ctx, |s| {
        if s.pending.is_empty() {
            s.overlay.take()
        } else {
            None
        }
    });
    if let Ok(Some(el)) = el {
        el.remove();
    }
}

/// Contract mount: canvas + custom data area (CBOR) + host channel (unused in this example, kept as a hook).
#[wasm_bindgen]
pub fn mount(canvas: web_sys::HtmlCanvasElement, data: JsValue, _host: JsValue) -> Result<u32, JsError> {
    install_panic_hook();
    let gl = canvas
        .get_context("webgl2")
        .map_err(|_| JsError::new("3dbrowser: webgl2 unavailable"))?
        .ok_or_else(|| JsError::new("3dbrowser: no webgl2 context"))?
        .dyn_into::<web_sys::WebGl2RenderingContext>()
        .map_err(|_| JsError::new("3dbrowser: not webgl2"))?;
    let context = Context::from_gl_context(std::sync::Arc::new(glow::Context::from_webgl2_context(gl)))
        .map_err(|e| JsError::new(&format!("3dbrowser: context: {e:?}")))?;

    let size = (canvas.width(), canvas.height());
    let camera = Camera::new_perspective(
        Viewport::new_at_origo(size.0.max(1), size.1.max(1)),
        vec3(2.5, 2.0, 4.5),
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.05,
        100.0,
    );
    let state = Rc::new(RefCell::new(State {
        // Three-light rig: key from upper front-left (with shadow map later), a weak fill from front-right,
        // rim from rear above for an outline, ambient as a floor against pitch black.
        ambient: AmbientLight::new(&context, 0.35, Srgba::WHITE),
        key_light: DirectionalLight::new(&context, 2.2, Srgba::WHITE, vec3(-0.5, -0.9, -0.4)),
        fill_light: DirectionalLight::new(&context, 0.6, Srgba::new(190, 205, 255, 255), vec3(0.7, -0.2, -0.3)),
        rim_light: DirectionalLight::new(&context, 1.1, Srgba::new(255, 245, 230, 255), vec3(0.1, -0.5, 0.9)),
        context,
        camera,
        color_objs: Vec::new(),
        models: Vec::new(),
        seen: HashSet::new(),
        pending: VecDeque::new(),
        current_assets: Vec::new(),
        // Fully transparent by default: the canvas paints no background, the host page shows through (for a color use the data area's clear)
        clear: [0.0, 0.0, 0.0, 0.0],
        dirty: true,
        size,
        canvas,
        overlay: None,
    }));
    state.borrow_mut().apply(decode(&data)?);

    let id = CTXS.with(|c| {
        let mut v = c.borrow_mut();
        v.push(Some(Rc::clone(&state)));
        (v.len() - 1) as u32
    });
    Ok(id)
}

/// Contract update: data-area deltas (a full re-push after append/patch) — consumed idempotently.
#[wasm_bindgen]
pub fn update(ctx: u32, data: JsValue) -> Result<(), JsError> {
    let spec = decode(&data)?;
    with_ctx(ctx, |s| s.apply(spec))?;
    Ok(())
}

/// Contract resize: the canvas pixel size is the host's call.
#[wasm_bindgen]
pub fn resize(ctx: u32, w: u32, h: u32) -> Result<(), JsError> {
    with_ctx(ctx, |s| {
        s.size = (w, h);
        s.camera
            .set_viewport(Viewport::new_at_origo(w.max(1), h.max(1)));
        s.dirty = true;
    })?;
    Ok(())
}

/// Contract unmount: drop the state; the canvas/GL is reclaimed by the browser.
#[wasm_bindgen]
pub fn unmount(ctx: u32) {
    CTXS.with(|c| {
        if let Some(slot) = c.borrow_mut().get_mut(ctx as usize) {
            *slot = None;
        }
    });
}

/// One frame driven by the host's rAF: pump async assets first, then redraw if dirty.
#[wasm_bindgen]
pub fn frame(ctx: u32) -> Result<(), JsError> {
    // Land the previous async load into the scene first (inside this borrow window, so no cross-domain borrow conflict)
    let mut arrived: Vec<three_d_asset::Model> = Vec::new();
    LOADED.with(|q| {
        let mut rest = Vec::new();
        for (c, id, cpu) in q.borrow_mut().drain(..) {
            if c == ctx {
                arrived.push(cpu);
            } else {
                rest.push((c, id, cpu));
            }
        }
        *q.borrow_mut() = rest;
    });
    for cpu in arrived {
        with_ctx(ctx, |s| s.add_model(cpu))?;
    }
    let job = with_ctx(ctx, |s| s.pump())?;
    if let Some((id, url)) = job {
        // Spin in the container during the fetch (three-d has no built-in progress; the only built-in loading UI
        // is in GS3D over on the splash side)
        overlay_ensure(ctx);
        // reqwest only accepts absolute URLs: resolve a relative url against document.baseURI
        // (payloads are specified as same-origin relative paths, e.g. `/assets/3dbrowser/tile.glb`).
        let base = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.base_uri().ok().flatten())
            .unwrap_or_default();
        let url = web_sys::Url::new_with_base(&url, &base)
            .map(|u| u.href())
            .unwrap_or(url);
        // Async load: on success push back into LOADED, and the next frame lands it inside the borrow window;
        // a failed asset is not returned to seen, so the next update can retry
        wasm_bindgen_futures::spawn_local(async move {
            match three_d_asset::io::load_async(&[&url]).await {
                Ok(mut loaded) => {
                    // The failure reason must reach the console: `.ok()` swallows it, and problems like a missing feature /
                    // unknown extension show up as "the model silently doesn't appear"
                    match loaded.deserialize(&url) {
                        Ok(cpu) => LOADED.with(|q| q.borrow_mut().push((ctx, id, cpu))),
                        Err(e) => {
                            web_sys::console::warn_1(
                                &format!("3dbrowser: deserialize failed: {e:?} {url}").into(),
                            );
                            forget(ctx, &id);
                        }
                    }
                }
                Err(e) => {
                    web_sys::console::warn_1(&format!("3dbrowser: load failed: {e:?} {url}").into());
                    forget(ctx, &id);
                }
            }
            // Remove the overlay only when the queue is empty: no mid-way flicker when one payload carries several assets
            overlay_hide_if_idle(ctx);
        });
    }
    with_ctx(ctx, |s| {
        if s.dirty {
            s.render();
        }
    })?;
    Ok(())
}

/// Give a failed asset back to the retry path: drop it from `seen` (so the next `apply` of
/// the same `key:url` queues it again) and withdraw the "in place" claim from
/// `current_assets`. Only the FAILURE paths call this — on success the id must stay in
/// `seen`, or the next update of an unchanged spec would load it a second time.
fn forget(ctx: u32, id: &str) {
    if let Err(e) = with_ctx(ctx, |s| {
        s.seen.remove(id);
        s.current_assets.retain(|asset| asset != id);
    }) {
        web_sys::console::warn_1(
            &format!("3dbrowser: forget after a failed load: {e:?}").into(),
        );
    }
}

/// Left-button drag = orbit around the target (index.js installs the events and forwards them here).
#[wasm_bindgen]
pub fn push_drag(ctx: u32, dx: f32, dy: f32) {
    with_ctx(ctx, |s| {
        let t = s.camera.target();
        s.camera.rotate_around_with_fixed_up(t, dx * 0.01, dy * 0.01);
        s.dirty = true;
    })
    .ok();
}

/// Wheel = dolly the view.
#[wasm_bindgen]
pub fn push_wheel(ctx: u32, delta: f32) {
    with_ctx(ctx, |s| {
        s.camera.zoom(delta, 0.3, 60.0);
        s.dirty = true;
    })
    .ok();
}

/// Right-button drag = pan (same feel as three.js OrbitControls' pan): the camera and the target move
/// together along the camera's right/up plane. The step scales with view distance — for the same drag on screen,
/// near moves fewer world units and far moves more, matching an orbit controller's feel.
#[wasm_bindgen]
pub fn push_pan(ctx: u32, dx: f32, dy: f32) {
    with_ctx(ctx, |s| {
        let dist = (s.camera.position() - s.camera.target()).magnitude();
        let k = dist * 0.0015;
        let right = s.camera.right_direction();
        let up = right.cross(s.camera.view_direction());
        s.camera.translate(-right * (dx * k) + up * (dy * k));
        s.dirty = true;
    })
    .ok();
}
