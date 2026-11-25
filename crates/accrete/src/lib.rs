#[cfg(feature = "schema")]
use schemars::JsonSchema;
#[cfg(feature = "classify")]
pub mod classify;
#[cfg(feature = "classify")]
use classify::Classify;
#[cfg(feature = "ops")]
pub mod template;
#[cfg(any(feature = "ops", feature = "classify"))]
use accrete_macro::AccreteOps;
#[cfg(any(feature = "ops", feature = "classify"))]
use accrete_macro::{ClassifyAccrete, ClassifyAttrs, ClassifyVariant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt::Debug;

#[cfg(feature = "ops")]
pub trait AccreteOps {
    fn get_type(&self) -> &str;
    fn borrow_children(&self) -> Option<&Vec<Accrete>>;
    fn borrow_children_mut(&mut self) -> Option<&mut Vec<Accrete>>;
    fn set_children(&mut self, accrete: Vec<Accrete>);
    fn borrow_attrs(&self) -> Option<&dyn Classify>;
    fn borrow_attrs_mut(&mut self) -> Option<&mut dyn Classify>;
    /// The two channels (ADR 0031 §2). A node that accepts neither answers None,
    /// which is what makes "a channel this node does not take" a frame error.
    fn get_source(&self) -> Option<&Source> {
        None
    }
    fn get_sink(&self) -> Option<&Sink> {
        None
    }
    fn get_id(&self) -> &Option<String>;
}

/// `wrap()` names the enum a variant payload becomes (the `AccreteOps` derive emits one impl per
/// variant). `WrapOps`, not `Wrap`: `Wrap` is `tiles`' line axis (ADR 0035) and a trait and an enum
/// cannot share the type namespace.
#[cfg(feature = "ops")]
pub trait WrapOps {
    type Target;
    fn wrap(self) -> Self::Target;
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub enum JsType {
    #[allow(non_camel_case_types)]
    bool,
    #[allow(non_camel_case_types)]
    number,
    #[default]
    #[allow(non_camel_case_types)]
    text,
    #[allow(non_camel_case_types)]
    password,
    #[allow(non_camel_case_types)]
    button,
    #[allow(non_camel_case_types)]
    submit,
}

impl JsType {
    pub fn input_type(&self) -> &'static str {
        match self {
            Self::bool => "checkbox",
            Self::number => "number",
            Self::text => "text",
            Self::password => "password",
            Self::button => "button",
            Self::submit => "submit",
        }
    }

    pub fn default_value(&self) -> Value {
        // JSON scalars have infallible constructors: no serializer round trip,
        // so nothing can fail for a value this module itself chose.
        match self {
            Self::number => Value::from(0),
            Self::bool => Value::Bool(false),
            _ => Value::String(String::new()),
        }
    }
}

/// Where a value is authoritative (ADR 0031 §1). The store is chosen by this
/// field, not by a separate variant: `local` lives in the page's own value
/// plane, `remote` on the side that owns the session, `both` assigns locally
/// and mirrors the change up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Remote,
    Local,
    Both,
}

impl Scope {
    /// Absent on the wire: `remote` is what every frame that says nothing means.
    fn is_remote(s: &Scope) -> bool {
        *s == Scope::Remote
    }

    /// True when the value lives in this page (ADR 0031 §1: local and both).
    pub fn local(self) -> bool {
        matches!(self, Self::Local | Self::Both)
    }

    /// True when the value reaches the far side (remote and both).
    pub fn remote(self) -> bool {
        matches!(self, Self::Remote | Self::Both)
    }
}

/// The read direction of one value (ADR 0031 §1-2, ADR 0039): where it comes from, who is authoritative,
/// which member of it, and what to show when there is none.
///
/// No `slot` means the value is a literal: `default` *is* the value (that is what the retired
/// `kind: default` said). With a slot, `default` is what to use while the slot is still empty.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    #[serde(default, skip_serializing_if = "Scope::is_remote")]
    pub scope: Scope,
    /// JSON Pointer into the slot's value (ADR 0008): the shape belongs to the
    /// producer, the read picks one field out of it. ADR 0031 is silent on
    /// extraction — it is orthogonal to the address, so it stays.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

