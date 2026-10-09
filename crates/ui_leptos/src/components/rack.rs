use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{use_common_css, use_source_id};
use accrete::classify::Classify;
use accrete::{Accrete, AccreteOps, Rack, RackAttr};
use leptos::html::*;
use leptos::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct ItemContainer {
    default: Option<Accrete>,
    index: HashMap<String, Accrete>,
}

impl From<Vec<Accrete>> for ItemContainer {
    fn from(data: Vec<Accrete>) -> Self {
        let mut default = None;
        let mut index = HashMap::new();
        for l in &data {
            if let Some(x) = l.get_selector() {
                index.insert(x.to_owned(), l.clone());
            } else {
                default = Some(l.clone());
            }
        }
        ItemContainer { index, default }
    }
}

impl ItemContainer {
    fn select(&self, child: &Accrete) -> Option<Accrete> {
        if let Some(s) = child.get_selector()
            && let Some(i) = self.index.get(s)
        {
            return Some(i.clone());
        }
        self.default.clone()
    }
}

/// 行键：有 id 用 id，无 id 退 `#{idx}`。keyed 键集与位置索引共用这一条约定，
/// 两处不再各写一遍。
fn row_key(child: &Accrete, idx: usize) -> String {
    child
        .get_id()
        .clone()
        .unwrap_or_else(|| format!("#{idx}"))
}

/// 行位置索引：一个 list 值 → 键到位置。list 每次变更建一遍（O(rows)），
/// 行 memo 取行由线性扫 id（N 行 × O(N) = O(N²)）降为查表 O(1)。
/// 重复键取首个命中，与原 `iter().find` 的语义一致。
fn row_positions(rows: &[Accrete]) -> HashMap<String, usize> {
    let mut pos = HashMap::with_capacity(rows.len());
    for (idx, child) in rows.iter().enumerate() {
        pos.entry(row_key(child, idx)).or_insert(idx);
    }
    pos
}

/// 列表容器：按 selector 索引 `item` 模板，遍历 `ctx.list[source]` 渲染。
pub fn rack_(accrete: Rack, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    // 行 owner 的挂载点：本函数执行时的 reactive owner（rack 自身的作用域），
    // 与外层 list 更新解耦——行订阅随 rack 消亡，不随外层重建丢失。
    let parent_owner = Owner::current().expect("rack_ requires a reactive owner");
    let mut css = vec!["rack", "f"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let item: ItemContainer = accrete.item.clone().unwrap_or_default().into();
    let Some(source) = use_source_id(&accrete).cloned() else {
        return div().into_any();
    };
    let scroll = accrete
        .attrs
        .as_ref()
        .map(|RackAttr { scroll, .. }| *scroll)
        .unwrap_or(false);

    let slot = ctx.slot_for_list(&source);
    // 行位置索引：list 每次变更只建一遍，所有行 memo 共用。建在外层闭包之外
    // ——闭包每次重跑都会重建自身状态，索引必须比它活得久（索引与行 owner 同
    // 挂 rack 作用域，父渲染重跑时一起重建）。
    let index = Memo::new(move |_| std::sync::Arc::new(row_positions(&slot.get())));
    move || -> AnyView {
        // 只订阅本 source 的槽：其他键的更新不触发本闭包
        let c = slot.get();
        // keyed list：行身份 = Accrete id（无 id 行按位置 "#{idx}" 兜底）。
        // 外层键集 diff：追帧只给新键建行，旧行节点存活；行内容做成
        // 反应式闭包再订阅 ctx.list，同 id 行的 merge 更新原地刷新，
        // 不重建行节点。
        let keys: Vec<String> = c
            .iter()
            .enumerate()
            .map(|(idx, child)| row_key(child, idx))
            .collect();
        let lcx = ctx.clone();
        let item2 = item.clone();
        let po = parent_owner.clone();
        let children = leptos::tachys::view::keyed::keyed(
            keys,
            |k: &String| k.clone(),
            move |_, key: String| {
                // 每行独立 owner（挂在 rack 的 owner 下）：外层键集 diff 不清理
                // 未变动行的 owner，行内 memo/订阅因此跨外层重建存活。
                let owner = po.with(Owner::new);
                let view = owner.with(|| {
                    let item = item2.clone();
                    let k2 = key.clone();
                    // 按行 memo：list 变更只重算本行 Accrete；输出相等则下游不动。
                    // 取行 = 位置索引查表（O(1)，id 行与 `#{idx}` 位置行同一条路），
                    // 槽句柄一次取好在闭包外——原先每跑一次 memo 查一遍注册表、
                    // 再线性扫 id。
                    let memo = Memo::new(move |_| {
                        let pos = *index.get().get(&k2)?;
                        slot.get().get(pos).cloned()
                    });
                    // 惰性闭包订阅 memo：本行 merge 时原地重渲染。
                    let rctx = lcx.clone();
                    move || -> AnyView {
                        let Some(child) = memo.get() else {
                            return div().into_any();
                        };
                        match item.select(&child) {
                            Some(mut template) => {
                                // 模板外壳 + child 作为其 children
                                template.set_children(vec![child]);
                                render_accrete(&rctx, &template)
                            }
                            None => render_accrete(&rctx, &child),
                        }
                    }
                    .into_any()
                });
                (
                    move |_index: usize| {},
                    leptos::tachys::reactive_graph::OwnedView::new_with_owner(view, owner),
                )
            },
        );

        if scroll {
            let id_ = id.clone();
            leptos::task::spawn_local(async move {
                crate::dom::eval(&format!(
                    r#"
                    var e = document.getElementById("{id_extra}");
                    if (e && Math.abs(e.scrollHeight - e.offsetHeight - e.scrollTop) < e.offsetHeight) {{
                        e.scrollTop = e.scrollHeight;
                    }}
                    "#,
                    id_extra = id_
                ));
            });
        }

        div().id(id.as_str()).class(css.as_str()).child(children).into_any()
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(id: Option<&str>) -> Accrete {
        let mut v = json!({
            "type": "text",
            "bind": { "value": { "kind": "default", "default": "x" } }
        });
        if let Some(i) = id {
            v.as_object_mut().unwrap().insert("id".into(), i.into());
        }
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn index_keys_are_the_keyed_row_keys() {
        let rows = [row(Some("a")), row(None), row(Some("b"))];
        let keys: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(idx, r)| row_key(r, idx))
            .collect();
        assert_eq!(keys, vec!["a", "#1", "b"]);
        let pos = row_positions(&rows);
        assert_eq!(pos["a"], 0);
        assert_eq!(pos["#1"], 1);
        assert_eq!(pos["b"], 2);
        assert_eq!(pos.len(), 3);
    }

    #[test]
    fn duplicate_keys_keep_the_first_hit() {
        // 同 id 两行：取首个，与原先 `iter().find` 一致
        let rows = [row(Some("dup")), row(Some("dup"))];
        assert_eq!(row_positions(&rows)["dup"], 0);
    }
}
