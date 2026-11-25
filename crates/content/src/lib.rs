pub mod codec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A downlink frame IS the **action list** itself (ADR 0021 decision 6): there is no envelope.
/// If the host needs its own `ev` / `sender` / `created`, it carries them in the host's own frame — the three hosts
/// (stage / prism / reflux) each have their own, and they align only at the `Content` layer.

/// The two dest names the transport owns (ADR 0044). They are not app events: an uplink `__ping__` is
/// answered with a downlink `__pong__` by the host, and neither ever reaches a handler or a slot. The
/// `__`-wrapped spelling is what keeps a page event from colliding with them by accident.
pub const PING_DEST: &str = "__ping__";
pub const PONG_DEST: &str = "__pong__";

/// Whether a `dest` is the transport's own (and therefore never app traffic).
pub fn is_reserved(dest: &str) -> bool {
    dest == PING_DEST || dest == PONG_DEST
}

/// Whether an **encoded** frame is one of those. Both codecs are tried, because the host answers in the
/// encoding it likes and the page must not care: a JSON pong on a CBOR page is still a pong (ADR 0044).
/// The byte scan is only a prefilter — a frame without the marker costs no decode at all, and a frame that
/// merely *contains* the word is decoded and then judged by its `dest`.
pub fn is_transport_frame(bytes: &[u8]) -> bool {
    let marked = bytes.windows(4).any(|w| w == b"__pi" || w == b"__po");
    if !marked {
        return false;
    }
    [codec::ActiveCodec::Json, codec::ActiveCodec::Cbor]
        .into_iter()
        .any(|c| c.decode::<Outflow>(bytes).map(|o| is_reserved(&o.dest)).unwrap_or(false))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Outflow {
    /// Uplink target: which target on the host the page sends this interaction to (ADR 0023). Not the same name as the downlink `slot` —
    /// uplink and downlink are not symmetric (one writes a position in the page, one invokes a host handler). `event` is not on this line.
    pub dest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "action")]
pub enum Content<T> {
    #[serde(rename = "create")]
    Create(Influx<T>),

    #[serde(rename = "tmpl")]
    Tmpl(InfluxTmpl),

    #[serde(rename = "set")]
    Set(Influx<T>),

    /// Append a row to the list slot `event` (ADR 0005, ex-join minus its
    /// merge role). Row identity lives in the data (`data.id`); the renderer
    /// rejects duplicate row ids.
    #[serde(rename = "append")]
    Append(AppendOp<T>),

    /// Remove the row `id` from the list slot `event`.
    #[serde(rename = "remove")]
    Remove(RemoveOp),

    /// Move the row `id` — which is already in the list — to another place in the same list
    /// (ADR 0005). The row is not rebuilt: the list's order changes, and the renderer's keyed
    /// diff reuses the node, so its focus, its animation and its scroll position survive the
    /// move. A row that is not there is refused, the same boundary `append` draws against a
    /// duplicate id; there is no `move` that creates a row.
    #[serde(rename = "move")]
    Move(MoveOp),

    /// Value-domain update addressed by JSON Pointer (ADR 0005).
    /// Plane: `slot == ""` → layout root; `id` set → that list row;
    /// otherwise → the named data slot.
    #[serde(rename = "patch")]
    Patch(PatchOp),