/// The write direction of one value: where it goes, and who is authoritative about it. No `default` — a write
/// has no use for a value it does not have.
///
/// `dest` is the word ADR 0023 chose for an uplink target, and it is what an outflow calls the same address
/// (`Outflow { dest, id?, data }`), so the emit no longer translates between two names for one thing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Sink {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
    #[serde(default, skip_serializing_if = "Scope::is_remote")]
    pub scope: Scope,
    /// JSON Pointer into the target value. Only a `scope: local` write uses it: it names the member this node
    /// owns, and the rest of the object is left alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct ClassAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// The container's own layout direction (same field as FrameAttr; ADR 0010 defect fix:
    /// previously only Case knew about horizontal, so the col class of select/rack could not be
    /// overridden by its own attrs — the parent case's horizontal only governed the parent div).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct SizeAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub enum PosH {
    #[allow(non_camel_case_types)]
    left(String),
    #[allow(non_camel_case_types)]
    right(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub enum PosV {
    #[allow(non_camel_case_types)]
    top(String),
    #[allow(non_camel_case_types)]
    bottom(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct PositionAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub h: Option<PosH>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub v: Option<PosV>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub enum Direction {
    U,
    D,
    L,
    R,
}

/// An in-place mount container for an external render module (ADR 0007).
/// The core does not interpret `data` — payload semantics belong entirely to the module; size/style go through `attrs`,
/// and coupling to the data flow goes through the existing bind slot protocol (`set`/`patch` addressed by `data_event`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Canvas {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The address of the ES module on the CDN (exports the mount/update/resize/unmount contract).
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<SizeAttr>,
    /// The **dynamic bag** of module-private options (ADR 0017): renderer configuration the core
    /// does not know (3d-force-graph's node/link sizes, force parameters, colours…) packed up here,
    /// which the host hands to the module's `mount(el, data, host, options)` as it is, interpreting not one key.
    /// This is where "variants do not enter accrete" lands: swapping renderers only swaps url + options, the DSL is unchanged.
    /// Absent/Null = `{}` (the module applies its own defaults); same semantics as `url` — mount-time config,
    /// not on the update hot-reload leg (changing it takes a remount).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
    /// `bind["value"]`: the payload convention is kin to Chart/diagram — `kind: source` points at
    /// a named data slot (the raw JSON of `bind.value.default` on the node in the slot is the module's data area),
    /// streaming data in = `set`/`patch` on that slot (pointer `/bind/value/default/...`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// A 3D force-directed relationship graph (ADR 0018). A native component, not a canvas module: the renderer and the layout
/// both live in wasm (the `forcegraph` crate), so there is no `url`, no JS asset, no
/// mount/update/resize/unmount contract — all that can be written is `attrs`, the renderer knobs `style`
/// and the data area `bind["value"]`.
///
/// Data area payload = `forcegraph::Snapshot`: `{nodes, links, focus?}`, diffed by id
/// (re-sending the whole snapshot is idempotent). `focus` is a view instruction
/// `{ids, seq?, padding_px?, duration_ms?}` — changing `seq` lets the same target fly again,
/// while a repeat write that leaves `seq` alone does not touch the camera.
///
/// **This is a standalone component, not a kind of canvas** (ADR 0029): rendering, layout, camera and picking are all in
/// the same wasm, so it has its own node type and its own `style` vocabulary (typed, so it can be checked by schema
/// and by patch), and does not take that url + options dynamic bag. The graph data is this interface's main reading surface,
/// and customisation grows by "adding one more dimension": the payload is a snapshot, adding fields does not swap the node type — grouping displays like
/// `node_cluster` / `edge_cluster` will appear as new fields of `Snapshot` and new knobs of `style`,
/// rather than becoming another component or swapping the renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct ForceGraph {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<SizeAttr>,
    /// Renderer knobs, all with defaults; when default they stay off the wire.
    #[serde(default, skip_serializing_if = "force_graph_style_is_default")]
    pub style: ForceGraphStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,

    /// Second value: the theme channel (`true`/`"dark"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Source>,
}

/// The graph renderer's knobs. Same semantics as canvas's `options`: mount-time config, changing it takes
/// a remount (not on the update leg). The only difference is that this is our own renderer's vocabulary, so it can be
/// typed and checked by schema and by patch.
/// The direction of an edge. Both orientations are "interfaces on a node": a link leaves from one orientation and enters
/// the other end's orientation, joined in between by a cubic curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LinkShape {
    #[default]
    Straight,
    /// Cartesian coordinates: the six directions ±x ±y ±z.
    Axes,
    /// Regular tetrahedron: the directions from the four vertices to the centre.
    Tetra,
}

/// The ground colour the graph sits on. The two palettes differ only in lightness: Morandi colours are low-chroma, so a palette with
/// enough contrast on a light ground will smear into a blur on a dark one, and vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    /// Default: nodes are orbs of light and edges are rays — light only works on a dark ground.
    #[default]
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
// Struct-level default: a frame writes only the knobs it wants to change, the rest take `Default::default()`.
// Field-level default only covers "the whole style is absent"; writing half an object reports a missing field.
#[serde(default)]
pub struct ForceGraphStyle {
    /// Node base radius (screen pixels).
    /// Node radius (screen pixels); 0 = derived from `node_size_ratio`.
    pub node_size_px: f32,
    /// Multiple of the link width for the node sphere diameter (default 2).
    pub node_size_ratio: f32,
    /// Radius added per weight tier (screen pixels); the tier count is `node_tiers`.
    pub node_size_step_px: f32,
    pub node_tiers: u8,
    pub link_width_px: f32,
    /// Edge direction: `straight` connects directly; `axes` takes the nearest of the six coordinate orientations;
    /// `tetra` takes the nearest of the regular tetrahedron's four vertex directions.
    pub link_shape: LinkShape,
    pub link_opacity: f32,
    /// Repulsion; a negative value = pushing each other apart.
    pub charge: f32,
    pub link_distance: f32,
    pub link_strength: Option<f32>,
    pub velocity_decay: f32,
    /// An always-on name label layer.
    pub labels: bool,
    pub label_px: f32,
    /// Above this node count draw only, no labels (the label layer is DOM projected per frame, so at volume it is noise).
    pub label_max_nodes: u32,
    /// After the first batch of data, fit the view to the whole graph; later appends neither re-layout nor fly the camera.
    pub auto_fit: bool,
    pub fit_padding_px: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Node colours, taken in rotation by `kind`; empty = the built-in Morandi scheme for `theme`.
    #[serde(default)]
    pub palette: Vec<String>,
    /// Ground: light / dark. Swaps only palette and panel background, not the layout.
    pub theme: Theme,
    /// Marker strength for highlighted nodes: peak opacity of the halo core and of each ripple ring.
    pub glow_alpha: f32,
    /// Highlight the reachable path (`reach_hops` edges out from the selected/focused node) instead of dimming the rest.
    pub highlight_reach: bool,
    /// The number of reachable hops.
    pub reach_hops: u32,
}

impl Default for ForceGraphStyle {
    fn default() -> Self {
        Self {
            node_size_px: 0.0,
            node_size_ratio: 2.0,
            node_size_step_px: 0.6,
            node_tiers: 3,
            // 2.5 px / 0.6 alpha: at 1.5 px and 0.35 the edges were drawn but
            // invisible on a light page (measured: 540 → 2486 coloured pixels
            // when the width went to 16 px).
            link_width_px: 2.5,
            link_shape: LinkShape::Straight,
            link_opacity: 0.8,
            charge: -30.0,
            link_distance: 30.0,
            link_strength: None,
            velocity_decay: 0.4,
            labels: true,
            label_px: 12.0,
            label_max_nodes: 150,
            auto_fit: true,
            fit_padding_px: 40.0,
            background: None,
            palette: Vec::new(),
            theme: Theme::default(),
            glow_alpha: 0.45,
            highlight_reach: true,
            reach_hops: 1,
        }
    }
}

fn force_graph_style_is_default(style: &ForceGraphStyle) -> bool {
    *style == ForceGraphStyle::default()
}

/// How a panel sits on the page — the first of the two axes that used to be three components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum PanelPlace {
    /// In the flow: opening it pushes the rest of the page down. (Was `fold`, `float_body: false`.)
    #[default]
    Inline,
    /// Over the trigger it belongs to: the page around it does not move. (Was `popup`.)
    Anchor,
    /// Over the whole viewport, at the named edge, with a mask behind it. (Was `layer`; a panel at
    /// the right edge is the drawer that had no spelling before.)
    Viewport,
}

