//! The live view: a canvas inside the component's element, an animation loop,
//! pointer input, node labels, a hover card and focus flights.
//!
//! Everything visual is ours, so the awkward parts of the module era are gone:
//! no library to inject, no four-function contract, no UMD-vs-AMD hijack, no
//! second copy of the same data in a codec. The component creates the host
//! element; this file adds a canvas plus a DOM overlay to it and keeps drawing.
//!
//! Rules that come from measurements, not taste:
//! - forces are cooled by an `alpha` that decays toward a small floor. Running
//!   at full strength forever is what "the graph never stops shaking" looks
//!   like; data changes and node drags reheat it instead;
//! - physics runs in fixed 1/60 s substeps while `dt` itself is clamped, so a
//!   background tab's catch-up frame cannot fling nodes across the scene;
//! - an append never re-frames the camera: only the first non-empty snapshot
//!   auto-fits, and only an explicit `focus` moves the camera afterwards;
//! - labels and the card are DOM elements updated per frame. Per-frame signal
//!   writes into leptos would rebuild the subtree sixty times a second.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlCanvasElement, HtmlElement, PointerEvent, WheelEvent};

use crate::camera::Camera;
use crate::force;
use crate::math::Vec3;
use crate::pick;
use crate::render::{self, Renderer};
use crate::scene::{Scene, Snapshot};
use crate::style::Style;

/// Pointer travel (CSS px) below which press-and-release counts as a click.
const CLICK_SLOP_PX: f32 = 4.0;
const ORBIT_RADIANS_PER_PX: f32 = 0.006;
const PITCH_LIMIT: f32 = 1.45;
const FLIGHT_MS: f64 = 420.0;
/// Padding used when framing a focus target, in CSS pixels.
const FOCUS_PADDING_PX: f32 = 80.0;
/// Fixed physics step; the accumulator feeds it whole steps only.
const STEP: f32 = 1.0 / 60.0;

struct Drag {
    pointer_id: i32,
    x: f32,
    y: f32,
    moved: bool,
    /// `Some(index)` = dragging that node, `None` = orbiting the camera.
    node: Option<usize>,
}

struct Flight {
    from_target: Vec3,
    from_distance: f32,
    to_target: Vec3,
    to_distance: f32,
    elapsed_ms: f64,
    duration_ms: f64,
}

struct State {
    camera: Camera,
    scene: Scene,
    style: Style,
    drag: Option<Drag>,
    hover: Option<usize>,
    flight: Option<Flight>,
    last_ts: f64,
    /// Cooling factor for the layout (1 = hot, floor = settled).
    alpha: f32,
    /// Leftover time not yet consumed by a fixed physics step.
    acc: f32,
}

/// DOM overlay: one label per node plus the hover card. Kept outside leptos on
/// purpose — it is rewritten every frame.
struct Overlay {
    layer: HtmlElement,
    card: HtmlElement,
    labels: Vec<HtmlElement>,
}

pub struct View {
    canvas: HtmlCanvasElement,
    renderer: Rc<RefCell<Renderer>>,
    state: Rc<RefCell<State>>,
    overlay: Rc<RefCell<Overlay>>,
    emit: Rc<dyn Fn(&str, serde_json::Value)>,
    tick: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>,
    raf: Rc<Cell<i32>>,
    listeners: Vec<(&'static str, js_sys::Function)>,
}

impl View {
    pub fn mount(
        host: &HtmlElement,
        style: Style,
        emit: impl Fn(&str, serde_json::Value) + 'static,
    ) -> Option<Self> {
        let document = web_sys::window()?.document()?;
        // Overlay children are absolutely positioned against the host.
        host.style().set_property("position", "relative").ok()?;

        let canvas = document
            .create_element("canvas")
            .ok()?
            .dyn_into::<HtmlCanvasElement>()
            .ok()?;
        canvas.set_class_name("fg-canvas");
        let css = canvas.style();
        css.set_property("display", "block").ok()?;
        css.set_property("width", "100%").ok()?;
        css.set_property("height", "100%").ok()?;
        host.append_child(&canvas).ok()?;

        let layer = document.create_element("div").ok()?.dyn_into::<HtmlElement>().ok()?;
        layer.set_class_name("fg-labels");
        let layer_css = layer.style();
        layer_css.set_property("position", "absolute").ok()?;
        layer_css.set_property("inset", "0").ok()?;
        layer_css.set_property("overflow", "hidden").ok()?;
        layer_css.set_property("pointer-events", "none").ok()?;
        layer_css.set_property("z-index", "1").ok()?;
        host.append_child(&layer).ok()?;

        let card = document.create_element("div").ok()?.dyn_into::<HtmlElement>().ok()?;
        card.set_class_name("fg-card");
        let card_css = card.style();
        card_css.set_property("position", "absolute").ok()?;
        card_css.set_property("display", "none").ok()?;
        card_css.set_property("pointer-events", "none").ok()?;
        card_css.set_property("z-index", "2").ok()?;
        card_css.set_property("background", "#0f172a").ok()?;
        card_css.set_property("color", "#f8fafc").ok()?;
        card_css.set_property("font", "12px/1.45 system-ui, sans-serif").ok()?;
        card_css.set_property("padding", "6px 8px").ok()?;
        card_css.set_property("border-radius", "6px").ok()?;
        card_css.set_property("white-space", "nowrap").ok()?;
        host.append_child(&card).ok()?;

        let gl = render::context(&canvas)?;
        let renderer = Rc::new(RefCell::new(Renderer::new(gl)?));
        let state = Rc::new(RefCell::new(State {
            camera: Camera::default(),
            scene: Scene::default(),
            style,
            drag: None,
            hover: None,
            flight: None,
            last_ts: 0.0,
            alpha: force::ALPHA_START,
            acc: 0.0,
        }));
        let overlay = Rc::new(RefCell::new(Overlay {
            layer,
            card,
            labels: Vec::new(),
        }));

        let mut view = Self {
            canvas,
            renderer,
            state,
            overlay,
            emit: Rc::new(emit),
            tick: Rc::new(RefCell::new(None)),
            raf: Rc::new(Cell::new(0)),
            listeners: Vec::new(),
        };
        view.install_input();
        view.start_loop();
        Some(view)
    }

