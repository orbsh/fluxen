//! 已注册的 stamp：定义是**结构**，不是文本（ADR 0037）。
//!
//! 旧形态是 `名字 → 带 {{key}} 洞的 JSON 文本`：注册时不校验，实例化时做一遍文本替换再
//! 解析。一句话里带引号就把 JSON 打断，洞是不是变量全看上下文，注册那一刻没有东西可查。
//!
//! 现在一步就位：树在注册时解析、定型、数洞（每个 `$name` 必须在 `params` 里，`params`
//! 里每个名字必须在树里出现），实例化只是把洞换成值——整值替换，不拼文本。一个值**就是**
//! 字符串 `"$name"` 才是变量（引号只是 JSON 的语法，整值即变量），`$$name` 是"真以 `$` 开头"
//! 的转义。所以 `"共 $q 件"` 是字面量，永远不会被当成变量。

use super::{Accrete, AccreteOps};
use std::collections::HashMap;
use serde_json::{Map, Value};

/// 一份 stamp 定义：解析过、校验过的树 + 它接收的名字。
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    params: Vec<String>,
    body: Value,
}

/// 注册表：名字 → 定义。解析只在这里发生一次（注册时），之后每个实例都从这棵树上克隆。
#[derive(Debug, Clone, Default)]
pub struct Templates(HashMap<String, Template>);

impl Templates {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.0.get(name)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 注册一份定义：`params` 是它接收的名字，`body` 是这棵树。
    ///
    /// 注册时能查的全查：树里每个 `$name` 必须在 `params` 里（否则实例化时才发现的拼写
    /// 错误变成了静默的空值）；`params` 里每个名字必须在树里出现（声明了不用，等于一个
    /// 永远没人传的参数）；**洞摘掉之后**剩下的每一块必须仍是合法的 Accrete 节点 —— 节点
    /// 类型、属性词表、必填字段都在这里定案，解析只发生这一次。洞本身不查：它整值放进去，
    /// 什么都能放（这正是"不做形状检查"的另一面）。
    ///
    /// 旧的 `{{key}}` 文本形态明确拒收：静默解释成新形态是更坏的结局——帧作者以为自己在
    /// 用洞，实际拿到一个字面量。
    pub fn register(&mut self, name: &str, params: &[&str], body: &Value) -> Result<(), String> {
        let declared: Vec<String> = params.iter().map(|p| (*p).to_string()).collect();
        let template = Template::new(declared, body)
            .map_err(|e| format!("template {name:?} rejected: {e}"))?;
        self.0.insert(name.to_string(), template);
        Ok(())
    }
}

impl Template {
    fn new(params: Vec<String>, body: &Value) -> Result<Self, String> {
        if let Value::String(_) = body {
            return Err(OLD_TEXT_FORM.into());
        }
        // ① 洞摘掉之后，这棵树还是帧语言里的那棵树吗。
        let Some(skeleton) = skeleton(body) else {
            return Err("body is one hole — a definition is a node tree it can be spliced into".into());
        };
        serde_json::from_value::<Accrete>(skeleton)
            .map_err(|e| format!("body is not a node tree: {e}"))?;
        // ② 树上的洞与声明的名字必须一一对上。
        let mut used = Vec::new();
        collect(body, &mut used);
        if let Some(missing) = used.iter().find(|u| !params.contains(u)) {
            return Err(format!("`${missing}` is used but not declared in params"));
        }
        if let Some(unused) = params.iter().find(|p| !used.contains(p)) {
            return Err(format!("`{unused}` is declared but never used"));
        }
        Ok(Self {
            params,
            body: body.clone(),
        })
    }

    /// 实例化：把每个洞换成 `data` 里同名的值，整值替换。
    ///
    /// 值不做形状/类型检查——它就是被放进那个位置的东西（`$items` 站在 `children` 里时，
    /// 放进去的是一串节点）。唯一会报错的是**缺一个名字**：洞没有值可换，模板不该自己
    /// 编一个空值出来。
    pub fn instance(&self, data: &Map<String, Value>) -> Result<Accrete, String> {
        let mut body = self.body.clone();
        fill(&mut body, data)?;
        serde_json::from_value(body).map_err(|e| format!("instance is not a node tree: {e}"))
    }