/// Who decides whether a panel is on screen — the second axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum PanelState {
    /// A local toggle: the trigger opens it, a click on the mask / Esc / outside closes it. Nothing
    /// about it reaches the sender.
    #[default]
    Toggle,
    /// The panel follows its binding: non-empty means on screen (ADR 0012's "empty is absent"),
    /// and dismissing clears that slot — one state, not two. Named `bound`, not `source`: the
    /// channel `source` says where a VALUE comes from, this says who decides the panel is there.
    Bound,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct PanelAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Where it sits. Absent = `inline`.
    #[serde(default, skip_serializing_if = "is_default_place")]
    pub place: PanelPlace,
    /// Who decides. Absent = `toggle`.
    #[serde(default, skip_serializing_if = "is_default_state")]
    pub state: PanelState,
    /// `viewport` only: which edge the body hugs. Absent = `L` — the left edge of the viewport
    /// (`Direction` is the same enum `popup` positions with).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    /// Closes on a click outside the body — the mask for `viewport`, "anywhere else" for the rest.
    /// A panel without it can only be closed by its trigger (a loom that must not lose its work).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dismiss: Option<bool>,
    /// `inline` only: the first child is the header and toggles the body (the old `fold` shape).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_header: Option<bool>,
    /// `viewport` only: the body's width. Absent = whatever the class says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    /// `viewport` only: where on the page it sits (`right: 1rem, top: 1rem`, …). Absent = the whole
    /// viewport, with the body centred and a mask behind it. This is what `float` used to be.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub pos: Option<PositionAttr>,
}

fn is_default_place(p: &PanelPlace) -> bool {
    *p == PanelPlace::Inline
}

fn is_default_state(s: &PanelState) -> bool {
    *s == PanelState::Toggle
}

