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
//! - the frame loop is demand-driven. A settled, unfocused canvas with the
//!   pointer elsewhere and the tab in front has nothing to draw, so it draws
//!   nothing: the last frame stays on screen and the loop is asleep until an
//!   event wakes it. A 60 Hz callback that repaints an unchanged scene is a
//!   laptop fan with a graph next to it;
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
use crate::render::Renderer;
use crate::scene::{Scene, Snapshot};
use crate::frame::{Frame, LinkShape};
use crate::style::{Style, Theme};

/// Pointer travel (CSS px) below which press-and-release counts as a click.
const CLICK_SLOP_PX: f32 = 4.0;

/// Distance between the hovered node's centre and the detail card, in CSS px.
const CARD_GAP_PX: f32 = 14.0;
const ORBIT_RADIANS_PER_PX: f32 = 0.006;
// No pitch limit: a frame has no pole to hit, so the drag can tip past vertical
// and keep going (see `Camera::orbit`).
const FLIGHT_MS: f64 = 420.0;
/// How far the camera may drift from "the graph fills the pane" before it is
/// corrected: a fifth either way is invisible, a factor of two is a graph the
/// viewer is standing inside.
const FIT_RATIO_LOW: f32 = 0.8;
const FIT_RATIO_HIGH: f32 = 1.25;
const FIT_MS: f64 = 320.0;
/// Padding used when framing a focus target, in CSS pixels.
const FOCUS_PADDING_PX: f32 = 80.0;
/// Fixed physics step; the accumulator feeds it whole steps only.
const STEP: f32 = 1.0 / 60.0;
/// Cooling factor below which the layout is considered at rest. The floor is
/// `force::ALPHA_FLOOR`; the small headroom above it lets the last couple of
/// collision passes heal an overlap that the springs had already given up on
/// before the loop goes quiet.
const IDLE_ALPHA: f32 = force::ALPHA_FLOOR * 1.6;
/// Simulated steps run before a brand-new graph is first painted. Cooling is
/// frame-based, so skipping this leaves the layout half-relaxed when it freezes.
const RELAX_STEPS: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DragMode {
    /// Left drag on empty space.
    Orbit,
    /// Right or middle drag: slide the camera target across the view plane.
    Pan,
    /// Left drag that started on a node.
    Node(usize),
}

struct Drag {
    pointer_id: i32,
    x: f32,
    y: f32,
    moved: bool,
    mode: DragMode,
}

struct Flight {
    from_target: Vec3,
    from_distance: f32,
    to_target: Vec3,
    to_distance: f32,
    elapsed_ms: f64,
    duration_ms: f64,
}

/// The loop's handle: who is scheduled, whether the page is visible, and the
/// callback to schedule. Cloned into every event handler so any of them can
/// start the loop again — that is the whole contract of an idle loop. Miss a
/// wake and the canvas freezes on a stale frame; wake too eagerly and the
/// saving is gone, so the list is exactly "things that can change the picture".
#[derive(Clone)]
struct Ticker {
    /// Frame id of the scheduled callback, 0 when none is scheduled.
    raf: Rc<Cell<i32>>,
    /// True while a frame is pending or being executed. `wake` is a no-op then.
    running: Rc<Cell<bool>>,
    /// The rAF callback, kept alive for the lifetime of the view.
    tick: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>,
    /// `document.hidden`, mirrored on every `visibilitychange`.
    hidden: Rc<Cell<bool>>,
}

impl Ticker {
    fn new(tick: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>, hidden: bool) -> Self {
        Self {
            raf: Rc::new(Cell::new(0)),
            running: Rc::new(Cell::new(false)),
            tick,
            hidden: Rc::new(Cell::new(hidden)),
        }
    }

