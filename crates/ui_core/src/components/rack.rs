use crate::Ctx;
use crate::ctx::render_accrete;
use crate::hooks::{use_common_css, use_source_id};
use accrete::classify::Classify;
use accrete::{
    Accrete, AccreteOps, Rack, RackAttr, RackDirection, RackEnter, RackLayout, RackPause, RackPlay,
    RackScroll, RackView,
};
use leptos::html::*;
use leptos::leptos_dom::helpers::set_interval;
use leptos::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

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

/// Which row an exclusive rack has on stage. `newest` without a rotation means the last one: a
/// frame that says "one at a time, follow the newest" must not show an empty area because nobody
/// ever set a cursor.
fn stage_row(view: &RackView, active: Option<usize>, len: usize) -> Option<usize> {
    match view.layout {
        RackLayout::Exclusive => active.or_else(|| match view.play {
            RackPlay::Newest => len.checked_sub(1),
            RackPlay::Rotate => None,
        }),
        RackLayout::Fixed | RackLayout::Adaptive => None,
    }
}

/// Where a paged window starts, given the view's own cursor. ONE definition: the row window and
/// the paging controls both ask here, so what a dot says is what is on screen.
fn window_start(view: &RackView, active: Option<usize>, len: usize) -> usize {
    // Only `fixed` has a window; every other layout mounts what it mounts, so there is no start to
    // compute and a value here would only invite someone to read it.
    if view.layout != RackLayout::Fixed {
        return 0;
    }
    let n = view.count.unwrap_or(1).max(1);
    match active {
        Some(cur) => cur.min(len.saturating_sub(1)),
        // Without a cursor "fixed N" means the last N: the newest rows are the ones a reader cares
        // about.
        None if view.play == RackPlay::Newest => len.saturating_sub(n),
        None => 0,
    }
}

/// The paging controls a frame asked for (`scroll: indicator | arrow`). LOCAL state only — a click
/// moves the window and nothing is said to the sender, the same rule a rotation follows. Clicks
/// stop propagating because `pause: click` listens on the same container: paging is not pausing.
fn pager(
    view: &RackView,
    active: RwSignal<Option<usize>>,
    shown: RwSignal<usize>,
    from: usize,
    per_page: usize,
    paged: bool,
    len: usize,
) -> AnyView {
    // One page is not a choice, and an adaptive rack has no page to name.
    if !paged || per_page == 0 || len <= per_page {
        return ().into_any();
    }
    let pages = len.div_ceil(per_page);
    let current = match view.layout {
        RackLayout::Exclusive => stage_row(view, active.get(), len),
        RackLayout::Fixed => Some(from / per_page),
        RackLayout::Adaptive => None,
    };
    // A person who pages has SEEN what lies behind the new page, so the rotation resumes after it
    // rather than replaying the queue from the start.
    let go = move |page: usize| {
        let start = page * per_page;
        active.set(Some(start));
        shown.set(start + 1);
    };
    let mut kids: Vec<AnyView> = Vec::new();
    match view.scroll {
        RackScroll::Indicator => {
            for page in 0..pages {
                let live = current == Some(page);
                kids.push(
                    span()
                        .class(if live { "dot on" } else { "dot" })
                        .on(leptos::ev::click, move |e: leptos::ev::MouseEvent| {
                            e.stop_propagation();
                            go(page);
                        })
                        .into_any(),
                );
            }
        }
        RackScroll::Arrow => {
            let here = current.unwrap_or(0);
            let back = here.saturating_sub(1);
            let ahead = (here + 1).min(pages - 1);
            kids.push(
                button()
                    .class(if here == 0 { "arrow off" } else { "arrow" })
                    .on(leptos::ev::click, move |e: leptos::ev::MouseEvent| {
                        e.stop_propagation();
                        go(back);
                    })
                    .child("\u{2039}")
                    .into_any(),
            );
            kids.push(
                button()
                    .class(if here + 1 >= pages { "arrow off" } else { "arrow" })
                    .on(leptos::ev::click, move |e: leptos::ev::MouseEvent| {
                        e.stop_propagation();
                        go(ahead);
                    })
                    .child("\u{203a}")
                    .into_any(),
            );
        }
        RackScroll::None | RackScroll::Bar => {}
    }
    div().class("pager").child(kids).into_any()
}