/// A panel: one "appear/disappear content area". `attrs.place` says where it goes (inline / hugging the trigger / hugging the viewport),
/// `attrs.state` says who decides it appears (local toggle / following the bound slot). The four cells on these two axes are the old
/// `fold`, `popup`, `layer`, plus the one cell they did not cover — hugging the viewport + local toggle (the drawer).
///
/// `children[0]` is the trigger, `children[1]` is the floating content (with `state: bound` the trigger may be omitted: the panel's
/// appearance is decided by the slot, nobody has to click). With no content nothing is drawn at all — omitting a child is a frame error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Panel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<PanelAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

/// When a loom sends (ADR 0032). The modes combine, so the attribute is a
/// set: `[enter, button]` is a chat box, `[change]` packs as the user types.
/// This is timing — a different axis from `scope`, which says where a value
/// lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum SubmitMode {
    #[default]
    Enter,
    Button,
    Change,
}

/// A bare string or a list on the wire: `submit: enter` and `submit: [enter]`
/// say the same thing, so the one-mode case (nearly every frame) does not have
/// to remember the list syntax.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum SubmitSet {
    One(SubmitMode),
    Many(Vec<SubmitMode>),
}

impl Default for SubmitSet {
    /// `[enter]` — the shape a loom has when it says nothing.
    fn default() -> Self {
        Self::Many(vec![SubmitMode::Enter])
    }
}

impl SubmitSet {
    pub fn modes(&self) -> &[SubmitMode] {
        match self {
            Self::One(m) => std::slice::from_ref(m),
            Self::Many(v) => v,
        }
    }

    pub fn has(&self, mode: SubmitMode) -> bool {
        self.modes().contains(&mode)
    }
}

/// How a field's change reaches the loom (ADR 0032 §8). The keystroke updates the field's own copy at
/// once — what this decides is when the *loom* hears about it, which is what `submit: [change]` turns
/// into frames. Closed, and one arm at a time: debounce and throttle answer different questions (wait
/// for quiet, or keep a cadence), and a frame asking for both is asking for a shape nobody wants.
///
/// Absent means `{debounce: 250}`: typing stays instant (the display is the field's own signal), while
/// a per-keystroke `[change]` submit becomes one frame per pause instead of one frame per letter —
/// without it a `sends: 1` loom spends its only send on the first character. `{debounce: 0}` is the
/// way back to "every keystroke, at once".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum Emit {
    Debounce(Debounce),
    Throttle(Throttle),
}

impl Default for Emit {
    /// `{debounce: 250}` — what a field does when it says nothing.
    fn default() -> Self {
        Self::Debounce(Debounce { debounce: 250 })
    }
}

impl Emit {
    /// The window in milliseconds. `0` means "now": debounce then notifies on the keystroke itself,
    /// throttle then has no window to speak of.
    pub fn millis(self) -> u32 {
        match self {
            Self::Debounce(d) => d.debounce,
            Self::Throttle(t) => t.throttle,
        }
    }

    pub fn is_throttle(self) -> bool {
        matches!(self, Self::Throttle(_))
    }

    /// The change reaches the loom on the event itself. Not a wire default — it is what a node whose
    /// change is already discreet (a `select` option click) uses, so it never inherits the field's
    /// 250 ms debounce.
    pub const IMMEDIATE: Emit = Emit::Debounce(Debounce { debounce: 0 });
}

/// `{debounce: ms}` — the change reaches the loom once the field has been quiet that long, carrying the
/// last value. The right shape for a search box: the reader stops, the frame leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Debounce {
    pub debounce: u32,
}

/// `{throttle: ms}` — the first change goes at once and the rest coalesce to at most one per window,
/// the window's last value landing at its end. The right shape for a live preview that must keep
/// moving while the reader types, without a frame per letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Throttle {
    pub throttle: u32,
}

fn submit_is_default(s: &SubmitSet) -> bool {
    s.modes() == [SubmitMode::Enter]
}

/// What a loom's fields become after it sends (ADR 0032 §4). One word, three exclusive values:
/// `keep` (absent — what a field always did), `clear` (the empty value of its own shape) and
/// `default` (the value `source.default` gave it at build time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ResetMode {
    #[default]
    Keep,
    Clear,
    Default,
}

fn reset_is_default(r: &ResetMode) -> bool {
    matches!(r, ResetMode::Keep)
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct LoomAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// When to send. Absent = `[enter]`.
    #[serde(default, skip_serializing_if = "submit_is_default")]
    pub submit: SubmitSet,
    /// What the fields become after a send. Absent = `keep`.
    #[serde(default, skip_serializing_if = "reset_is_default")]
    pub reset: ResetMode,
    /// How many sends this loom has: absent or `-1` = unlimited, `0` = locked from the start. Each send
    /// takes one; at zero the loom's fields are disabled and it refuses to send (ADR 0032 §5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sends: Option<i64>,
}