    /// Schedule a frame unless one is already on its way. Never starts a loop
    /// in a hidden tab: the next `visibilitychange` does that instead.
    fn wake(&self) {
        if self.running.get() || self.hidden.get() {
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let slot = self.tick.borrow();
        let Some(cb) = slot.as_ref() else {
            return;
        };
        if let Ok(id) = window.request_animation_frame(cb.as_ref().unchecked_ref()) {
            self.raf.set(id);
            self.running.set(true);
        }
    }

    /// Drop any scheduled frame and go quiet (tab hidden, view disposed).
    fn halt(&self) {
        if let Some(window) = web_sys::window() {
            let id = self.raf.replace(0);
            if id != 0 {
                window.cancel_animation_frame(id).ok();
            }
        }
        self.running.set(false);
    }
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
    /// One refit is allowed after the first layout settles — the seed sphere is
    /// much larger than the settled cloud, so fitting only at the first
    /// snapshot leaves the graph as a small knot in the middle of the pane.
    settle_fit_pending: bool,
    /// Set once the user orbits or zooms: after that the camera is theirs.
    camera_touched: bool,
    /// Pixels per world unit at the pivot plane, as of the last collision pass.
    /// World-space collision radius, kept only so the debug attribute can
    /// report it.
    collide_world: f32,
    /// Whether the pivot has been moved to the settled cloud's centre since the
    /// last time the node set changed.
    pivot_settled: bool,
    /// Whether the pointer is inside the canvas. The halo's breath is the only
    /// animation that outlives a settled layout, and it runs only while someone
    /// is actually looking at the graph — there is no CSS overlay to hand it to
    /// instead, because the haloes are per-node billboards inside the GL scene,
    /// not boxes on the label layer.
    pointer_inside: bool,
    /// The `data-fg` value last written. Comparing before writing keeps a frame
    /// that changes nothing from invalidating style for the whole panel.
    attr: String,
}

impl State {
    /// Collision radius in world units.
    ///
    /// Deliberately independent of the camera. A radius derived from the current
    /// zoom closes a loop with auto-fit: the cloud grows, the camera pulls back,
    /// the pixel-sized radius grows in world units, the cloud grows again. That
    /// loop has no equilibrium — it ran to 175k world units in seconds. The
    /// layout is a property of the data; the viewport is not allowed to change
    /// the shape. What keeps spheres from touching on screen is the framing: at
    /// the distance that fits the cloud, the projected spacing is an order of
    /// magnitude above the pixel radius.
    fn set_collide_radii(&mut self) {
        let base = (self.style.link_distance * 0.5).max(1e-3);
        let max_weight = self.scene.max_weight();
        let radii: Vec<f32> = self
            .scene
            .nodes
            .iter()
            .map(|n| self.style.radius_px(n.weight.unwrap_or(1.0), max_weight))
            .collect();
        let biggest = radii.iter().copied().fold(1.0_f32, f32::max);
        for (index, body) in self.scene.bodies.iter_mut().enumerate() {
            let px = radii
                .get(index)
                .copied()
                .unwrap_or(self.style.node_size_px);
            // Heavier nodes are drawn bigger, so they get proportionally more
            // room — but never so little that they sit on top of a neighbour.
            body.collide = base * (px / biggest).max(0.6);
        }
        self.collide_world = base;
    }
}

/// Repaint everything the theme owns: the panel behind the canvas, the label
/// scrim and text, and the switch's own caption. The palette is read by
/// `frame::build`, so the next frame picks it up without touching the layout.
fn apply_theme(
    style: &Style,
    host: &HtmlElement,
    canvas: &HtmlCanvasElement,
    overlay: &Overlay,
    button: &HtmlElement,
) {
    let background = style.background();
    for css in [canvas.style(), host.style()] {
        match &background {
            Some(color) => css.set_property("background", color).ok(),
            None => css.set_property("background", "").ok(),
        };
    }
    let (foreground, scrim) = style.label_colors();
    overlay
        .layer
        .style()
        .set_property("color", foreground)
        .ok();
    for label in &overlay.labels {
        let css = label.style();
        css.set_property("color", foreground).ok();
        css.set_property("background", scrim).ok();
    }
    // The caption names the mode a click switches *to*.
    let (caption, mode) = match style.theme {
        Theme::Dark => ("亮", "dark"),
        Theme::Light => ("暗", "light"),
    };
    button.set_text_content(Some(caption));
    host.set_attribute("data-theme", mode).ok();
}

/// The label switch's caption: whether the names are being drawn.
fn label_caption(labels: bool) -> &'static str {
    if labels {
        "字"
    } else {
        "无字"
    }
}

/// The link-shape switch's caption: one character per shape, because the panel is
/// 240 px wide and the switch sits next to the theme one.
fn shape_label(shape: LinkShape) -> &'static str {
    match shape {
        LinkShape::Straight => "直",
        LinkShape::Axes => "轴",
        LinkShape::Tetra => "四面",
    }
}

/// DOM overlay: one label per node plus the hover card. Kept outside leptos on
/// purpose — it is rewritten every frame.
struct Overlay {
    layer: HtmlElement,
    card: HtmlElement,
    labels: Vec<HtmlElement>,
    /// The card's measured box and the text it was measured with. Sizing the
    /// placement needs the box (a card near the panel edge is clamped back
    /// inside it), and measuring only when the text changes keeps the frame
    /// loop free of a forced layout.
    card_box: Option<(f32, f32)>,
    card_text: String,
}

