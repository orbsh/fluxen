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
                // 自动滚动 = "开启 + 在底部"才跟随（PLAN §17）：旧判据
                // "离底部不到一个视口高"会把手动上滚一点的人拽回底部。
                // 旗标由只装一次的 scroll 监听维护（我们自己的滚动也保持它为
                // true）；上拖即停，拖回底部恢复。
                //
                // 触发面有两条，缺一不可：
                // ① 本函数所在的闭包重跑 = 列表槽被写（新行 append）；
                // ② 容器/子节点尺寸变化——打字机长高、模块宿主按 ADR 0012 撑开、
                //    图表带渲染，都不写列表槽，闭包不重跑。只靠 ① 的话，一整行
                //    内容流式长高的场景（walkthrough 的"一个 box"）永远不跟随。
                //    所以 ② 由 ResizeObserver（尺寸）+ 子树 MutationObserver（DOM/
                //    文字变化）补齐，判据仍是同一条"在底部才跟随"。
                //
                // 生长是分多帧到的（模块宿主撑开、图表带渲染），单发的"跳一下"会
                // 停在中间值上（实测落后 20~480px），故触发后走一个收敛循环：每帧
                // 重钉、高度连续两帧不变才停；旗标为"0"（用户上滚）立即停。
                crate::dom::eval(&format!(
                    r#"
                    var e = document.getElementById("{id_extra}");
                    if (e) {{
                        if (!e.dataset.scrollInit) {{
                            e.dataset.scrollInit = "1";
                            // 最后一次钉下的 scrollTop：用来认出"这次 scroll 是我们自己
                            // 造成的"——生长常晚于 scroll 事件派发，此刻按 live 值重算
                            // 会把"还在跟随"误判成用户上滚。
                            var expectTop = -1;
                            var chasing = false;
                            var pin = function () {{
                                if (e.dataset.atBottom === "0") {{ return; }}
                                var max = e.scrollHeight - e.clientHeight;
                                expectTop = max;
                                e.scrollTop = max;
                            }};
                            // 追赶循环：生长分多帧到（模块宿主撑开、图表带渲染、打字
                            // 机），单拍"跳一下"会停在中间值上。用 setTimeout 而不是
                            // rAF——headless/慢机上 rAF 实测被拖到秒级，追赶因此落后
                            // 数百 px。高度稳定三拍、或旗标转 "0"（用户上滚）、或超时
                            // 2s 即停；期间任何新变化再触发一轮。
                            var chase = function () {{
                                pin();
                                if (chasing) {{ return; }}
                                chasing = true;
                                var last = -1, stable = 0, t0 = Date.now();
                                var tick = function () {{
                                    pin();
                                    var now = e.scrollHeight - e.clientHeight;
                                    stable = (now === last) ? stable + 1 : 0;
                                    last = now;
                                    if (e.dataset.atBottom === "0" || stable >= 3 ||
                                        Date.now() - t0 > 2000) {{
                                        chasing = false;
                                        return;
                                    }}
                                    setTimeout(tick, 40);
                                }};
                                setTimeout(tick, 40);
                            }};
                            var atBottom = function () {{
                                // 落在底部、或落回刚钉下的位置 = 仍是跟随（用户自己
                                // 滚到底也落进第一条）；只有明显离开两者才算上滚。
                                var max = e.scrollHeight - e.clientHeight;
                                if (Math.abs(e.scrollTop - max) <= 2 ||
                                    (expectTop >= 0 && Math.abs(e.scrollTop - expectTop) <= 2)) {{
                                    e.dataset.atBottom = "1";
                                    return;
                                }}
                                e.dataset.atBottom = "0";
                            }};
                            var ro = new ResizeObserver(chase);
                            var observeChildren = function () {{
                                ro.disconnect();
                                ro.observe(e);
                                for (var i = 0; i < e.children.length; i++) {{
                                    ro.observe(e.children[i]);
                                }}
                            }};
                            new MutationObserver(function (recs) {{
                                for (var i = 0; i < recs.length; i++) {{
                                    if (recs[i].type === "childList") {{ observeChildren(); break; }}
                                }}
                                chase();
                            }}).observe(e, {{ childList: true, subtree: true,
                                              characterData: true, attributes: true }});
                            observeChildren();
                            e.addEventListener("scroll", atBottom, {{ passive: true }});
                            e.dataset.atBottom =
                                (e.scrollHeight - e.scrollTop - e.clientHeight <= 2) ? "1" : "0";
                        }}
                        chase();
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