/// A loom packs a group of fields into one object and sends it (ADR 0032).
/// It owns the exit for its fields (`sink`), when they leave (`attrs.submit`), what they become
/// afterwards (`attrs.reset`) and how many times they may (`attrs.sends`) — the field itself owns
/// nothing but its value. `id` marks this submission (the instance), not the fields: fields are
/// named by their own `tag`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Loom {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The key this loom contributes to its parent's packed object — the same `tag` rule a field follows,
    /// because a nested loom is an input of the enclosing one. Only read when the loom sits inside another
    /// loom; with no `tag` it packs nothing into its parent (`id` is identity, `dest` is where it sends, and
    /// neither is a name — ADR 0032 §4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<LoomAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink: Option<Sink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Svg {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<SizeAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Path {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// How much of the slot a `tiles` node puts on stage at once (ADR 0031 §4). Absent = every item is
/// laid out, the way the frame's CSS says; `{size: 1}` = one at a time, occupying the whole area —
/// the marquee, and also the page; `{size: N}` = a moving window of N consecutive items, which is
/// what a paged table needs (and what `rack.view.count` used to be). Nothing about the container
/// changes between the three: a window is a count, not a mode.
///
/// There is no default: a frame that says nothing has no window at all, and a window of zero would
/// be a box that can never show anything, so it is refused at decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Window {
    #[serde(deserialize_with = "size_at_least_one")]
    #[cfg_attr(feature = "schema", schemars(range(min = 1)))]
    pub size: usize,
}

/// A window narrower than one item is not a window: it would render an area that can never show
/// anything the frame asked for.
fn size_at_least_one<'de, D: serde::Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
    use serde::de::Error;
    match usize::deserialize(d)? {
        0 => Err(D::Error::custom(
            "window size is at least 1; a zero-width window shows nothing",
        )),
        n => Ok(n),
    }
}

/// What advances the stage (ADR 0031 §4). Exactly two answers, because there are exactly two ways
/// an item gets there: a clock this node owns, or a slot the world writes. Omitted = no stepping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum Step {
    /// `{timer: 4000}` — ms between turns. One clock per node, stopped when the node goes away:
    /// rotation, dwell and "newest on top" are all this.
    Timer { timer: u64 },
    /// `{slot: "page"}` — the value of that slot says which item is on stage. What a `pages` node
    /// was: a selection driven from outside (a nav click, a route, another component).
    Slot { slot: String },
}

/// The direction a `tiles`' ITEMS flow in (ADR 0035) — flex's meaning, not "the shape of an entry":
/// `row` lays the items out left to right, `row-reverse` right to left, `col` top to bottom,
/// `col-reverse` bottom to top.
///
/// The words are flex's because a frame author already knows them, down to the `-reverse` spellings
/// that reverse the ORDER without changing the axis. The default is `col`, the one place this
/// vocabulary deliberately differs from flex: flex lays out inline content and starts on a line,
/// while `tiles` lays out a list of items — and an unlabelled list is vertical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum Dir {
    Row,
    RowReverse,
    #[default]
    Col,
    ColReverse,
}

/// What the items do when they no longer fit on one line (ADR 0035). `wrap` is absent: a wall of
/// cards is the common shape, and `nowrap` is a strip that must stay one strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum Wrap {
    #[default]
    Wrap,
    WrapReverse,
    /// `nowrap` is a word, not two: the CSS spelling, and one serde cannot derive from `NoWrap`.
    #[serde(rename = "nowrap")]
    NoWrap,
}

/// One container for a slot's items, cut into blocks (ADR 0031 §4): `rack` and `pages` were this
/// node with different steppers. `children` is the layout a tile is drawn with — a selector picks
/// one template, the rest of the list is the default (the mechanism `rack.item` used to carry).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Tiles {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<TilesAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct TilesAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Which way the ITEMS flow (ADR 0035), in flex's sense. `row` is a strip: items left to right,
    /// and each entry's fields run down it, in the grid's ROW axis. `col` (default) is a list: items
    /// top to bottom, each entry's fields across it, in the grid's COLUMN axis — the table every
    /// frame already draws. `row-reverse` / `col-reverse` keep the shape and reverse the order the
    /// items arrive in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<Dir>,
    /// What happens when the items no longer fit on one line (ADR 0035). Default `wrap`, because
    /// a card wall is the shape that has no other spelling. `wrap-reverse` also puts the FIELDS on
    /// the other side of the item axis: the header row below the entries, or the header column on
    /// the right — the fields axis is the line axis, so reversing the lines reverses where they land.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<Wrap>,
    /// Absent = every item is laid out and nothing steers; present = that many consecutive items
    /// are on stage at once, starting at the cursor (ADR 0031 §4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<Window>,
    /// What advances the window. Only meaningful with a `window`: a stepper with nothing to step is
    /// a frame error at load, not a clock that ticks for nobody.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<Step>,
    /// Absent = each item is a node drawn as it is, one block per item. Present = each item is an
    /// ENTRY and each field projects one of its children into a cell (ADR 0031 §4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<Field>>,
    /// Absent = no header row. The cells carry `selector`, so the frame styles its own header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<Header>,
}