    #[serde(rename = "empty")]
    #[default]
    Empty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AppendOp<T> {
    /// The list slot this row lands in — named `slot`, not `event`: the wire has
    /// exactly one meaning per word, and an *event* is the uplink's name
    /// (`Outflow.event`, `bind.*.event`), never an address inside the tree.
    pub slot: String,
    /// The landing spot (ADR 0005). 0 = the end (the default, i.e. an append); a positive number counts inward from the **tail** (+1 = before the last one,
    /// +len = the head); a negative number counts inward from the **head** (-1 = the head). Out of domain is rejected by the renderer, see `append_index`.
    ///
    /// The direction is deliberate: indices always count inward from this operation's own anchor end — append is anchored at the tail, so positive numbers walk toward the head;
    /// an insert anchored at the head has the opposite sign.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub position: i64,
    pub data: T,
}

/// Absent means 0 (the end): when the position carries no information the field is not written into the frame — `position: 0` is redundant with the default.
fn is_zero(n: &i64) -> bool {
    *n == 0
}

/// Which index in the list a row lands at: `len` is the current row count, `position` is the number on the wire.
///
/// | position | landing spot |
/// |---|---|
/// | `0` | the end (append, default) |
/// | `+n` (`1..=len`) | before the n-th position counting inward from the tail (`+1` = before the last one, `+len` = the head) |
/// | `-n` (`1..=len`) | before the n-th position counting inward from the head (`-1` = the head, `-len` = before the last one) |
///
/// Out of domain returns `None`: the caller rejects the operation and counts it, it does **not** clamp — clamping would put the row where the caller did not ask
/// for, and that is a silent downgrade. Note that it is off by one from the familiar `Vec::insert(i)` / `list.insert(i)`: there the
/// `i` is an array index, while the number here is the n-th position counted from the anchor end.
pub fn append_index(len: usize, position: i64) -> Option<usize> {
    match position {
        0 => Some(len),
        n if n > 0 => usize::try_from(n).ok().and_then(|k| len.checked_sub(k)),
        n => n
            .checked_neg()
            .and_then(|k| usize::try_from(k).ok())
            .and_then(|k| k.checked_sub(1))
            .filter(|spot| *spot < len),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RemoveOp {
    pub slot: String,
    pub id: String,
}

/// One row's new place in a list it is already in (ADR 0005).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MoveOp {
    pub slot: String,
    /// The row: its `data.id`. A row with no id cannot be moved — there is nothing to name.
    pub id: String,
    /// The landing spot, in `append`'s vocabulary: `0` = the end, a positive number counts inward
    /// from the tail, a negative one inward from the head. The count is taken on the list **with
    /// the row lifted out** — otherwise "counted before the row came out" and "counted after"
    /// would both be defensible, and the implementation and the reader would disagree. That is one
    /// position fewer than `append` has, which sees the list as it is; see `move_index`.
    ///
    /// Always written, `0` included: for `append` the default *is* the end, while here the end is
    /// one of many places a row can be sent, so leaving it out would read as "no opinion" where the
    /// operation has nothing else to say.
    pub position: i64,
}

/// Which index a row lands at when it is moved: `len` is the list's row count **with the row still
/// in it** (so the row must exist: `len >= 1`), and `position` is the number on the wire.
///
/// The row is lifted out first, so the count runs over `len - 1` positions — one fewer than
/// `append`'s domain. With ten rows, "send the last one to the front" is `-1`, and "send the first
/// one to the end" is `0`; `+1` is second from the end of the lifted list, which is where `+1`
/// lands an `append` too.
///
/// Out of domain returns `None` and the caller refuses the operation — the same rule `append_index`
/// states, for the same reason: clamping would put the row where nobody asked.
pub fn move_index(len: usize, position: i64) -> Option<usize> {
    len.checked_sub(1)
        .and_then(|lifted| append_index(lifted, position))
}

/// Patch write semantics (ADR 0005). `Replace` swaps the pointed value;
/// `Append` extends a string / pushes into an array (the token-stream job
/// of the retired positional Concat).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub enum PatchKind {
    #[serde(rename = "replace")]
    #[default]
    Replace,
    #[serde(rename = "append")]
    Append,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchOp {
    pub slot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// JSON Pointer (RFC 6901) into the target node's WIRE shape — the
    /// field vocabulary external producers see, no second path syntax.
    pub path: String,
    pub op: PatchKind,
    pub value: Value,
}

/// Registration of a stamp definition (ADR 0037). `data` is the node tree itself — a
/// definition used to be a JSON *string* with `{{key}}` holes; that form is refused
/// (see `accrete::template::Templates::register`), because a value with a quote in it
/// used to break the body and nothing about the text could be checked at registration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InfluxTmpl {
    pub name: String,
    /// The names the definition takes: every `$name` in `data` is one of these, and
    /// every one of these is used.
    #[serde(default)]
    pub params: Vec<String>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Influx<T> {
    /// `create` addresses the layout root with `slot: ""` (the same convention
    /// `patch` uses); `set` names a data slot.
    pub slot: String,
    pub data: T,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub column: usize,
    #[serde(default)]
    pub header: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Empty;