pub struct View {
    canvas: HtmlCanvasElement,
    /// Kept so a theme change from outside (a bind, an event) repaints the same
    /// surface the switch does, without re-mounting the view.
    host: HtmlElement,
    button: HtmlElement,
    renderer: Rc<RefCell<Renderer>>,
    state: Rc<RefCell<State>>,
    overlay: Rc<RefCell<Overlay>>,
    emit: Rc<dyn Fn(&str, serde_json::Value)>,
    /// The frame loop: demand-driven, woken by anything that changes the picture.
    ticker: Ticker,
    listeners: Vec<(&'static str, js_sys::Function)>,
    /// `visibilitychange` on the document and `resize` on the window.
    page_listeners: Vec<(&'static str, js_sys::Function)>,
    /// Watches the canvas itself: the panel can change size without the window
    /// doing so (a rack reflow, a sidebar collapsing), and a stale drawing
    /// buffer is a stretched graph.
    observer: Option<web_sys::ResizeObserver>,
}

impl View {
    pub fn mount(
        host: &HtmlElement,
        style: Style,
        emit: impl Fn(&str, serde_json::Value) + 'static,
    ) -> Option<Self> {
        let document = web_sys::window()?.document()?;
        // Overlay children are absolutely positioned against the host.
        // The graph is the panel: no padding of its own, so the canvas fills what
        // the layout gave the element and the fit routine frames the whole box.
        host.style().set_property("position", "relative").ok()?;
        host.style().set_property("padding", "0").ok()?;
        host.style().set_property("margin", "0").ok()?;
        host.style().set_property("overflow", "hidden").ok()?;

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
        let css = layer.style();
        css.set_property("position", "absolute").ok()?;
        css.set_property("inset", "0").ok()?;
        css.set_property("overflow", "hidden").ok()?;
        css.set_property("pointer-events", "none").ok()?;
        css.set_property("z-index", "1").ok()?;
        host.append_child(&layer).ok()?;

        let card = document.create_element("div").ok()?.dyn_into::<HtmlElement>().ok()?;
        card.set_class_name("fg-card");
        let card_css = card.style();
        card_css.set_property("position", "absolute").ok()?;
        // Absolute with `auto` offsets lays the card out at its *static*
        // position — which is below the canvas, i.e. outside the panel. The
        // offsets must be explicit, like the label layer's.
        card_css.set_property("left", "0").ok()?;
        card_css.set_property("top", "0").ok()?;
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

        let renderer = Rc::new(RefCell::new(Renderer::new(&canvas)?));
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
            settle_fit_pending: true,
            camera_touched: false,
            collide_world: 0.0,
            pivot_settled: false,
            pointer_inside: false,
            attr: String::new(),
        }));
        let overlay = Rc::new(RefCell::new(Overlay {
            layer,
            card,
            labels: Vec::new(),
            card_box: None,
            card_text: String::new(),
        }));
        // The loop's handle exists before any handler: every switch and pointer
        // listener below is a reason to run a frame.
        let ticker = Ticker::new(Rc::new(RefCell::new(None)), document.hidden());

        // The theme switch lives inside the panel: it repaints the surface, the
        // labels and the palette, so the same graph can be read on a light or a
        // dark page without touching the layout.
        let button = document
            .create_element("button")
            .ok()?
            .dyn_into::<HtmlElement>()
            .ok()?;
        button.set_class_name("fg-theme");
        {
            let css = button.style();
            css.set_property("position", "absolute").ok();
            css.set_property("top", "6px").ok();
            css.set_property("right", "8px").ok();
            css.set_property("z-index", "3").ok();
            css.set_property("font", "11px/1.6 system-ui, sans-serif").ok();
            css.set_property("padding", "0 6px").ok();
            css.set_property("border", "0").ok();
            css.set_property("border-radius", "4px").ok();
            css.set_property("cursor", "pointer").ok();
            css.set_property("background", "rgba(127,127,127,0.18)").ok();
            css.set_property("color", "inherit").ok();
        }
        button
            .set_attribute("data-fg-theme", "switch")
            .ok();
        host.append_child(&button).ok();