    /// Feed a payload snapshot. Surviving ids keep their position; only the
    /// first non-empty snapshot frames the camera, and only a focus change
    /// moves it afterwards.
    pub fn set_payload(&mut self, snapshot: &Snapshot) {
        let viewport = self.viewport();
        let mut guard = self.state.borrow_mut();
        let state: &mut State = &mut guard;
        let diff = state.scene.apply(snapshot.clone());
        let points: Vec<Vec3> = state.scene.bodies.iter().map(|b| b.pos).collect();

        let (auto_fit, padding) = (state.style.auto_fit, state.style.fit_padding_px);
        if !state.scene.fitted && auto_fit && !points.is_empty() {
            state.camera.fit(&points, padding, viewport);
            state.scene.fitted = true;
        }
        // New nodes need the layout to move again — otherwise they sit on the
        // neighbour centroid and never settle into place.
        if !diff.added.is_empty() || diff.removed > 0 {
            state.alpha = state.alpha.max(force::ALPHA_REHEAT);
        }
        if diff.focus_changed {
            state.alpha = state.alpha.max(force::ALPHA_REHEAT);
            let focused = state.scene.focus_points();
            if !focused.is_empty() {
                let mut probe = state.camera;
                probe.fit(&focused, FOCUS_PADDING_PX, viewport);
                let duration_ms = state
                    .scene
                    .focus
                    .as_ref()
                    .and_then(|f| f.duration_ms)
                    .map(f64::from)
                    .unwrap_or(FLIGHT_MS)
                    .max(1.0);
                state.flight = Some(Flight {
                    from_target: state.camera.target,
                    from_distance: state.camera.distance,
                    to_target: probe.target,
                    to_distance: probe.distance,
                    elapsed_ms: 0.0,
                    duration_ms,
                });
            }
        }
    }

    fn viewport(&self) -> (f32, f32) {
        (
            self.canvas.client_width().max(1) as f32,
            self.canvas.client_height().max(1) as f32,
        )
    }