/// 列表容器：按 selector 索引 `item` 模板，遍历 `ctx.list[source]` 渲染。
pub fn rack_(accrete: Rack, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    // 行 owner 的挂载点：本函数执行时的 reactive owner（rack 自身的作用域），
    // 与外层 list 更新解耦——行订阅随 rack 消亡，不随外层重建丢失。
    // 只在 leptos 的 render 作用域里调用：没有 owner，行订阅就无处挂。
    #[allow(clippy::expect_used)]
    let parent_owner = Owner::current().expect("rack_ requires a reactive owner");
    // THE VIEW (ADR 0024): how the same rows are laid out, how they enter, who plays them. The
    // defaults are exactly what a rack has always done, so a frame that says nothing about `view`
    // renders through the same path it always did — no wrapper node, no timer, no class.
    let view = accrete
        .attrs
        .as_ref()
        .and_then(|RackAttr { view, scroll, .. }| {
            // The old boolean keeps its meaning: it was "scrollbar, follow the tail".
            Some(match (view.clone(), *scroll) {
                (Some(v), true) => RackView {
                    scroll: RackScroll::Bar,
                    ..v
                },
                (Some(v), false) => v,
                (None, true) => RackView {
                    scroll: RackScroll::Bar,
                    ..RackView::default()
                },
                (None, false) => RackView::default(),
            })
        })
        .unwrap_or_default();
    // Rows earn a wrapper node only when the view needs a handle on them (a fade to run, an
    // opacity to toggle). Otherwise the tree is the one it has always been.
    let paints = view.layout != RackLayout::Adaptive || view.enter != RackEnter::None;

    // ONE KNOB FOR DIRECTION: the frame says `view.direction`, and it lands through the
    // `horizontal` field `use_common_css` already reads. Two mechanisms would let the frame say one
    // thing while the class says another.
    let mut accrete = accrete;
    if view.direction == RackDirection::Horizontal {
        accrete
            .attrs
            .get_or_insert_with(RackAttr::default)
            .horizontal = Some(true);
    }
    let accrete = accrete;

    let mut css = vec!["rack", "f"];
    match view.layout {
        // EXCLUSIVE: every row sits in the same grid cell, stacked, and only the current one is
        // opaque. Fading out is therefore CSS's job — no "keep the node a while after it is
        // removed" timing that nobody but the renderer could run.
        RackLayout::Exclusive => css.push("rot"),
        RackLayout::Fixed | RackLayout::Adaptive => {}
    }
    // `view.scroll` says what the container offers, so it decides the CSS that offers it: a bar is
    // the scroll container every existing frame already names by hand, and anything else clips
    // (a paged view must not scroll under its own controls).
    match view.scroll {
        RackScroll::Bar => css.push("scrolly"),
        RackScroll::None | RackScroll::Indicator | RackScroll::Arrow => css.push("noscroll"),
    }
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    let item: ItemContainer = accrete.item.clone().unwrap_or_default().into();
    let Some(source) = use_source_id(&accrete).cloned() else {
        return div().into_any();
    };
    // FOLLOW-THE-TAIL lives only where these three meet (the old chat path). A notice rack —
    // exclusive, rotating, no scrollbar — must not inherit "follow while within a screen of the
    // bottom".
    let follows = view.scroll == RackScroll::Bar
        && view.direction == RackDirection::Vertical
        && view.layout == RackLayout::Adaptive;

    // Rows per page: one at a time (exclusive), the declared window (fixed), or everything
    // (adaptive — where a page would name nothing, so paging controls are not offered).
    let per_page = match view.layout {
        RackLayout::Exclusive => 1,
        RackLayout::Fixed => view.count.unwrap_or(1).max(1),
        RackLayout::Adaptive => usize::MAX,
    };
    let paged = matches!(view.scroll, RackScroll::Indicator | RackScroll::Arrow)
        && view.layout != RackLayout::Adaptive;

    let slot = ctx.slot_for_list(&source);
    // 行位置索引：list 每次变更只建一遍，所有行 memo 共用。建在外层闭包之外
    // ——闭包每次重跑都会重建自身状态，索引必须比它活得久（索引与行 owner 同
    // 挂 rack 作用域，父渲染重跑时一起重建）。
    let index = Memo::new(move |_| std::sync::Arc::new(row_positions(&slot.get())));
    // How many rows there are, as a signal: a row's own closure outlives the render pass that built
    // it, so "which row is on stage" has to read the CURRENT length, not a captured one.
    let row_count = {
        let s = slot.clone();
        Memo::new(move |_| s.get().len())
    };

    // WHO IS ON STAGE: `None` means nobody. A notice area empties itself when its turn ends — the
    // rows stay in the slot, the renderer simply stops showing them. Rotation ends at `None`, never
    // wrapping back to the first row: a sentence already said should not return every so often.
    //
    // Rotation and pausing are the renderer's own state. The venue only sends rows; how long one
    // stays, when it is replaced, and what holds it still are decided here (2026-10-31).
    let active = RwSignal::new(None::<usize>);
    // How many rows have had their turn. Rotation walks the queue in the order it was said — a
    // second line arriving while the first is being read waits behind it, which is what a queue is
    // for; the only thing that jumps is an empty area meeting an unread row.
    let shown = RwSignal::new(0usize);
    let paused = RwSignal::new(false);
    let rotating = view.play == RackPlay::Rotate && view.layout != RackLayout::Adaptive;
    if rotating {
        let tick_slot = slot.clone();
        let every = view.every.unwrap_or(4000).max(200);
        set_interval(
            move || {
                if paused.get_untracked() {
                    return;
                }
                let len = tick_slot.get_untracked().len();
                let Some(cur) = active.get_untracked() else {
                    return;
                };
                if len == 0 {
                    active.set(None);
                } else if cur + 1 < len {
                    shown.set(cur + 1);
                    active.set(Some(cur + 1));
                } else {
                    shown.set(len);
                    active.set(None);
                }
            },
            Duration::from_millis(every),
        );
        // An empty area shows the next unread row at once, so a refusal never waits for a turn that
        // has nothing to wait behind.
        let watch = slot.clone();
        let last_len = RwSignal::new(0usize);
        Effect::new(move |_| {
            let len = watch.get().len();
            let before = last_len.get_untracked();
            last_len.set(len);
            if len < before {
                // A removal renumbers the rows underneath the cursor; start over rather than point
                // into the middle of a list that changed shape.
                shown.set(0);
                active.set(None);
                return;
            }
            if len > before && active.get_untracked().is_none() {
                let next = shown.get_untracked().min(len - 1);
                shown.set(next);
                active.set(Some(next));
            }
        });
    }
    move || -> AnyView {
        // 只订阅本 source 的槽：其他键的更新不触发本闭包
        let c = slot.get();
        // keyed list：行身份 = Accrete id（无 id 行按位置 "#{idx}" 兜底）。
        // 外层键集 diff：追帧只给新键建行，旧行节点存活；行内容做成
        // 反应式闭包再订阅 ctx.list，同 id 行的 merge 更新原地刷新，
        // 不重建行节点。
        // FIXED N: mount only the rows in the window (paging is driven by the index). Every other
        // layout mounts all of them — exclusive hides the rest with opacity, so the slot keeps the
        // whole history.
        let (from, to) = match view.layout {
            RackLayout::Fixed => {
                let start = window_start(&view, active.get(), c.len());
                (start, (start + per_page).min(c.len()))
            }
            _ => (0, c.len()),
        };
        let keys: Vec<String> = c
            .iter()
            .enumerate()
            .skip(from)
            .take(to.saturating_sub(from))
            .map(|(idx, child)| row_key(child, idx))
            .collect();
        let lcx = ctx.clone();
        let item2 = item.clone();
        // The row closures outlive this pass, so they get their own copy of the view (the outer
        // closure re-runs and must not have its `view` moved out from under it).
        let view_rows = view.clone();
        let po = parent_owner.clone();
        let children = leptos::tachys::view::keyed::keyed(
            keys,
            |k: &String| k.clone(),
            move |row_index: usize, key: String| {
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
                        let inner = match item.select(&child) {
                            Some(mut template) => {
                                // 模板外壳 + child 作为其 children
                                template.set_children(vec![child]);
                                render_accrete(&rctx, &template)
                            }
                            None => render_accrete(&rctx, &child),
                        };
                        if !paints {
                            return inner;
                        }
                        // The wrapper exists only to give the row a handle that can be switched:
                        // the enter animation plays once (on mount), and in an exclusive layout the
                        // show/hide is an opacity transition — both directions are CSS's.
                        let enter = match view.enter {
                            RackEnter::Fade => " enter-fade",
                            RackEnter::Translate => " enter-slide",
                            RackEnter::None => "",
                        };
                        let shown_by_index = view.layout == RackLayout::Exclusive;
                        let stage_view = view_rows.clone();
                        let cls = move || {
                            let is_active = !shown_by_index
                                || stage_row(&stage_view, active.get(), row_count.get())
                                    == Some(row_index);
                            format!("slide{enter}{}", if is_active { " active" } else { "" })
                        };
                        div().class(cls).child(inner).into_any()
                    }
                    .into_any()
                });
                (
                    move |_index: usize| {},
                    leptos::tachys::reactive_graph::OwnedView::new_with_owner(view, owner),
                )
            },
        );

        if follows {
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

        let root = div()
            .id(id.as_str())
            .class(css.as_str())
            .child(children)
            .child(pager(&view, active, shown, from, per_page, paged, c.len()));
        match (rotating, view.pause) {
            (true, RackPause::Click) => root
                .on(leptos::ev::click, move |_| {
                    paused.set(!paused.get_untracked());
                })
                .into_any(),
            (true, RackPause::Hover) => root
                .on(leptos::ev::mouseenter, move |_| paused.set(true))
                .on(leptos::ev::mouseleave, move |_| paused.set(false))
                .into_any(),
            _ => root.into_any(),
        }
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

    fn view(layout: RackLayout, play: RackPlay, count: Option<usize>) -> RackView {
        RackView {
            layout,
            play,
            count,
            ..RackView::default()
        }
    }

    #[test]
    fn one_at_a_time_follows_the_newest_row_without_a_cursor() {
        // A frame that says "exclusive + newest" and never rotates still has something to show:
        // the last row. An empty area would be a bug the sender cannot see.
        let v = view(RackLayout::Exclusive, RackPlay::Newest, None);
        assert_eq!(stage_row(&v, None, 3), Some(2));
        assert_eq!(stage_row(&v, None, 0), None, "no rows, nobody on stage");
    }

    #[test]
    fn rotation_starts_with_nobody_on_stage() {
        let v = view(RackLayout::Exclusive, RackPlay::Rotate, None);
        assert_eq!(stage_row(&v, None, 3), None, "the clock decides, not the frame");
        assert_eq!(stage_row(&v, Some(1), 3), Some(1), "a cursor outranks the policy");
    }

    #[test]
    fn a_fixed_window_defaults_to_the_newest_rows() {
        let v = view(RackLayout::Fixed, RackPlay::Newest, Some(2));
        assert_eq!(window_start(&v, None, 5), 3, "the last two of five");
        assert_eq!(window_start(&v, None, 1), 0, "never past the end");
        assert_eq!(window_start(&v, Some(9), 5), 4, "a stale cursor clamps");
        assert_eq!(window_start(&v, Some(2), 5), 2);
    }

    #[test]
    fn an_adaptive_rack_has_no_page_and_no_stage() {
        let v = view(RackLayout::Adaptive, RackPlay::Rotate, None);
        assert_eq!(stage_row(&v, Some(1), 3), None, "everything is on screen already");
        assert_eq!(window_start(&v, Some(1), 3), 0);
    }

    #[test]
    fn duplicate_keys_keep_the_first_hit() {
        // 同 id 两行：取首个，与原先 `iter().find` 一致
        let rows = [row(Some("dup")), row(Some("dup"))];
        assert_eq!(row_positions(&rows)["dup"], 0);
    }
}