        // Second switch: which way the links leave their nodes.
        let shape_button = document
            .create_element("button")
            .ok()?
            .dyn_into::<HtmlElement>()
            .ok()?;
        shape_button.set_class_name("fg-shape");
        {
            let css = shape_button.style();
            css.set_property("position", "absolute").ok();
            css.set_property("top", "6px").ok();
            css.set_property("right", "52px").ok();
            css.set_property("z-index", "3").ok();
            css.set_property("font", "11px/1.6 system-ui, sans-serif").ok();
            css.set_property("padding", "0 6px").ok();
            css.set_property("border", "0").ok();
            css.set_property("border-radius", "4px").ok();
            css.set_property("cursor", "pointer").ok();
            css.set_property("background", "rgba(127,127,127,0.18)").ok();
            css.set_property("color", "inherit").ok();
        }
        // Third switch: the labels. `style.labels` is read every frame by the
        // overlay, so flipping it here takes effect on the next frame — no remount,
        // no relayout.
        let labels_button = document
            .create_element("button")
            .ok()?
            .dyn_into::<HtmlElement>()
            .ok()?;
        labels_button.set_class_name("fg-labels-toggle");
        {
            let css = labels_button.style();
            css.set_property("position", "absolute").ok();
            css.set_property("top", "6px").ok();
            css.set_property("right", "96px").ok();
            css.set_property("z-index", "3").ok();
            css.set_property("font", "11px/1.6 system-ui, sans-serif").ok();
            css.set_property("padding", "0 6px").ok();
            css.set_property("border", "0").ok();
            css.set_property("border-radius", "4px").ok();
            css.set_property("cursor", "pointer").ok();
            css.set_property("background", "rgba(127,127,127,0.18)").ok();
            css.set_property("color", "inherit").ok();
        }
        labels_button
            .set_attribute("data-fg-labels", "switch")
            .ok();
        labels_button.set_text_content(Some(label_caption(state.borrow().style.labels)));
        host.append_child(&labels_button).ok();
        {
            let state_btn = Rc::clone(&state);
            let caption_btn = labels_button.clone();
            let ticker_btn = ticker.clone();
            let cb = Closure::<dyn FnMut()>::new(move || {
                let labels = {
                    let mut state = state_btn.borrow_mut();
                    state.style.labels = !state.style.labels;
                    state.style.labels
                };
                caption_btn.set_text_content(Some(label_caption(labels)));
                // The label set is decided per frame, and the loop is asleep:
                // flipping the switch has to ask for the frame that draws it.
                ticker_btn.wake();
            });
            labels_button
                .add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())
                .ok();
            cb.forget();
        }

        shape_button.set_attribute("data-fg-shape", "switch").ok();
        shape_button.set_text_content(Some(shape_label(state.borrow().style.link_shape)));
        host.append_child(&shape_button).ok();
        {
            let state_btn = Rc::clone(&state);
            let label_btn = shape_button.clone();
            let ticker_btn = ticker.clone();
            let cb = Closure::<dyn FnMut()>::new(move || {
                let next = {
                    let mut state = state_btn.borrow_mut();
                    state.style.link_shape = match state.style.link_shape {
                        LinkShape::Straight => LinkShape::Axes,
                        LinkShape::Axes => LinkShape::Tetra,
                        LinkShape::Tetra => LinkShape::Straight,
                    };
                    state.style.link_shape
                };
                label_btn.set_text_content(Some(shape_label(next)));
                ticker_btn.wake();
            });
            shape_button
                .add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())
                .ok();
            cb.forget();
        }
        apply_theme(&state.borrow().style, &host, &canvas, &overlay.borrow(), &button);
        {
            let state_btn = Rc::clone(&state);
            let overlay_btn = Rc::clone(&overlay);
            let ticker_btn = ticker.clone();
            let (host_btn, canvas_btn, button_btn) = (host.clone(), canvas.clone(), button.clone());
            let cb = Closure::<dyn FnMut()>::new(move || {
                state_btn.borrow_mut().style.toggle_theme();
                let style = state_btn.borrow().style.clone();
                apply_theme(&style, &host_btn, &canvas_btn, &overlay_btn.borrow(), &button_btn);
                // The palette is read by `frame::build`: the spheres have to be
                // redrawn, and nothing else would ask for that frame.
                ticker_btn.wake();
            });
            button
                .add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())
                .ok();
            cb.forget();
        }

        let mut view = Self {
            canvas,
            host: host.clone(),
            button,
            renderer,
            state,
            overlay,
            emit: Rc::new(emit),
            ticker,
            listeners: Vec::new(),
            page_listeners: Vec::new(),
            observer: None,
        };
        view.install_input();
        view.install_lifecycle();
        view.start_loop();
        Some(view)
    }

    /// Switch the palette from outside the panel — a `theme` bind, an event the
    /// page turned into a patch. Presentation only: the scene, the camera and the
    /// layout are untouched, so nothing jumps when the theme flips.
    pub fn set_theme(&mut self, theme: Theme) {
        if self.state.borrow().style.theme == theme {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            state.style.theme = theme;
        }
        let style = self.state.borrow().style.clone();
        apply_theme(&style, &self.host, &self.canvas, &self.overlay.borrow(), &self.button);
        // Presentation only, but the palette is the frame's input: redraw once.
        self.ticker.wake();
    }

    pub fn theme(&self) -> Theme {
        self.state.borrow().style.theme
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

        // A graph that arrives whole is relaxed before it is first painted.
        // Cooling runs on frames, so without this the viewer watches a random
        // seed cloud drift for three seconds — and the frame budget is often
        // spent before the springs have won, which is what "the forces feel
        // weak, the layout is a tangle" actually was.
        let whole_graph_is_new =
            !state.scene.bodies.is_empty() && diff.added.len() == state.scene.nodes.len();
        if whole_graph_is_new {
            state.set_collide_radii();
            let params = state.style.params();
            let mut alpha = force::ALPHA_START;
            for _ in 0..RELAX_STEPS {
                let scene = &mut state.scene;
                force::step(&mut scene.bodies, &scene.links, &params, alpha);
                alpha = (alpha * force::ALPHA_DECAY).max(force::ALPHA_FLOOR);
            }
            // Kill the leftover velocity: forces stop at the alpha floor, but
            // velocity keeps integrating, so a pre-relaxed graph *keeps
            // drifting* after it was framed — which is why the camera distance
            // matched a cloud twice as small as the one on screen, and why the
            // orbit pivot sat off to one side.
            for body in state.scene.bodies.iter_mut() {
                body.vel = Vec3::ZERO;
            }
            state.alpha = alpha;
            state.settle_fit_pending = true;
        } else if !diff.added.is_empty() || diff.removed > 0 {
            // New nodes need the layout to move again — otherwise they sit on
            // the neighbour centroid and never settle into place.
            state.alpha = state.alpha.max(force::ALPHA_START);
            state.pivot_settled = false;
        }
        if diff.focus_changed {
            state.alpha = state.alpha.max(force::ALPHA_REHEAT);
            // Frame the highlighted nodes *and* the paths that lead out of them:
            // fitting to the selected node alone zooms so far in that the rest of
            // the graph spills off the panel.
            let mut focused = state.scene.focus_points();
            if state.style.highlight_reach {
                let (nodes, _) = crate::frame::reachable(&state.scene, state.style.reach_hops);
                let mut points: Vec<crate::math::Vec3> = nodes
                    .iter()
                    .filter_map(|index| state.scene.bodies.get(*index).map(|body| body.pos))
                    .collect();
                if !points.is_empty() {
                    points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
                    focused = points;
                }
            }
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
                state.camera_touched = true;
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
        drop(guard);
        // New data is exactly the case the loop must not sleep through: nodes
        // may be reheated, a flight may have been started, labels may have
        // changed. One wake covers all of it.
        self.ticker.wake();
    }

    fn viewport(&self) -> (f32, f32) {
        (
            self.canvas.client_width().max(1) as f32,
            self.canvas.client_height().max(1) as f32,
        )
    }

    /// Start the frame loop. It is *demand-driven*: a frame decides whether
    /// anything can still change and stops rescheduling when nothing can, so a
    /// settled graph in a visible tab costs one frame, not sixty a second.
    /// Whatever changes the picture calls `Ticker::wake`.
    fn start_loop(&mut self) {
        let ticker = self.ticker.clone();
        let state = Rc::clone(&self.state);
        let renderer = Rc::clone(&self.renderer);
        let overlay = Rc::clone(&self.overlay);
        let canvas = self.canvas.clone();
        let next = ticker.clone();

        let callback = Closure::<dyn FnMut(f64)>::new(move |ts: f64| {
            // A hidden tab draws nothing and costs nothing. `visibilitychange`
            // announces the flag; the loop restarts from there.
            if next.hidden.get() {
                next.running.set(false);
                next.raf.set(0);
                return;
            }
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

            let idle = {
                let mut guard = state.borrow_mut();
                let st: &mut State = &mut guard;
                // A frame after a sleep is a fresh start, not a catch-up: the
                // wall-clock gap is dropped rather than clamped, or waking from
                // an idle minute would integrate four capped steps at once.
                let dt = if st.last_ts > 0.0 && (ts - st.last_ts) < 500.0 {
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

                st.set_collide_radii();
                // The halo's clock: rings are derived from it in `frame::build`,
                // so the animation costs no state beyond one phase per lit node.
                st.scene.advance_glow(dt);

                // Physics is O(n²) and the cooling factor is the only thing that
                // says positions can still move. At the floor the springs,
                // charge and collision passes have nothing left to resolve — the
                // collision pass is what heals overlaps, and overlaps only
                // appear when something moved.
                if st.alpha > force::ALPHA_FLOOR {
                    let params = st.style.params();
                    st.acc = (st.acc + dt).min(STEP * 4.0);
                    let mut steps = 0;
                    while st.acc >= STEP && steps < 4 {
                        let scene = &mut st.scene;
                        force::step(&mut scene.bodies, &scene.links, &params, st.alpha);
                        st.acc -= STEP;
                        st.alpha = (st.alpha * force::ALPHA_DECAY).max(force::ALPHA_FLOOR);
                        steps += 1;
                    }
                } else {
                    st.acc = 0.0;
                }

                // The seed cloud is far wider than the settled one, so the fit
                // done at the first snapshot frames a graph that then shrinks.
                // Frame it once more when it has come to rest, and only if the
                // user has not taken the camera over.
                // Auto-fit is a standing rule, not a one-off. The layout keeps
                // growing after every append, and a camera framed once around
                // the *seed* cloud ends up inside the graph: the pivot then sits
                // below the surface, so a drag looks like the graph swinging
                // around a fixed point in empty space instead of turning about
                // its own middle. Reframe whenever the ratio has drifted far
                // enough to matter, and never while the user owns the camera.
                if st.style.auto_fit && !st.camera_touched && st.drag.is_none() {
                    let points: Vec<Vec3> = st.scene.bodies.iter().map(|b| b.pos).collect();
                    if points.len() > 1 {
                        let mut want = st.camera;
                        want.fit(&points, st.style.fit_padding_px, viewport);
                        let ratio = st.camera.distance / want.distance.max(1e-3);
                        if !(FIT_RATIO_LOW..=FIT_RATIO_HIGH).contains(&ratio) {
                            for body in st.scene.bodies.iter_mut() {
                                body.vel = Vec3::ZERO;
                            }
                            st.flight = Some(Flight {
                                from_target: st.camera.target,
                                from_distance: st.camera.distance,
                                to_target: want.target,
                                to_distance: want.distance,
                                elapsed_ms: 0.0,
                                duration_ms: FIT_MS,
                            });
                        }
                    }
                }

                if st.settle_fit_pending {
                    if !st.camera_touched && st.style.auto_fit && st.alpha <= IDLE_ALPHA {
                        let points: Vec<Vec3> =
                            st.scene.bodies.iter().map(|b| b.pos).collect();
                        if points.len() > 1 {
                            let mut camera = st.camera;
                            camera.fit(&points, st.style.fit_padding_px, viewport);
                            st.camera = camera;
                        }
                        st.settle_fit_pending = false;
                    } else if st.camera_touched || !st.style.auto_fit {
                        // The refit can never be honoured once the camera is the
                        // user's (or autofit is off), and a pending flag that
                        // nothing can clear is a loop that never sleeps: the
                        // graphs with a focus directive were still drawing 60
                        // times a second long after everything had settled.
                        st.settle_fit_pending = false;
                    }
                }

                // Decide, then draw: everything about *what* is shown lives in
                // `frame::build` (pure, host-tested); the backend only uploads.
                let frame =
                    crate::frame::build(&st.camera, viewport, dpr, &st.scene, &st.style);
                renderer.borrow_mut().draw(&frame);
                update_overlay(&mut overlay.borrow_mut(), st, viewport);

                // Nothing left to animate: the layout is at rest, no flight, no
                // drag, no refit owed, and no pointer over the canvas to breathe
                // the halo for. Then this frame is the last one until an event
                // asks for another.
                let at_rest = st.alpha <= IDLE_ALPHA
                    && st.flight.is_none()
                    && st.drag.is_none()
                    && !st.settle_fit_pending;
                let idle = at_rest && !st.pointer_inside;
                Self::write_debug(st, &canvas, viewport, &frame, idle);
                idle
            };

            if idle {
                ticker.running.set(false);
                ticker.raf.set(0);
                return;
            }
            let scheduled = web_sys::window().and_then(|window| {
                let slot = ticker.tick.borrow();
                slot.as_ref()
                    .and_then(|cb| window.request_animation_frame(cb.as_ref().unchecked_ref()).ok())
            });
            match scheduled {
                Some(id) => ticker.raf.set(id),
                None => {
                    ticker.running.set(false);
                    ticker.raf.set(0);
                }
            }
        });

        *self.ticker.tick.borrow_mut() = Some(callback);
        self.ticker.wake();
    }

    /// Page-level reasons to run a frame: the tab becoming visible again, and the
    /// canvas changing size. A `ResizeObserver` on the canvas covers a rack
    /// reflow that never resizes the window; the window listener is the cheap
    /// belt for browsers that fire it first.
    fn install_lifecycle(&mut self) {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let ticker = self.ticker.clone();
        let on_visibility = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
            let hidden = web_sys::window()
                .and_then(|w| w.document())
                .map(|d| d.hidden())
                .unwrap_or(false);
            ticker.hidden.set(hidden);
            if hidden {
                ticker.halt();
            } else {
                // No state is touched while hidden, so one frame redraws the
                // page exactly as it was left.
                ticker.wake();
            }
        });
        let function = on_visibility
            .as_ref()
            .unchecked_ref::<js_sys::Function>()
            .clone();
        on_visibility.forget();
        document
            .add_event_listener_with_callback("visibilitychange", &function)
            .ok();
        self.page_listeners.push(("visibilitychange", function));

        if let Some(window) = web_sys::window() {
            let ticker = self.ticker.clone();
            let on_resize = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
                ticker.wake();
            });
            let function = on_resize.as_ref().unchecked_ref::<js_sys::Function>().clone();
            on_resize.forget();
            window
                .add_event_listener_with_callback("resize", &function)
                .ok();
            self.page_listeners.push(("resize", function));
        }

        let ticker = self.ticker.clone();
        let on_canvas_resize =
            Closure::<dyn FnMut(js_sys::Array, web_sys::ResizeObserver)>::new(
                move |_entries: js_sys::Array, _observer: web_sys::ResizeObserver| {
                    ticker.wake();
                },
            );
        if let Ok(observer) =
            web_sys::ResizeObserver::new(on_canvas_resize.as_ref().unchecked_ref())
        {
            let element: &web_sys::Element = self.canvas.as_ref();
            observer.observe(element);
            self.observer = Some(observer);
        }
        on_canvas_resize.forget();
    }

    /// The canvas's only assertable state (it has no DOM children): counts, the
    /// cooling factor and the camera's numbers. Written *only when the string
    /// changed* — the previous version wrote a 200-character attribute every
    /// frame, which invalidates style for the whole panel, and computed a
    /// pairwise `min_px` over every projected node at 60 Hz, which is O(n²) for
    /// a number the camera also moves (the journal's own note: it is a
    /// screen-space distance, not an overlap test). Both are gone; what a
    /// checker can read is unchanged apart from `min_px`, and `idle` now says
    /// whether the loop has stopped.
    fn write_debug(
        st: &mut State,
        canvas: &HtmlCanvasElement,
        viewport: (f32, f32),
        frame: &Frame,
        idle: bool,
    ) {
        // Cloud centre and screen extent: one projection pass, no pairs.
        let mut cx = 0.0_f32;
        let mut cy = 0.0_f32;
        let (mut x0, mut x1) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut y0, mut y1) = (f32::INFINITY, f32::NEG_INFINITY);
        let mut seen = 0_usize;
        for body in &st.scene.bodies {
            let Some(screen) = st.camera.project(viewport, body.pos) else {
                continue;
            };
            cx += screen.x;
            cy += screen.y;
            x0 = x0.min(screen.x);
            x1 = x1.max(screen.x);
            y0 = y0.min(screen.y);
            y1 = y1.max(screen.y);
            seen += 1;
        }
        let (cx, cy, cw, ch) = if seen == 0 {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            let n = seen as f32;
            (cx / n, cy / n, x1 - x0, y1 - y0)
        };
        let text = format!(
            "nodes={} links={} alpha={:.3} c={:.0},{:.0} spread={:.0}x{:.0} d={:.0} r={:.1} vp={:.0}x{:.0} ppu={:.2} cl={:.1} touch={} sel={} halo={} domes={} ph={:.2} idle={}",
            st.scene.nodes.len(),
            st.scene.links.len(),
            st.alpha,
            cx,
            cy,
            cw,
            ch,
            st.camera.distance,
            st.scene
                .bodies
                .iter()
                .map(|b| (b.pos - st.camera.target).len())
                .fold(0.0_f32, f32::max),
            viewport.0,
            viewport.1,
            st.collide_world,
            st.scene
                .bodies
                .iter()
                .map(|b| b.collide)
                .fold(f32::INFINITY, f32::min),
            u8::from(st.camera_touched),
            st.scene
                .selected
                .map(|i| i.to_string())
                .unwrap_or_else(|| "-".to_string()),
            frame.haloes.len(),
            frame.domes.len(),
            st.scene.glow_phase(),
            u8::from(idle),
        );
        // Formatting allocates; touching the DOM does not happen unless the value
        // moved. That is the difference between a style invalidation per frame
        // and none at all while the loop is asleep.
        if st.attr != text {
            let _ = canvas.set_attribute("data-fg", &text);
            st.attr = text;
        }
    }

    fn install_input(&mut self) {
        let canvas = self.canvas.clone();
        let state = Rc::clone(&self.state);
        // Every handler below is a reason to run a frame; with an idle loop, a
        // handler that forgets to wake it is a frozen canvas.
        let ticker = self.ticker.clone();
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
            let node = pick::pick(&st.camera, viewport, &st.scene.bodies, &radii, point);
            // Left on a node drags it, left on nothing orbits, right (or middle)
            // pans — the split every 3D editor uses, and the only way to reach
            // a part of the graph that orbiting alone cannot bring into view.
            let mode = match event.button() {
                1 | 2 => DragMode::Pan,
                _ => match node {
                    // Nothing is pinned and nothing is reheated on press: a
                    // press that never moves is a *click* (selection), and doing
                    // either made the graph jog under the pointer — measured 11px
                    // of drift, which moved the node out from under the next
                    // click and made the halo look like it had moved. Both belong
                    // to the first real movement.
                    Some(index) => DragMode::Node(index),
                    None => DragMode::Orbit,
                },
            };
            if event.button() != 0 {
                // No middle-click autoscroll, no right-click menu mid-pan.
                event.prevent_default();
            }
            st.drag = Some(Drag {
                pointer_id: event.pointer_id(),
                x: point.0,
                y: point.1,
                moved: false,
                mode,
            });
            canvas.set_pointer_capture(event.pointer_id()).ok();
            // A press starts an interaction the loop must animate through.
            st.pointer_inside = true;
            ticker.wake();
        });
        let function = on_down.as_ref().unchecked_ref::<js_sys::Function>().clone();
        on_down.forget();
        self.listen("pointerdown", function);

        let canvas_move = self.canvas.clone();
        let state = Rc::clone(&self.state);
        let ticker = self.ticker.clone();
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
                        // A drag is what needs the layout awake and the node held
                        // still; a click needs neither.
                        if let DragMode::Node(index) = drag.mode {
                            if let Some(body) = st.scene.bodies.get_mut(index) {
                                body.pinned = true;
                                body.vel = Vec3::ZERO;
                            }
                            st.alpha = force::ALPHA_REHEAT;
                        }
                    }
                    if drag.moved {
                        match drag.mode {
                            DragMode::Node(index) => {
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
                            DragMode::Orbit => {
                                st.camera_touched = true;
                                // Grab the scene: dragging right turns the
                                // camera left, so what is under the pointer
                                // travels with it; dragging down tips the view
                                // down onto the graph. Both axes are the
                                // screen's own, and neither is bounded.
                                st.camera.orbit(
                                    -dx * ORBIT_RADIANS_PER_PX,
                                    -dy * ORBIT_RADIANS_PER_PX,
                                );
                                st.flight = None;
                            }
                            DragMode::Pan => {
                                // Grab-the-scene: the point under the cursor
                                // stays under the cursor, so dragging right
                                // moves the target left.
                                st.camera_touched = true;
                                let per_px = st
                                    .camera
                                    .project(viewport, st.camera.target)
                                    .map(|s| 1.0 / s.px_per_unit.max(1e-6))
                                    .unwrap_or(1.0);
                                let basis = st.camera.basis();
                                st.camera.target -= basis.right * (dx * per_px);
                                st.camera.target += basis.up * (dy * per_px);
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
            st.pointer_inside = true;
            ticker.wake();
        });
        let function = on_move.as_ref().unchecked_ref::<js_sys::Function>().clone();
        on_move.forget();
        self.listen("pointermove", function);

        {
            // Crossing the border without moving is still an arrival: the halo
            // breath has to start from here, not from the first pointermove.
            let state = Rc::clone(&self.state);
            let ticker = self.ticker.clone();
            let on_enter = Closure::<dyn FnMut(PointerEvent)>::new(move |_event: PointerEvent| {
                state.borrow_mut().pointer_inside = true;
                ticker.wake();
            });
            let function = on_enter.as_ref().unchecked_ref::<js_sys::Function>().clone();
            on_enter.forget();
            self.listen("pointerenter", function);
        }

        let state = Rc::clone(&self.state);
        let emit = Rc::clone(&self.emit);
        let ticker = self.ticker.clone();
        let on_up = Closure::<dyn FnMut(PointerEvent)>::new(move |_event: PointerEvent| {
            // Only a left press on a node that did not turn into a drag counts
            // as a click. The background is not an event source: releasing
            // there says nothing a producer can act on.
            let click = {
                let mut guard = state.borrow_mut();
                let st: &mut State = &mut guard;
                let Some(drag) = st.drag.take() else {
                    return;
                };
                // Only a real drag changed anything: unpinning a node that was
                // never pinned and reheating a layout that nobody moved made the
                // graph jog ~11px on every click.
                if drag.moved {
                    if let DragMode::Node(index) = drag.mode {
                        if let Some(body) = st.scene.bodies.get_mut(index) {
                            body.pinned = false;
                        }
                        st.alpha = force::ALPHA_REHEAT;
                    }
                }
                if drag.moved {
                    None
                } else {
                    match drag.mode {
                        DragMode::Node(index) => Some(index),
                        DragMode::Orbit | DragMode::Pan => None,
                    }
                }
            };
            match click {
                Some(index) => {
                    // Clicking a node picks it: the halo is the view's own
                    // answer to "which one am I looking at", so it does not
                    // wait for the producer to send anything back.
                    {
                        let mut st = state.borrow_mut();
                        let already = st.scene.selected == Some(index);
                        st.scene.select(if already { None } else { Some(index) });
                    }
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
                None => {}
            }
            // The release unpins, re-cools and may have selected: one frame.
            ticker.wake();
        });
        let function = on_up.as_ref().unchecked_ref::<js_sys::Function>().clone();
        on_up.forget();
        self.listen("pointerup", function);

        // Leaving the canvas ends the hover. The card follows the pointer
        // position the view last saw, so without this a card stays standing
        // after the cursor has gone somewhere else.
        let state = Rc::clone(&self.state);
        let ticker = self.ticker.clone();
        let on_leave = Closure::<dyn FnMut(PointerEvent)>::new(move |_event: PointerEvent| {
            let mut guard = state.borrow_mut();
            let st: &mut State = &mut guard;
            st.hover = None;
            // Out of the canvas: the breath stops with the last cleared frame,
            // and a settled layout lets the loop go.
            st.pointer_inside = false;
            drop(guard);
            ticker.wake();
        });
        let function = on_leave.as_ref().unchecked_ref::<js_sys::Function>().clone();
        on_leave.forget();
        self.listen("pointerleave", function);

        let on_menu = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            event.prevent_default();
        });
        let function = on_menu.as_ref().unchecked_ref::<js_sys::Function>().clone();
        on_menu.forget();
        self.listen("contextmenu", function);

        let state = Rc::clone(&self.state);
        let ticker = self.ticker.clone();
        let on_wheel = Closure::<dyn FnMut(WheelEvent)>::new(move |event: WheelEvent| {
            event.prevent_default();
            let mut guard = state.borrow_mut();
            let st: &mut State = &mut guard;
            st.flight = None;
            st.camera_touched = true;
            let factor = (event.delta_y() as f32 * 0.001).exp();
            st.camera.distance = (st.camera.distance * factor).clamp(1.0, 100_000.0);
            drop(guard);
            ticker.wake();
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
        self.ticker.halt();
        self.ticker.tick.borrow_mut().take();
        if let Some(observer) = self.observer.take() {
            observer.disconnect();
        }
        for (name, function) in self.listeners.drain(..) {
            self.canvas
                .remove_event_listener_with_callback(name, &function)
                .ok();
        }
        for (name, function) in self.page_listeners.drain(..) {
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                document
                    .remove_event_listener_with_callback(name, &function)
                    .ok();
            }
            if let Some(window) = web_sys::window() {
                window
                    .remove_event_listener_with_callback(name, &function)
                    .ok();
            }
        }
        Renderer::lose_context(&self.canvas);
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
                    // Colour and scrim come from the theme, applied to the
                    // layer and re-applied here for labels created later.
                    let (fg, scrim) = st.style.label_colors();
                    css.set_property("color", fg).ok();
                    css.set_property("background", scrim).ok();
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
        // Screen coordinates also go into data attributes: reading them back
        // is how a headless check knows where the layout put each node without
        // parsing CSS transforms.
        let _ = label.set_attribute("data-x", &format!("{:.1}", screen.x));
        let _ = label.set_attribute("data-y", &format!("{:.1}", screen.y));
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
            let text = format!("{name}\n{kind} · weight {weight:.0}");
            if overlay.card_text != text {
                overlay.card.set_text_content(Some(&text));
                overlay.card_text = text;
                overlay.card_box = None;
            }
            let css = overlay.card.style();
            css.set_property("white-space", "pre").ok();
            css.set_property("display", "block").ok();
            // The host clips to the panel, so the card has to be placed *inside*
            // it: measure once per text (offset size is zero while hidden, hence
            // the display write above) and clamp both axes.
            let (w, h) = match overlay.card_box {
                Some(size) => size,
                None => {
                    let size = (
                        overlay.card.offset_width() as f32,
                        overlay.card.offset_height() as f32,
                    );
                    overlay.card_box = Some(size);
                    size
                }
            };
            let pad = 4.0;
            let x = (screen.x - w * 0.5).clamp(pad, (viewport.0 - w - pad).max(pad));
            // Above the node by default (it is what the labels do not cover),
            // below it when the node sits too close to the top edge.
            let y = if screen.y - h - CARD_GAP_PX >= pad {
                screen.y - h - CARD_GAP_PX
            } else {
                screen.y + CARD_GAP_PX
            };
            let _ = css
                .set_property("transform", &format!("translate({}px, {}px)", x.round(), y.round()))
                .ok();
        }
        None => {
            let _ = overlay.card.style().set_property("display", "none");
        }
    }
}