    fn start_loop(&self) {
        let state = Rc::clone(&self.state);
        let renderer = Rc::clone(&self.renderer);
        let overlay = Rc::clone(&self.overlay);
        let canvas = self.canvas.clone();
        let tick_slot: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::clone(&self.tick);
        let raf = Rc::clone(&self.raf);

        let reschedule = Rc::clone(&tick_slot);
        let callback = Closure::<dyn FnMut(f64)>::new(move |ts: f64| {
            let viewport = (
                canvas.client_width().max(1) as f32,
                canvas.client_height().max(1) as f32,
            );
            let dpr = web_sys::window()
                .map(|w| w.device_pixel_ratio() as f32)
                .unwrap_or(1.0)
                .clamp(0.5, 4.0);
            let px = (
                (viewport.0 * dpr).round().max(1.0) as u32,
                (viewport.1 * dpr).round().max(1.0) as u32,
            );
            if canvas.width() != px.0 || canvas.height() != px.1 {
                canvas.set_width(px.0);
                canvas.set_height(px.1);
            }

            {
                let mut guard = state.borrow_mut();
                let st: &mut State = &mut guard;
                let dt = if st.last_ts > 0.0 {
                    ((ts - st.last_ts) / 1000.0).clamp(0.0, 0.1) as f32
                } else {
                    STEP
                };
                st.last_ts = ts;

                if let Some(flight) = st.flight.as_mut() {
                    flight.elapsed_ms += (dt * 1000.0) as f64;
                    let t = (flight.elapsed_ms / flight.duration_ms).clamp(0.0, 1.0) as f32;
                    // Smoothstep: no overshoot, no snap at the end.
                    let e = t * t * (3.0 - 2.0 * t);
                    st.camera.target = flight.from_target.lerp(flight.to_target, e);
                    st.camera.distance =
                        flight.from_distance + (flight.to_distance - flight.from_distance) * e;
                    if t >= 1.0 {
                        st.flight = None;
                    }
                }

                let params = st.style.params();
                st.acc = (st.acc + dt).min(STEP * 4.0);
                let mut steps = 0;
                while st.acc >= STEP && steps < 4 {
                    let scene = &mut st.scene;
                    force::step(&mut scene.bodies, &scene.links, &params, STEP, st.alpha);
                    st.acc -= STEP;
                    st.alpha = (st.alpha * force::ALPHA_DECAY).max(force::ALPHA_FLOOR);
                    steps += 1;
                }

                renderer
                    .borrow_mut()
                    .draw(&st.camera, viewport, &st.scene, &st.style);
                update_overlay(&mut overlay.borrow_mut(), st, viewport);
                // Assertable state for e2e (the canvas has no DOM children).
                let _ = canvas.set_attribute(
                    "data-fg",
                    &format!(
                        "nodes={} links={} alpha={:.3}",
                        st.scene.nodes.len(),
                        st.scene.links.len(),
                        st.alpha
                    ),
                );
            }

            if let Some(window) = web_sys::window() {
                let slot = reschedule.borrow();
                if let Some(cb) = slot.as_ref() {
                    if let Ok(id) = window.request_animation_frame(cb.as_ref().unchecked_ref()) {
                        raf.set(id);
                    }
                }
            }
        });

        if let Some(window) = web_sys::window() {
            if let Ok(id) = window.request_animation_frame(callback.as_ref().unchecked_ref()) {
                self.raf.set(id);
            }
        }
        *self.tick.borrow_mut() = Some(callback);
    }

    fn install_input(&mut self) {
        let canvas = self.canvas.clone();
        let state = Rc::clone(&self.state);
        let on_down = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
            let rect = canvas.get_bounding_client_rect();
            let point = (
                event.client_x() as f32 - rect.left() as f32,
                event.client_y() as f32 - rect.top() as f32,
            );
            let viewport = (
                rect.width().max(1.0) as f32,
                rect.height().max(1.0) as f32,
            );
            let mut guard = state.borrow_mut();
            let st: &mut State = &mut guard;
            let max_weight = st.scene.max_weight();
            let radii: Vec<f32> = st
                .scene
                .bodies
                .iter()
                .map(|b| st.style.radius_px(b.weight, max_weight))
                .collect();
            // A press that lands on a node drags that node; anywhere else
            // orbits the camera. Without this the graph is read-only.
            let node = pick::pick(&st.camera, viewport, &st.scene.bodies, &radii, point);
            if let Some(index) = node {
                if let Some(body) = st.scene.bodies.get_mut(index) {
                    body.pinned = true;
                    body.vel = Vec3::ZERO;
                }
                st.alpha = force::ALPHA_REHEAT;
            }
            st.drag = Some(Drag {
                pointer_id: event.pointer_id(),
                x: point.0,
                y: point.1,
                moved: false,
                node,
            });
            canvas.set_pointer_capture(event.pointer_id()).ok();
        });
        let function = on_down.as_ref().unchecked_ref::<js_sys::Function>().clone();
        on_down.forget();
        self.listen("pointerdown", function);

        let canvas_move = self.canvas.clone();
        let state = Rc::clone(&self.state);
        let on_move = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
            let rect = canvas_move.get_bounding_client_rect();
            let point = (
                event.client_x() as f32 - rect.left() as f32,
                event.client_y() as f32 - rect.top() as f32,
            );
            let viewport = (
                rect.width().max(1.0) as f32,
                rect.height().max(1.0) as f32,
            );
            let max_weight;
            let mut guard = state.borrow_mut();
            let st: &mut State = &mut guard;
            max_weight = st.scene.max_weight();
            let radii: Vec<f32> = st
                .scene
                .bodies
                .iter()
                .map(|b| st.style.radius_px(b.weight, max_weight))
                .collect();

