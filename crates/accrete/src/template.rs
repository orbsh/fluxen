use super::Accrete;
use crate::AccreteOps;
use std::collections::HashMap;

/// 已注册模板表：名字 → 带 `{{key}}` 槽位的 Accrete JSON 文本（ADR 0006）。
pub type Templates = HashMap<String, String>;

/// 替换单个模板体：字符串值 → 原样文本（插值进 `"..."` 内），
/// 其余 JSON 值 → 紧凑序列化（拼接完整节点/数组）。替换后整体 parse
/// 回 Accrete。循环/条件由生产端预生成 JSON 片段塞槽位，
/// 这里没有模板语言。宽松匹配空白：`{{k}}` 与 `{{ k }}` 同形。
/// 不支持 `{% %}`、过滤器、路径取值。
pub fn subst(
    body: &str,
    data: &serde_json::Map<String, serde_json::Value>,
) -> Result<Accrete, String> {
    let mut out = body.to_string();
    for (k, v) in data {
        let s = match v {
            // 字符串 = 插值语义（同 Jinja `{{ x }}`）：不带引号原样注入。
            // 含引号的值会破坏 JSON——生产端的义务，替换层不转义。
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out = out.replace(&format!("{{{{{k}}}}}"), &s);
        out = out.replace(&format!("{{{{ {k} }}}}"), &s);
    }
    serde_json::from_str::<Accrete>(&out).map_err(|e| format!("rendered template invalid: {e} => {out}"))
}

impl Accrete {
    /// 深度优先展开本节点及后代的 template 变体，就地替换。
    /// 返回警告串（未注册名、解析失败）——accrete 零日志依赖，
    /// 由调用方决定打 warn（对齐 ADR 0005 的 miss-warn-drop 惯例）。
    /// 失败节点保持 `template` 原形，渲染端有兜底占位。
    pub fn expand(&mut self, env: &Templates) -> Vec<String> {
        let mut warns = Vec::new();
        self.expand_into(env, &mut warns);
        warns
    }

    fn expand_into(&mut self, env: &Templates, warns: &mut Vec<String>) {
        if let Accrete::template(r) = self {
            match env.get(&r.name) {
                Some(body) => match subst(body, &r.data) {
                    Ok(x) => *self = x,
                    Err(e) => {
                        warns.push(format!("template {:?}: {e}", r.name));
                        return;
                    }
                },
                None => {
                    warns.push(format!("template {:?} not registered", r.name));
                    return;
                }
            }
        }
        // 展开产物本身可能还挂 template（模板套模板），统一继续下钻。
        if let Some(cs) = self.borrow_children_mut() {
            for c in cs {
                c.expand_into(env, warns);
            }
        }
    }
}
