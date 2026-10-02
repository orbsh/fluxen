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
use serde_json::{Map, Value, to_value};
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
    fn get_bind(&self) -> Option<&HashMap<String, Bind>>;
    fn set_bind(&mut self, bind: Option<HashMap<String, Bind>>);
    fn get_id(&self) -> &Option<String>;
}

#[cfg(feature = "ops")]
pub trait Wrap {
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
        match self {
            Self::number => to_value(0),
            Self::bool => to_value(false),
            _ => to_value(""),
        }
        .unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BindVariant {
    Source {
        source: String,
        /// 可选 JSON Pointer（ADR 0008）：进槽内节点的 WIRE SHAPE 提取
        /// 子值（与 ADR 0005 patch 路径同一词法）；省略 = 取槽节点自己的
        /// `bind[<key>].default`（同 key 直传惯例）。
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    /// 具名本地值槽（ADR 0008）：独立于 data（Accrete 展示形态）的
    /// 裸 Value 平面——发射侧省略 `path`（整值写入），订阅侧带 `path`
    /// 时进槽内 Value 的 JSON Pointer 提取子值（形状归生产端，
    /// 与 wire-shape 无关）。
    /// ADR 0010：落槽载荷与上行同形（`Outflow {event, id?, data}` 包装
    /// 对象）——事件形状由事件定义、不由落点决定，`kind: event` 改绑
    /// `kind: local` 订阅方零改动。`event` 缺省 = 槽名（槽即频道）；
    /// `path` 仍是订阅侧字段，发射侧忽略。
    Local {
        slot: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        event: Option<String>,
    },
    Event {
        event: String,
    },
    Field {
        field: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<Value>,
    },
    Submit {},
    Default {},
}

impl Default for BindVariant {
    fn default() -> Self {
        BindVariant::Default {}
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct Bind {
    #[serde(flatten)]
    pub variant: BindVariant,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<JsType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct ClassAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// 容器自身的排布方向（同 CaseAttr 字段；ADR 0010 缺陷修复：
    /// 此前只有 Case 认识 horizontal，select/rack 的 col 类无法被
    /// 自己的 attrs 推翻——父 case 的 horizontal 只管父 div）。
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct DirectionAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct StyleAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Placeholder {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Chart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// G2 封装模块的 ES module 地址（ADR 0009：ADR 0007 契约，
    /// mount/update/resize/unmount）。默认指向自家静态资产——
    /// 版本固定，换渲染实现/CDN 时显式覆盖。
    #[serde(default = "default_chart_url")]
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    /// `bind["value"]`：G2 spec（纯数据，GoG 词汇——type/data/encode/
    /// transform/scale/interaction）。流式更新 = set/patch 指针进
    /// `/data` 等节点，封装模块整 spec 重设 + `chart.update()` 增量重绘。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
}

pub fn default_chart_url() -> String {
    "/assets/g2chart/index.js".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Diagram {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
}

/// 外部渲染模块的原位挂载容器（ADR 0007）。
/// 核心不解释 `data`——载荷语义完全归模块；尺寸/样式走 `attrs`，
/// 与数据流的联动走现有 bind 槽协议（`set`/`patch` 按 `data_event` 寻址）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Canvas {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// CDN 上的 ES module 地址（导出 mount/update/resize/unmount 契约）。
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<SizeAttr>,
    /// `bind["value"]`：载荷惯例与 Chart/diagram 同族——`kind: source` 指向
    /// 具名数据槽（槽内节点的 `bind.value.default` 原始 JSON 即模块 data 区），
    /// 流式喂数据 = 对该槽 `set`/`patch`（指针 `/bind/value/default/...`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Float {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<PositionAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct FoldAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_header: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub float_body: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Fold {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<FoldAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct FormAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instant: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Form {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<FormAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Popup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<DirectionAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
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
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Group {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<StyleAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
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
    pub bind: Option<HashMap<String, Bind>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum PagesDisplay {
    /// 只挂载命中行：切页省内存，子树状态（滚动/输入/GL）随卸载丢失。
    #[default]
    Render,
    /// 所有行常驻 DOM，未命中行容器加 class `hide`（display:none）：
    /// 状态全保，代价是全部行的渲染与内存常驻（生产端自选）。
    Dom,
}

/// id 字典页面容器（ADR 0010）：`bind["value"]`（source，list 平面）喂
/// 页面行——行 = 任意 Accrete 节点，其 `id` 即字典键（行身份在数据里，
/// 同 rack 纪律）；`bind["select"]`（local 或 source，可带 `path`）是
/// 选中信号，tracked 读、按 path 提键（path 缺省 = 整值；提出来必须是
/// 字符串，否则 warn + 不切换）。命中键显示对应行，未命中 = warn + 空白。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Pages {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(default)]
    pub display: PagesDisplay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct RackAttr {
    #[serde(default)]
    pub scroll: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Rack {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<RackAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<Vec<Accrete>>,
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
    pub bind: Option<HashMap<String, Bind>>,
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
    pub bind: Option<HashMap<String, Bind>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Input {
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Select {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Table {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Thead {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Tbody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Tr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Th {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Td {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
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
    pub bind: Option<HashMap<String, Bind>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct TextArea {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<ClassAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct CaseAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<bool>,
    #[allow(non_camel_case_types)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Case {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<CaseAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Template {
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
    case(Case),
    #[ui_acrete(has_id = "true")]
    placeholder(Placeholder),
    #[ui_acrete(has_id = "true")]
    chart(Chart),
    #[ui_acrete(has_id = "true")]
    diagram(Diagram),
    #[ui_acrete(has_id = "true")]
    canvas(Canvas),
    float(Float),
    #[ui_acrete(has_id = "true")]
    fold(Fold),
    form(Form),
    popup(Popup),
    svg(Svg),
    group(Group),
    path(Path),
    #[ui_acrete(has_id = "true")]
    rack(Rack),
    #[ui_acrete(has_id = "true")]
    pages(Pages),
    button(Button),
    image(Image),
    input(Input),
    select(Select),
    table(Table),
    thead(Thead),
    tbody(Tbody),
    tr(Tr),
    th(Th),
    td(Td),
    text(Text),
    textarea(TextArea),
    template(Template),
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
                    arr.into_iter().nth(idx).ok_or_else(|| {
                        format!("pointer miss: {path:?} (no index {idx})")
                    })?
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
                Value::Object(map) => {
                    map.get_mut(seg.as_str()).ok_or_else(|| {
                        format!("pointer miss: {path:?} (no key {seg:?})")
                    })?
                }
                Value::Array(arr) => {
                    let idx: usize = seg
                        .parse()
                        .map_err(|_| format!("bad array index {seg:?} at {path:?}"))?;
                    arr.get_mut(idx).ok_or_else(|| {
                        format!("pointer miss: {path:?} (no index {idx})")
                    })?
                }
                _ => return Err(format!("pointer miss: {path:?} (hit non-container)")),
            };
        }

        // Last segment: apply f. Note RFC 6901 makes "/" the member keyed
        // "" (not the node itself), which the Object arm below handles.
        let seg = segs.last().unwrap();
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
        self.with_pointer(path, |mut old| {
            match old {
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
            }
        })
    }
}

/*
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(tag = "type")]
pub enum JsonTableAccrete {
    thead,
    tbody,
    tr,
    th,
    td,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(tag = "type")]
pub enum JsonSvgAccrete {
    group,
    path,
}
*/
