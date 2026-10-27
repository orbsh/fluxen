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
        // JSON scalars have infallible constructors: no serializer round trip,
        // so nothing can fail for a value this module itself chose.
        match self {
            Self::number => Value::from(0),
            Self::bool => Value::Bool(false),
            _ => Value::String(String::new()),
        }
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

/// 代码编辑器封装模块的默认地址（ADR 0016）。Monaco 的 AMD 子集装在
/// `assets/monaco/vs/**`，封装模块在 `assets/editor/`——两者都是同一档
/// 「版本固定的静态资产」；换 CDN 时显式覆盖本字段。
pub fn default_editor_url() -> String {
    "/assets/editor/index.js".to_string()
}

/// 代码编辑器（ADR 0016）。走 ADR 0007 的模块通道——与 Canvas/Chart 同一条
/// 挂载路径，库（Monaco）在模块 `mount()` 里才拉（ADR 0015）。
/// 数据区（`bind["value"]`）载荷 = `{code, lang?, event?}`：
/// - `code`：源码文本（"代码作为参数"）；
/// - `lang`：Monaco 语言 id（缺省 plaintext）；
/// - `event`：保存时上行的对事件名（缺省 `submit`）——编辑器自己在 Ctrl/Cmd+S
///   里 `host.send(event, CBOR({code, lang}))`，与表单 submit 同一条上行腿。
/// `{}`/Null = 尚无数据 → 不占位（ADR 0012）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Editor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 封装模块的 ES module 地址（ADR 0007 契约）。默认指向自家静态资产。
    #[serde(default = "default_editor_url")]
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<SizeAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
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
    /// 模块私有选项的**动态袋**（ADR 0017）：核心不认识的渲染器配置
    /// （3d-force-graph 的节点/边尺寸、力参数、配色……）打包放这里，
    /// 宿主原样交给模块的 `mount(el, data, host, options)`，一个键都不解释。
    /// 这是"变体不进 accrete"的落点：换渲染器只换 url + options，DSL 不变。
    /// 缺省/Null = `{}`（模块自己套默认值）；与 `url` 同一语义——挂载期配置，
    /// 不进 update 热更新腿（改它要靠重新挂载）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
    /// `bind["value"]`：载荷惯例与 Chart/diagram 同族——`kind: source` 指向
    /// 具名数据槽（槽内节点的 `bind.value.default` 原始 JSON 即模块 data 区），
    /// 流式喂数据 = 对该槽 `set`/`patch`（指针 `/bind/value/default/...`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
}

/// 三维力导向关系图（ADR 0018）。原生组件，不是 canvas 模块：渲染器与布局
/// 都在 wasm 里（`forcegraph` crate），所以没有 `url`、没有 JS 资产、没有
/// mount/update/resize/unmount 契约——可写的只有 `attrs`、渲染器旋钮 `style`
/// 和数据区 `bind["value"]`。
///
/// 数据区载荷 = `forcegraph::Snapshot`：`{nodes, links, focus?}`，按 id 求
/// 差量（整份快照重复送达幂等）。`focus` 是视图指令
/// `{ids, seq?, padding_px?, duration_ms?}`——`seq` 变了同一个目标可以再飞一次，
/// 不动 `seq` 的重复写入不碰相机。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct ForceGraph {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<SizeAttr>,
    /// 渲染器旋钮，全部有缺省值；缺省时不进 wire。
    #[serde(default, skip_serializing_if = "force_graph_style_is_default")]
    pub style: ForceGraphStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
}

/// 图渲染器的旋钮。语义与 canvas 的 `options` 同级：挂载期配置，改它要
/// 重新挂载（不进 update 腿）。区别只在于这是自家渲染器的词表，所以可以
/// 类型化、可以被 schema 和 patch 校验。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
// 结构级 default：帧里只写想改的旋钮，其余取 `Default::default()`。
// 字段级 default 只管"整个 style 缺席"，写半个对象就会报 missing field。
#[serde(default)]
pub struct ForceGraphStyle {
    /// 节点基础半径（屏幕像素）。
    pub node_size_px: f32,
    /// 每档权重增加的半径（屏幕像素）；档数见 `node_tiers`。
    pub node_size_step_px: f32,
    pub node_tiers: u8,
    pub link_width_px: f32,
    pub link_opacity: f32,
    /// 斥力，负值 = 相互推开。
    pub charge: f32,
    pub link_distance: f32,
    pub link_strength: Option<f32>,
    pub velocity_decay: f32,
    /// 常显名字标签层。
    pub labels: bool,
    pub label_px: f32,
    /// 超过这个节点数只画不标（标签层是逐帧投影的 DOM，量大就成噪声）。
    pub label_max_nodes: u32,
    /// 首批数据后自动取景到整张图；之后的追加不重排、不飞相机。
    pub auto_fit: bool,
    pub fit_padding_px: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// 节点配色，按 `kind` 轮取；空 = 用内置的暗色方案。
    #[serde(default)]
    pub palette: Vec<String>,
}

impl Default for ForceGraphStyle {
    fn default() -> Self {
        Self {
            node_size_px: 7.0,
            node_size_step_px: 3.0,
            node_tiers: 3,
            // 2.5 px / 0.6 alpha: at 1.5 px and 0.35 the edges were drawn but
            // invisible on a light page (measured: 540 → 2486 coloured pixels
            // when the width went to 16 px).
            link_width_px: 2.5,
            link_opacity: 0.6,
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
        }
    }
}

fn force_graph_style_is_default(style: &ForceGraphStyle) -> bool {
    *style == ForceGraphStyle::default()
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

/// 模态层（ADR 0014）：应用显式声明的全屏挂载点，portal 把内容传送进来。
/// 显隐只有一条规则——所绑 `bind["value"]` 槽非空即显示（空即无，ADR 0012 复用）；
/// `dismiss` 点层自身（模糊区）时清空该槽（清空即关）。几何内联在渲染侧，
/// 遮罩走 custom.css。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Layer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<LayerAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<HashMap<String, Bind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct LayerAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// dismiss（ADR 0014）：点层自身（模糊区，而非模态内容）清空所绑槽。
    /// 本页瞬时状态：只写 local/data 平面，不上线协议。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dismiss: Option<bool>,
}

/// 渲染位置传送（ADR 0014）：children 渲染到 `to` 选中的元素里，就地只留一个空锚点。
/// 只做位置，不管显隐与遮罩——那是 layer 的语义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(feature = "ops", derive(AccreteOps))]
#[cfg_attr(feature = "classify", derive(ClassifyAccrete))]
pub struct Portal {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<PortalAttr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Accrete>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[cfg_attr(any(feature = "ops", feature = "classify"), derive(ClassifyAttrs))]
pub struct PortalAttr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// 目标挂载点：CSS 选择器，选中应用显式声明的 layer 宿主（ADR 0014：挂载点由
    /// 应用声明，渲染器不加固定逻辑；解析不到就不渲染 + warn）。
    pub to: String,
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
    #[ui_acrete(has_id = "true")]
    forcegraph(ForceGraph),
    #[ui_acrete(has_id = "true")]
    editor(Editor),
    float(Float),
    #[ui_acrete(has_id = "true")]
    layer(Layer),
    portal(Portal),
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