    /// 它接收的名字（注册时校验过：恰好等于树上的洞）。
    pub fn params(&self) -> &[String] {
        &self.params
    }
}

/// 文本形态的拒绝语：说清**该做什么**，而不只是"这个不行"。
const OLD_TEXT_FORM: &str = "a template body is a node tree, not a `{{key}}` JSON string — \
                             write the tree and put `\"$name\"` (whole value) where a value goes";

/// 一个字符串是变量、是转义后的字面量，还是字面量。
///
/// - `"$name"` → 变量（整值就是变量；名字是标识符）；
/// - `"$$name"` → 字面量 `"$name"`（真以 `$` 开头的文本只有这一种写法）；
/// - 其它 → 字面量，包括 `"共 $q 件"` 这种洞在句子中间的字符串——变量必须占满整个值。
fn variable(s: &str) -> Option<&str> {
    let name = s.strip_prefix('$')?;
    if name.starts_with('$') {
        return None;
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_alphabetic() || first == '_') {
        return None;
    }
    chars
        .all(|c| c.is_alphanumeric() || c == '_')
        .then_some(name)
}

/// 把洞从树上摘掉：洞所在的键（或数组元素）整个消失，剩下的原样留下。
///
/// 注册时解析的就是这棵"没有洞的树"——洞站在哪儿就意味着那里少了点什么，而"少了点什么"
/// 仍然是合法的节点；洞本身不被检查（它整值放进去，形状由调用方决定）。洞如果占着定义
/// 的根，就没有可摘的外壳了：`None`。
fn skeleton(v: &Value) -> Option<Value> {
    match v {
        Value::String(s) => variable(s).is_none().then(|| v.clone()),
        Value::Array(a) => Some(Value::Array(a.iter().filter_map(skeleton).collect())),
        Value::Object(m) => Some(Value::Object(
            m.iter()
                .filter_map(|(k, x)| skeleton(x).map(|x| (k.clone(), x)))
                .collect(),
        )),
        _ => Some(v.clone()),
    }
}

/// 树上的洞，按出现顺序去重。
fn collect(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            if let Some(name) = variable(s)
                && !out.iter().any(|x| x == name)
            {
                out.push(name.to_string());
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
        Value::Object(m) => m.values().for_each(|x| collect(x, out)),
        _ => {}
    }
}

/// 整值替换：洞所在的那个值被换掉，周围的字符串一个字节都不碰（就没有"周围"）。
fn fill(v: &mut Value, data: &Map<String, Value>) -> Result<(), String> {
    match v {
        Value::String(s) => {
            if let Some(name) = variable(s) {
                match data.get(name) {
                    Some(x) => *v = x.clone(),
                    None => return Err(format!("missing value for `${name}`")),
                }
            } else if s.starts_with("$$") {
                // One `$` is the escape, not two: `$$name` is the literal `$name`.
                *s = s[1..].to_string();
            }
        }
        Value::Array(a) => {
            for x in a.iter_mut() {
                fill(x, data)?;
            }
        }
        Value::Object(m) => {
            for x in m.values_mut() {
                fill(x, data)?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl Accrete {
    /// 深度优先展开本节点及后代的 `stamp`，就地替换。
    /// 返回警告串（未注册名、缺值、实例不再是节点）——accrete 零日志依赖，
    /// 由调用方决定打 warn（对齐 ADR 0005 的 miss-warn-drop 惯例）。
    /// 失败节点保持 `stamp` 原形，渲染端有兜底占位。
    pub fn expand(&mut self, env: &Templates) -> Vec<String> {
        let mut warns = Vec::new();
        self.expand_into(env, &mut warns);
        warns
    }

    fn expand_into(&mut self, env: &Templates, warns: &mut Vec<String>) {
        if let Accrete::stamp(r) = self {
            match env.get(&r.name) {
                Some(t) => match t.instance(&r.data) {
                    Ok(x) => *self = x,
                    Err(e) => {
                        warns.push(format!("stamp {:?}: {e}", r.name));
                        return;
                    }
                },
                None => {
                    warns.push(format!("stamp {:?} not registered", r.name));
                    return;
                }
            }
        }
        // 展开产物本身可能还挂 stamp（模板套模板），统一继续下钻。
        if let Some(cs) = self.borrow_children_mut() {
            for c in cs {
                c.expand_into(env, warns);
            }
        }
    }
}