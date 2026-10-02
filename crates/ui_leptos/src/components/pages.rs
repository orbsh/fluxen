use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{use_common_css, use_source, use_source_id, use_source_untracked};
use accrete::{Accrete, AccreteOps, Pages, PagesDisplay};
use leptos::html::*;
use leptos::prelude::*;
use leptos::tachys::reactive_graph::OwnedView;
use serde_json::Value;

/// 行字典（ADR 0010）：键 = 行 `id`（同 rack 行身份纪律，无 id 退 `#{idx}`）。
/// 纯函数，单测覆盖。
pub(crate) fn dict_of(rows: &[Accrete]) -> std::collections::HashMap<String, Accrete> {
    rows.iter()
        .enumerate()
        .map(|(idx, r)| {
            (
                r.get_id().clone().unwrap_or_else(|| format!("#{idx}")),
                r.clone(),
            )
        })
        .collect()
}

/// 选中信号 → 字典键：字符串整值即键；非字符串 None（调用方 warn + 不切换）。
pub(crate) fn key_of(v: &Value) -> Option<String> {
    v.as_str().map(String::from)
}

/// id 字典页面容器（ADR 0010）。
///
/// `bind["value"]`：source（list 平面）页面行，行 id = 字典键。
/// `bind["select"]`：local/source 订阅，path 提键（path 缺省 = 整值）。
/// `display`：`Render` 只挂命中行（切页丢子树状态）；`Dom` 全挂、
/// 未命中行容器加 `hide` class（状态全保）。未命中键 warn + 空白。
pub fn pages_(accrete: Pages, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    // 同 case/rack 模式：容器带 `f`（flex:1）尽量占满父空间；默认 `col`
    // （由 use_common_css 的类型表授予，可被自身 attrs.horizontal 推翻）。
    // 内部页面容器同样 `f`（hide 位于 custom 层，层序压过 layout 的 `.f`，
    // 未命中页不因此复显）。
    let mut css = vec!["pages", "f"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let display = accrete.display.clone();
    // 页面行来源：bind["value"] 必须是 Source（list 平面）
    let Some(source) = use_source_id(&accrete).cloned() else {
        return div().id(id.as_str()).class(css.as_str()).into_any();
    };
    let slot = ctx.slot_for_list(&source);
    let parent_owner = Owner::current().expect("pages_ requires a reactive owner");

    // 选中键：effect 订阅 select 绑定（local 槽/source 槽 + path 提取），
    // 把「读绑定 + 判类型」集中在这一处，渲染闭包只消费键信号。
    // 初值 untracked 读（同发射侧纪律：避免订阅链在构造期就挂上）。
    let seed = key_of(&use_source_untracked(&ctx, &accrete, "select").unwrap_or(Value::Null));
    let selected: RwSignal<Option<String>> = parent_owner.with(|| RwSignal::new(seed));
    {
        let ctx2 = ctx.clone();
        let acc2 = accrete.clone();
        Effect::new(move |_| match use_source(&ctx2, &acc2, "select") {
            Some(v) => match key_of(&v) {
                Some(k) => selected.set(Some(k)),
                // 非字符串 = warn + 不切换（保持上一个键）
                None => tracing::warn!("pages: select value is not a string: {v}"),
            },
            // 槽未写入：无键（空白）
            None => selected.set(None),
        });
    }

    // render 模式：整棵命中页面包在一个按 selected 反应的视图里——
    // 只有命中行变化才重建（其余行从未 mount）。
    if matches!(display, PagesDisplay::Render) {
        let ctx = ctx.clone();
        let id = id.clone();
        let css = css.clone();
        let po = parent_owner.clone();
        return move || -> AnyView {
            let Some(k) = selected.get() else {
                return div().id(id.as_str()).class(css.as_str()).into_any();
            };
            let rows = slot.get();
            match dict_of(&rows).get(&k) {
                Some(row) => {
                    // 每次切换独立 owner：卸载即随 owner 清理（丢状态，正是
                    // render 模式的语义）；壳与 dom 模式同形（page f）
                    let owner = po.with(Owner::new);
                    let view = owner.with(|| render_accrete(&ctx, row));
                    div()
                        .id(id.as_str())
                        .class(css.as_str())
                        .child(
                            div()
                                .class("page f")
                                .child(OwnedView::new_with_owner(view, owner)),
                        )
                        .into_any()
                }
                None => {
                    tracing::warn!("pages: no page for key {k:?}");
                    div().id(id.as_str()).class(css.as_str()).into_any()
                }
            }
        }
        .into_any();
    }

    // dom 模式：外层闭包只订阅行槽（不读 selected）——切页若惊动外层就会
    // 整批重建行节点，正是要避免的。行的显隐是各自 class 闭包读
    // selected，因此只翻转 display，DOM 节点与子树状态全程存活。
    let ctx = ctx.clone();
    let id = id.clone();
    let css = css.clone();
    move || -> AnyView {
        let rows = slot.get();
        let children = rows
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                let k = row.get_id().clone().unwrap_or_else(|| format!("#{idx}"));
                let sel = selected;
                div()
                    // 可见页 `f`（占满 pages 容器）；隐藏页不带 `f`——
                    // 同 div 上 `.f` 与 `.hide` 冲突（实测 display 被算回
                    // flex），互斥直接做进 class 切换
                    .class(move || {
                        if sel.get().as_deref() == Some(k.as_str()) {
                            "page f"
                        } else {
                            "page hide"
                        }
                    })
                    .child(render_accrete(&ctx, row))
                    .into_any()
            })
            .collect::<Vec<_>>();
        div().id(id.as_str()).class(css.as_str()).child(children).into_any()
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(id: Option<&str>, text: &str) -> Accrete {
        let mut v = json!({
            "type": "text",
            "bind": { "value": { "kind": "default", "default": text } }
        });
        if let Some(i) = id {
            v.as_object_mut().unwrap().insert("id".into(), i.into());
        }
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn dict_keys_rows_by_id_with_positional_fallback() {
        let d = dict_of(&[row(Some("home"), "h"), row(None, "p"), row(Some("about"), "a")]);
        assert_eq!(d.len(), 3);
        assert_eq!(d["home"].get_bind().unwrap()["value"].default, Some(json!("h")));
        assert_eq!(d["#1"].get_bind().unwrap()["value"].default, Some(json!("p")));
        assert_eq!(d["about"].get_bind().unwrap()["value"].default, Some(json!("a")));
    }

    #[test]
    fn key_extraction_accepts_only_strings() {
        assert_eq!(key_of(&json!("home")), Some("home".into()));
        assert_eq!(key_of(&json!(2)), None);
        assert_eq!(key_of(&json!(null)), None);
        assert_eq!(key_of(&json!({ "data": "x" })), None);
    }
}