/// One field of a grid (ADR 0031 §4). `key` names the entry field this field projects — the
/// `attrs.selector` of one of the entry's children — and it is also what a two-row header derives
/// its group from: split on `/`, everything before the last one is the group and the rest is the leaf
/// (`price/retail` sits under `price`). Formatting has no spelling yet, so it is not implemented.
///
/// A FIELD, not a column: `column` names where a thing sits, and where it sits is `dir`'s business.
/// `tiles` has a direction of its own (`row | row-reverse | col | col-reverse`, ADR 0035), and the
/// fields run across it with `dir: col` and down it with `dir: row` — so a word that names one of
/// the two would be a lie half the time. `field` names the thing itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct Field {
    pub key: String,
    /// The header text; absent falls back to the key's leaf.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A CSS track size (`12rem`, `minmax(0, 2fr)`); absent is `minmax(0, 1fr)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    /// Extra class tokens for this field's cells: a field is not an element, so it is addressed
    /// the way the rest of the renderer addresses things (ADR 0031 §4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selectors: Option<Vec<String>>,
}

/// Where a cell's content sits inside its field. `start` is what a text cell already does, so it is
/// the default and stays off the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

/// The header of a grid (ADR 0031 §4, ADR 0035): the cells carry `selector` — one hook for the whole
/// header, since no header cell is a special element. `layers: 2` puts a group layer over a leaf
/// layer, both read off the fields' keys; there is no second shape to say the same thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct Header {
    pub selector: String,
    /// `1` (default) or `2`: how many LINES of header cells there are along the field axis. A
    /// direction word (`rows`) named where the header sat, which stopped being true the moment
    /// `dir: row` made the header a column; a layer is one line whatever axis it runs on.
    #[serde(
        default = "layers_default",
        skip_serializing_if = "layers_is_one",
        deserialize_with = "layers_1_or_2"
    )]
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 2)))]
    pub layers: u8,
}

/// Absent `layers` is one layer, not zero: `default` alone would hand back `0` and the node would
/// claim a header with no cells.
fn layers_default() -> u8 {
    1
}

fn layers_is_one(layers: &u8) -> bool {
    *layers <= 1
}

