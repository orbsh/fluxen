pub mod codec;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 下行的一帧就是**动作列表**本身（ADR 0021 决策 6）：没有信封。
/// 宿主要带自己的 `ev`／`sender`／`created`，就装在宿主自己的帧里——三个宿主
/// （stage / prism / reflux）各有各的，只在 `Content` 这一层对齐。

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Outflow {
    /// 上行目标：页面把这次交互送到宿主上的哪个目标（ADR 0023）。与下行的 `slot` 不同名——
    /// 上下行并不对称（一个写页面里的位置，一个调起宿主的处理器）。`event` 不在这条线上。
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

    /// Value-domain update addressed by JSON Pointer (ADR 0005).
    /// Plane: `slot == ""` → layout root; `id` set → that list row;
    /// otherwise → the named data slot.
    #[serde(rename = "patch")]
    Patch(PatchOp),

    /// Page configuration (ADR 0038 §2): options by name, each written into that
    /// option's `remote` half. Merge, not replace — a member left out keeps its value,
    /// and a member that is `null` clears its remote half. Not generic over `T`: the
    /// configuration's currency is a bare `Value`, not a node tree.
    #[serde(rename = "config")]
    Config(ConfigOp),

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
    /// 落点（ADR 0005）。0 = 末尾（默认，即追加）；正数从**尾**往内数（+1 = 最后一个之前，
    /// +len = 头部）；负数从**头**往内数（-1 = 头部）。域外由渲染器拒收，见 `append_index`。
    ///
    /// 方向是刻意的：索引一律从本操作自己的锚定端往内数——append 锚在尾，所以正数往头走；
    /// 从头部锚定的插入与它正负相反。
    #[serde(default, skip_serializing_if = "is_zero")]
    pub position: i64,
    pub data: T,
}

/// 省缺即 0（末尾）：位置没有信息时帧里不写这个字段——`position: 0` 是可由缺省推出的冗余。
fn is_zero(n: &i64) -> bool {
    *n == 0
}

/// 一行落在列表的哪个下标：`len` 是当前行数，`position` 是线上那个数。
///
/// | position | 落点 |
/// |---|---|
/// | `0` | 末尾（追加，默认） |
/// | `+n`（`1..=len`） | 从尾往内第 n 个位置之前（`+1` = 最后一个之前，`+len` = 头部） |
/// | `-n`（`1..=len`） | 从头往内第 n 个位置之前（`-1` = 头部，`-len` = 最后一个之前） |
///
/// 域外返回 `None`：调用方拒收该操作并计数，**不夹取**——夹取会把行放到调用方没要求的
/// 位置，那是静默降级。注意它与你熟悉的 `Vec::insert(i)` / `list.insert(i)` 差一位：那里的
/// `i` 是数组下标，这里的数是从锚定端数过来的第几个位置。
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

/// One frame's worth of options (ADR 0038 §2). `data` is required: a `config` frame
/// that carries nothing is a producer bug, and an absent `data` fails to decode here
/// rather than silently meaning "no change".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConfigOp {
    pub data: Map<String, Value>,
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
