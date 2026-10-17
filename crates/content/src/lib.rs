pub mod codec;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{OneOrMany, serde_as};

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct Created(DateTime<Utc>);

impl Default for Created {
    fn default() -> Self {
        Self(Utc::now())
    }
}

type Session = String;

/// 操作通道（ev）的渲染类取值：生产端一律填这个，其余值渲染器忽略。
pub const EV_DRAW: &str = "draw";

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Message<T>
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    /// 消息级操作类别。"draw" = 渲染类（见 EV_DRAW）；必填，旧帧缺字段直接解码报错。
    pub ev: String,
    pub sender: Session,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<Created>,
    #[serde_as(as = "OneOrMany<_>")]
    pub content: Vec<Content<T>>,
}

impl<T> From<(Session, Content<T>)> for Message<T>
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    fn from(value: (Session, Content<T>)) -> Self {
        Message {
            ev: EV_DRAW.into(),
            sender: value.0,
            created: Some(Created::default()),
            content: vec![value.1],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Outflow {
    pub event: String,
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
    /// Plane: `event == ""` → layout root; `id` set → that list row;
    /// otherwise → the named data slot.
    #[serde(rename = "patch")]
    Patch(PatchOp),

    /// Remote write into the vals plane (ADR 0011): inject one `local` event
    /// as if the page had emitted it. The payload shape is the ADR 0010
    /// `Outflow`, so subscribers cannot tell a remote write from an in-page
    /// emit. Write is whole-value; the renderer short-circuits equal values
    /// (the write/read loop guard).
    #[serde(rename = "inject")]
    Inject(InjectOp),

    #[serde(rename = "empty")]
    #[default]
    Empty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AppendOp<T> {
    pub event: String,
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
    pub event: String,
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
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// JSON Pointer (RFC 6901) into the target node's WIRE shape — the
    /// field vocabulary external producers see, no second path syntax.
    pub path: String,
    pub op: PatchKind,
    pub value: Value,
}

/// Remote vals-plane write (ADR 0011). Field names mirror `Outflow` — `slot`
/// selects the channel, the rest is its payload — so the slot ends up holding
/// exactly what an in-page `Local` emit would write. Not generic over `T`:
/// the vals plane's currency is a bare `Value` (ADR 0008 — event data is not
/// display form), unlike every other plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InjectOp {
    pub slot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub data: Value,
}

impl InjectOp {
    /// The wrapper the slot receives. `event` defaults to the slot name — the
    /// same rule `BindVariant::Local` follows (the slot is the channel).
    pub fn outflow(&self) -> Outflow {
        Outflow {
            event: self.event.clone().unwrap_or_else(|| self.slot.clone()),
            id: self.id.clone(),
            data: self.data.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InfluxTmpl {
    pub name: String,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Influx<T> {
    pub event: String,
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