            match st.drag.as_mut() {
                Some(drag) if drag.pointer_id == event.pointer_id() => {
                    let (dx, dy) = (point.0 - drag.x, point.1 - drag.y);
                    if !drag.moved && (dx.abs() > CLICK_SLOP_PX || dy.abs() > CLICK_SLOP_PX) {
                        drag.moved = true;
                    }
                    if drag.moved {
                        match drag.node {
                            Some(index) => {
                                // Move the node on the plane through it, parallel
                                // to the screen: screen delta / px_per_unit.
                                if let Some(body) = st.scene.bodies.get(index).copied() {
                                    if let Some(screen) = st.camera.project(viewport, body.pos) {
                                        let basis = st.camera.basis();
                                        let per_px = 1.0 / screen.px_per_unit.max(1e-6);
                                        let world = basis.right * (dx * per_px)
                                            - basis.up * (dy * per_px);
                                        if let Some(body) = st.scene.bodies.get_mut(index) {
                                            body.pos += world;
                                            body.vel = Vec3::ZERO;
                                        }
                                    }
                                }
                                st.alpha = force::ALPHA_REHEAT.max(0.35);
                            }
                            None => {
                                st.camera.yaw -= dx * ORBIT_RADIANS_PER_PX;
                                st.camera.pitch = (st.camera.pitch + dy * ORBIT_RADIANS_PER_PX)
                                    .clamp(-PITCH_LIMIT, PITCH_LIMIT);
                                st.flight = None;
                            }
                        }
                        drag.x = point.0;
                        drag.y = point.1;
                    }
                }
                _ => {
                    // No press held: hover feedback only.
                    st.hover = pick::pick(&st.camera, viewport, &st.scene.bodies, &radii, point);
                }
            }
        });
        let function = on_move.as_ref().unchecked_ref::<js_sys::Function>().clone();
        on_move.forget();
        self.listen("pointermove", function);

        let state = Rc::clone(&self.state);
        let emit = Rc::clone(&self.emit);
        let on_up = Closure::<dyn FnMut(PointerEvent)>::new(move |_event: PointerEvent| {
            let (hit, dragged_node) = {
                let mut guard = state.borrow_mut();
                let st: &mut State = &mut guard;
                let Some(drag) = st.drag.take() else {
                    return;
                };
                if let Some(index) = drag.node {
                    if let Some(body) = st.scene.bodies.get_mut(index) {
                        body.pinned = false;
                    }
                    st.alpha = force::ALPHA_REHEAT;
                }
                if drag.moved {
                    (None, drag.node)
                } else {
                    (drag.node, None)
                }
            };
            let _ = dragged_node;
            match hit {
                Some(index) => {
                    let st = state.borrow();
                    let node = st.scene.nodes.get(index);
                    emit(
                        "node-click",
                        serde_json::json!({
                            "at": "node",
                            "id": node.map(|n| n.id.clone()).unwrap_or_default(),
                            "name": node.and_then(|n| n.name.clone()),
                            "kind": node.and_then(|n| n.kind.clone()),
                            "weight": node.and_then(|n| n.weight),
                        }),
                    );
                }
                None => emit(
                    "background-click",
                    serde_json::json!({ "at": "background" }),
                ),
            }
        });
        let function = on_up.as_ref().unchecked_ref::<js_sys::Function>().clone();
        on_up.forget();
        self.listen("pointerup", function);

        let state = Rc::clone(&self.state);
        let on_wheel = Closure::<dyn FnMut(WheelEvent)>::new(move |event: WheelEvent| {
            event.prevent_default();
            let mut guard = state.borrow_mut();
            let st: &mut State = &mut guard;
            st.flight = None;
            let factor = (event.delta_y() as f32 * 0.001).exp();
            st.camera.distance = (st.camera.distance * factor).clamp(1.0, 100_000.0);
        });
        let function = on_wheel.as_ref().unchecked_ref::<js_sys::Function>().clone();
        on_wheel.forget();
        self.listen("wheel", function);
    }

    fn listen(&mut self, name: &'static str, function: js_sys::Function) {
        self.canvas
            .add_event_listener_with_callback(name, &function)
            .ok();
        self.listeners.push((name, function));
    }

    pub fn dispose(&mut self) {
        let id = self.raf.replace(0);
        if id != 0 {
            if let Some(window) = web_sys::window() {
                window.cancel_animation_frame(id).ok();
            }
        }
        self.tick.borrow_mut().take();
        for (name, function) in self.listeners.drain(..) {
            self.canvas
                .remove_event_listener_with_callback(name, &function)
                .ok();
        }
        if let Some(gl) = render::context(&self.canvas) {
            if let Ok(Some(ext)) = gl.get_extension("WEBGL_lose_context") {
                if let Ok(f) = js_sys::Reflect::get(&ext, &JsValue::from_str("loseContext")) {
                    if let Ok(f) = f.dyn_into::<js_sys::Function>() {
                        f.call0(&ext).ok();
                    }
                }
            }
        }
        let overlay = self.overlay.borrow();
        overlay.layer.remove();
        overlay.card.remove();
        self.canvas.remove();
    }
}

