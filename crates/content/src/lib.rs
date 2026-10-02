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
    pub data: T,
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