/// `layers` is a closed set of two answers (ADR 0031 §4). A third value would otherwise render as
/// something nobody chose, so it is refused where every other shape is checked: at decode.
fn layers_1_or_2<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    use serde::de::Error;
    match u8::deserialize(d)? {
        n @ (1 | 2) => Ok(n),
        other => Err(D::Error::custom(format!(
            "header layers is 1 or 2; got {other}"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct ButtonAttr {
    #[serde(default)]
    pub oneshot: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Button {
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ButtonAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct ImageAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    #[serde(default)]
    pub thumb: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Image {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ImageAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// A field: one value, one line (or many, with `multiline`) (ADR 0032).
///
/// Unknown keys are refused: a field has no `sink` any more, and an old page that still carries one should
/// hear about it rather than quietly lose the write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub id: Option<String>,
    /// The field's name inside an enclosing loom: the loom packs this tag
    /// as the key, so the sink's address no longer names the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// What kind of field this is — the element type and the parse, one declaration (ADR 0032 §2: a field
    /// has one type, and it is not the read's or the write's). Named `kind` because `type` is the node's own
    /// discriminator on the wire. Absent = text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<JsType>,

    /// Multi-line is a layout of the field, not a different node.
    #[serde(default, skip_serializing_if = "is_false")]
    pub multiline: bool,
    /// Second value: the placeholder text (literal or slot) — the channel
    /// `textarea` used to carry on its own type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<Source>,
    /// When this field's typing reaches the loom (ADR 0032 §8). Absent = `{debounce: 250}`, which is
    /// what makes a per-keystroke `submit: [change]` bearable; `{debounce: 0}` is a frame per letter.
    /// Timing, not addressing: `source`/`tag` say where the value lives, this says when it is heard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit: Option<Emit>,
}

/// A disc in the `oklch` plane (the field-shaped picker). Two of the three terms are on the face — the
/// angle is the hue, the radius is the chroma (the centre is grey) — and the third is pinned by `attrs`,
/// because a flat face has no room for a third axis. What it packs is `"<light> <chroma> <hue>"`: a
/// written-out `L C H` triple, the same spelling colour members already take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
#[serde(deny_unknown_fields)]
pub struct Disc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The field's name inside an enclosing loom (same rule as `input`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<DiscAttr>,
    /// The current value, in the triple spelling above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// What the disc does not show: lightness, and the usual attribute surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct DiscAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Lightness the disc keeps (0–1). The face is drawn at this lightness, so the disc is a slice of the
    /// space, not the whole of it.
    pub light: f64,
}

impl Input {
    /// The field's timing, with the default filled in — the renderer's entry point.
    pub fn emission(&self) -> Emit {
        self.emit.unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
#[serde(deny_unknown_fields)]
pub struct Select {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The field's name inside an enclosing loom (same rule as `input`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,

    /// Second value: the option rows (list plane).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

/// An entry: a group of named data fields, as one item of a list — each field a child node whose
/// `attrs.selector` is the field's name (ADR 0031 §4 — an entry is the item of a `tiles` that has
/// `fields`).
///
/// A field whose direct children are themselves entries holds a LIST of entries, which a grid draws
/// as child rows under the row it belongs to. That is the whole of "hierarchy instead of spans": the
/// data already says which row a row belongs to, so the markup does not merge cells to say it again.
///
/// An ENTRY, not a record: `record` implies a row, and a row is a direction word. `tiles` runs its
/// items along whichever axis `dir` names (`row | row-reverse | col | col-reverse`), and on the
/// horizontal one the items flow across a row — where "record" (a row's worth) would name the
/// container, not the thing. `entry` names the thing itself: what a list holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Entry {
    /// The item's key (ADR 0031 §4, "Every item has a key"): an entry that names itself keeps that
    /// name across removals and re-sorts; one that does not is addressed by where it arrived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    /// The fields. A child's `attrs.selector` is the field's name, so a field projects by that
    /// key rather than by position — reordering fields cannot move a value to another field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct TextAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Text {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<TextAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// A frame is a box: it wraps children, it can read a slot, and (ADR 0034) it is the
/// only thing that can carry cargo — as a source (`draggable`), as a landing
/// (`accept` + `sink`), or as both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct FrameAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<bool>,
    #[allow(non_camel_case_types)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<Map<String, Value>>,
    /// Inline style, written as it is. This is the surface `group` had: a frame can
    /// say everything a `group` could.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<HashMap<String, String>>,
    /// This box may be dragged away (ADR 0034). Its cargo is its own subtree — there
    /// is no second payload to keep in step with it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draggable: Option<Draggable>,
    /// What cargo this box takes (ADR 0034). Absent `tag` = takes any. Needs `sink`:
    /// a drop that cannot emit is a frame error at load.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept: Option<Accept>,
}

/// The tag of a draggable box's cargo (ADR 0034). Absent = untagged: a target that
/// names no tag takes it, a target that names one does not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Draggable {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

/// Which cargo a box takes (ADR 0034): absent `tag` = any, a tag = that one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Accept {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Frame {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<FrameAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// The landing uplink (ADR 0034): where a drop on this box goes — an event name or
    /// a local slot, the same channel `dock` had.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink: Option<Sink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

/// A registered template, instantiated by name (ADR 0006). Renamed from `Template`
/// (ADR 0034); the definition side is unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Stamp {
    name: String,
    data: Map<String, Value>,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyVariant))]
#[serde(tag = "type")]
pub enum Accrete {
    /// A box (ADR 0027/0034). It reads a slot and it is the only place cargo lives:
    /// `attrs.draggable` makes it a drag source, `attrs.accept` + `sink` a landing.
    /// It takes a fallback id because a landing emits, and the frame's id is the id
    /// of what was dropped on.
    #[ui_acrete(has_id = "true")]
    frame(Frame),
    #[ui_acrete(has_id = "true")]
    canvas(Canvas),
    #[ui_acrete(has_id = "true")]
    forcegraph(ForceGraph),
    /// A content area: the four cells of the two axes `place` × `state` (ADR 0030). `fold`/`popup`/`layer` as node types
    /// are gone, along with their shells in the model: all four cells are implemented in ui_core's `components/panel.rs`,
    /// and this arm is the single name they share.
    #[ui_acrete(has_id = "true")]
    panel(Panel),
    loom(Loom),
    svg(Svg),
    path(Path),
    /// One container for a slot's items (ADR 0031 §4): `window` says how many are on it, `step`
    /// what advances it. `rack` and `pages` were the same node with different steppers.
    #[ui_acrete(has_id = "true")]
    tiles(Tiles),
    /// An entry — the item a `tiles` with `fields` projects (ADR 0031 §4). Rendered on its own it
    /// is a block of its fields; the grid is what turns it into a row.
    entry(Entry),
    button(Button),
    image(Image),
    input(Input),
    select(Select),
    /// A disc in the `oklch` plane: a field whose angle is the hue and whose radius is the chroma. One term
    /// (`attrs.light`) is pinned, so what it packs is the whole `L C H` triple a colour token reads — the
    /// picker and the value stay one thing, and nothing downstream has to know that a disc was involved.
    disc(Disc),
    text(Text),
    /// A registered template by name (ADR 0006), spelled `stamp` since ADR 0034.
    stamp(Stamp),
}

#[cfg(feature = "ops")]
impl Accrete {
    pub fn cmp_id(&self, other: &Self) -> bool {
        let Some(id) = self.get_id() else {
            return false;
        };
        let Some(oid) = other.get_id() else {
            return false;
        };
        id == oid
    }
}

impl Accrete {
    /// Read-only JSON Pointer walk over the WIRE shape (ADR 0008, the
    /// `Source.path` extraction). Same segment vocabulary as the patch
    /// path — one syntax for both directions.
    pub fn get_at(&self, path: &str) -> Result<Value, String> {
        let segs = Self::pointer_segments(path)?;
        let mut cur = serde_json::to_value(self).map_err(|e| e.to_string())?;
        for seg in segs {
            cur = match cur {
                Value::Object(map) => map
                    .get(seg.as_str())
                    .cloned()
                    .ok_or_else(|| format!("pointer miss: {path:?} (no key {seg:?})"))?,
                Value::Array(arr) => {
                    let idx: usize = seg
                        .parse()
                        .map_err(|_| format!("bad array index {seg:?} at {path:?}"))?;
                    arr.into_iter()
                        .nth(idx)
                        .ok_or_else(|| format!("pointer miss: {path:?} (no index {idx})"))?
                }
                _ => return Err(format!("pointer miss: {path:?} (hit non-container)")),
            };
        }
        Ok(cur)
    }

    /// JSON Pointer (RFC 6901) segments, with `~1`/`~0` unescaping.
    fn pointer_segments(path: &str) -> Result<Vec<String>, String> {
        if !path.starts_with('/') {
            return Err(format!("pointer must start with '/': {path:?}"));
        }
        Ok(path[1..]
            .split('/')
            .map(|s| s.replace("~1", "/").replace("~0", "~"))
            .collect())
    }

    /// Apply `f` to the value at `path`, then re-validate the whole node.
    /// The pointer works on the WIRE shape (serde field names) — the same
    /// vocabulary external producers see, no second path syntax (ADR 0005).
    /// On any miss, type violation, or invalid result the node is untouched
    /// (atomicity comes from the type system, not hand-written setters).
    fn with_pointer(
        &mut self,
        path: &str,
        f: impl FnOnce(Value) -> Option<Value>,
    ) -> Result<(), String> {
        let segs = Self::pointer_segments(path)?;
        let mut root = serde_json::to_value(&*self).map_err(|e| e.to_string())?;

        // Descend to the parent container (all but the last segment).
        let mut cur = &mut root;
        for seg in &segs[..segs.len().saturating_sub(1)] {
            cur = match cur {
                Value::Object(map) => map
                    .get_mut(seg.as_str())
                    .ok_or_else(|| format!("pointer miss: {path:?} (no key {seg:?})"))?,
                Value::Array(arr) => {
                    let idx: usize = seg
                        .parse()
                        .map_err(|_| format!("bad array index {seg:?} at {path:?}"))?;
                    arr.get_mut(idx)
                        .ok_or_else(|| format!("pointer miss: {path:?} (no index {idx})"))?
                }
                _ => return Err(format!("pointer miss: {path:?} (hit non-container)")),
            };
        }

        // Last segment: apply f. Note RFC 6901 makes "/" the member keyed
        // "" (not the node itself), which the Object arm below handles.
        let Some(seg) = segs.last() else {
            return Err(format!("pointer has no segment to write: {path:?}"));
        };
        match cur {
            Value::Object(map) => {
                let old = map
                    .remove(seg.as_str())
                    .ok_or_else(|| format!("pointer miss: {path:?} (no key {seg:?})"))?;
                let new = f(old).ok_or_else(|| format!("unsupported append at {path:?}"))?;
                map.insert(seg.clone(), new);
            }
            Value::Array(arr) => {
                let idx: usize = seg
                    .parse()
                    .map_err(|_| format!("bad array index {seg:?} at {path:?}"))?;
                let old = arr
                    .get_mut(idx)
                    .ok_or_else(|| format!("pointer miss: {path:?} (no index {idx})"))?;
                let take = std::mem::replace(old, Value::Null);
                let new = f(take).ok_or_else(|| format!("unsupported append at {path:?}"))?;
                *old = new;
            }
            _ => return Err(format!("pointer miss: {path:?} (hit non-container)")),
        }

        let patched: Accrete =
            serde_json::from_value(root).map_err(|e| format!("patch rejected: {e}"))?;
        *self = patched;
        Ok(())
    }

    /// Replace the value at a JSON Pointer path (ADR 0005 `op: replace`).
    pub fn replace_at(&mut self, path: &str, value: Value) -> Result<(), String> {
        self.with_pointer(path, |_| Some(value))
    }

    /// Extend the value at a JSON Pointer path (ADR 0005 `op: append`):
    /// string concatenation or array push. Other shapes are an error —
    /// the streaming-accumulator job of the retired positional merge.
    pub fn append_at(&mut self, path: &str, value: Value) -> Result<(), String> {
        self.with_pointer(path, |mut old| match old {
            Value::String(ref mut s) => match value {
                Value::String(t) => {
                    s.push_str(&t);
                    Some(Value::String(std::mem::take(s)))
                }
                other => Some(Value::String(other.to_string())),
            },
            Value::Array(mut a) => {
                a.push(value);
                Some(Value::Array(a))
            }
            Value::Null => Some(value),
            _ => None,
        })
    }
}

/*
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(tag = "type")]
pub enum JsonSvgAccrete {
    group,
    path,
}
*/