/// Per-frame DOM overlay: one absolutely positioned label per node plus the
/// hover card. Node positions come from the same projection the renderer uses,
/// so a label always sits on its sphere.
fn update_overlay(overlay: &mut Overlay, st: &State, viewport: (f32, f32)) {
    let nodes = &st.scene.nodes;
    let bodies = &st.scene.bodies;
    let want_labels = st.style.labels && nodes.len() <= st.style.label_max_nodes as usize;

    if overlay.labels.len() != nodes.len() {
        // Rebuild: the label set follows the node list one-for-one.
        for label in overlay.labels.drain(..) {
            label.remove();
        }
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        for _ in nodes {
            match document.create_element("div").ok().and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
                Some(el) => {
                    let css = el.style();
                    css.set_property("position", "absolute").ok();
                    css.set_property("left", "0").ok();
                    css.set_property("top", "0").ok();
                    css.set_property("transform", "translate(-2000px, -2000px)").ok();
                    css.set_property("white-space", "nowrap").ok();
                    css.set_property(
                        "font",
                        &format!("{}px/1.2 system-ui, sans-serif", st.style.label_px),
                    )
                    .ok();
                    css.set_property("color", "#0f172a").ok();
                    css.set_property("background", "rgba(255,255,255,0.72)").ok();
                    css.set_property("padding", "0 3px").ok();
                    css.set_property("border-radius", "3px").ok();
                    overlay.layer.append_child(&el).ok();
                    overlay.labels.push(el);
                }
                None => return,
            }
        }
    }

    for (index, label) in overlay.labels.iter().enumerate() {
        let Some(node) = nodes.get(index) else {
            label.style().set_property("display", "none").ok();
            continue;
        };
        let Some(body) = bodies.get(index) else {
            label.style().set_property("display", "none").ok();
            continue;
        };
        let Some(screen) = st.camera.project(viewport, body.pos) else {
            label.style().set_property("display", "none").ok();
            continue;
        };
        let inside = screen.x > -40.0
            && screen.y > -20.0
            && screen.x < viewport.0 + 40.0
            && screen.y < viewport.1 + 20.0;
        if !want_labels || !inside {
            label.style().set_property("display", "none").ok();
            continue;
        }
        let text = node.name.clone().unwrap_or_else(|| node.id.clone());
        if label.text_content().as_deref() != Some(text.as_str()) {
            label.set_text_content(Some(&text));
        }
        let css = label.style();
        css.set_property("display", "block").ok();
        css.set_property(
            "transform",
            &format!(
                "translate({}px, {}px) translate(-50%, -140%)",
                screen.x.round(),
                screen.y.round()
            ),
        )
        .ok();
    }

    // Hover card: details for the node under the cursor.
    match st.hover.and_then(|i| nodes.get(i).map(|n| (i, n))) {
        Some((index, node)) => {
            let Some(body) = bodies.get(index) else {
                overlay.card.style().set_property("display", "none").ok();
                return;
            };
            let Some(screen) = st.camera.project(viewport, body.pos) else {
                overlay.card.style().set_property("display", "none").ok();
                return;
            };
            let name = node.name.clone().unwrap_or_else(|| node.id.clone());
            let kind = node.kind.clone().unwrap_or_else(|| "—".to_string());
            let weight = node.weight.unwrap_or(1.0);
            overlay.card.set_text_content(Some(&format!(
                "{name}\n{kind} · weight {weight:.0}"
            )));
            let css = overlay.card.style();
            css.set_property("white-space", "pre").ok();
            css.set_property("display", "block").ok();
            let _ = css
                .set_property(
                    "transform",
                    &format!(
                        "translate({}px, {}px) translate(-50%, -260%)",
                        screen.x.round(),
                        screen.y.round()
                    ),
                )
                .ok();
        }
        None => {
            let _ = overlay.card.style().set_property("display", "none");
        }
    }
}