use crate::Ctx;
use crate::ctx::render_brick;
use crate::hooks::{use_common_css, use_source_id};
use brick::classify::Classify;
use brick::{Brick, BrickOps, Rack, RackAttr};
use leptos::html::*;
use leptos::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct ItemContainer {
    default: Option<Brick>,
    index: HashMap<String, Brick>,
}

impl From<Vec<Brick>> for ItemContainer {
    fn from(data: Vec<Brick>) -> Self {
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
    fn select(&self, child: &Brick) -> Option<Brick> {
        if let Some(s) = child.get_selector()
            && let Some(i) = self.index.get(s)
        {
            return Some(i.clone());
        }
        self.default.clone()
    }
}

/// 列表容器：按 selector 索引 `item` 模板，遍历 `ctx.list[source]` 渲染。
pub fn rack_(brick: Rack, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    // 行 owner 的挂载点：本函数执行时的 reactive owner（rack 自身的作用域），
    // 与外层 list 更新解耦——行订阅随 rack 消亡，不随外层重建丢失。
    let parent_owner = Owner::current().expect("rack_ requires a reactive owner");
    let mut css = vec!["rack", "f"];
    use_common_css(&mut css, &brick);
    let css = css.join(" ");

    let item: ItemContainer = brick.item.clone().unwrap_or_default().into();
    let Some(source) = use_source_id(&brick).cloned() else {
        return div().into_any();
    };
    let scroll = brick
        .attrs
        .as_ref()
        .map(|RackAttr { scroll, .. }| *scroll)
        .unwrap_or(false);

    let slot = ctx.slot_for_list(&source);
    move || -> AnyView {
        // 只订阅本 source 的槽：其他键的更新不触发本闭包
        let c = slot.get();
        // keyed list：行身份 = Brick id（无 id 行按位置 "#{idx}" 兜底）。
        // 外层键集 diff：追帧只给新键建行，旧行节点存活；行内容做成
        // 反应式闭包再订阅 ctx.list，同 id 行的 merge 更新原地刷新，
        // 不重建行节点。
        let keys: Vec<String> = c
            .iter()
            .enumerate()
            .map(|(idx, child)| {
                child
                    .get_id()
                    .clone()
                    .unwrap_or_else(|| format!("#{idx}"))
            })
            .collect();
        let lcx = ctx.clone();
        let source2 = source.clone();
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
                    let ctx = lcx.clone();
                    let src = source2.clone();
                    let item = item2.clone();
                    let k2 = key.clone();
                    // 按行 memo：list 变更只重算本行 Brick；输出相等则下游不动。
                    let memo = Memo::new(move |_| {
                        let l = ctx.slot_for_list(&src).get();
                        if let Some(pos) = k2.strip_prefix('#') {
                            let i: usize = pos.parse().ok()?;
                            l.get(i).cloned()
                        } else {
                            l.iter()
                                .find(|b| b.get_id().as_deref() == Some(k2.as_str()))
                                .cloned()
                        }
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
                                render_brick(&rctx, &template)
                            }
                            None => render_brick(&rctx, &child),
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
