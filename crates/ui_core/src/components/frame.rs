//! 框：结构容器，也是数据区域的唯一形态（ADR 0027）。
//!
//! 这里原来有两个组件——`case`（画 children）和 `placeholder`（绑 `Source` 时
//! 画槽里的树，否则画 children）。它们不是两种东西：`placeholder` 的实现是
//! `case` 的超集，差别只有"绑了 source 读不读槽"。把它们分成两个组件等于要求
//! 作者在写布局时预判这块将来会不会被数据填——而今天静态、明天动态，类型就得
//! 跟着改。绑定能后加，类型不能。
//!
//! 所以只剩 `frame`：
//!
//!   bind.value.slot   槽有值 → 画槽里的那棵树（带一次淡入淡出），
//!                     槽空 → 画 children（默认内容）
//!   无 source          画 children
//!
//! ADR 0034 把拖拽的两端也收到这里：`attrs.draggable` 是源，`attrs.accept` + `sink`
//! 是落点。理由只有一条——货物是子树，而属性包不住子树；发射必须发生在货物落进的
//! 那个盒子上，而那个盒子就是这里。`group`/`grab`/`dock` 三个类型随之消失。
//!
//! 手势与飞行态来自 `Ctx.drag`：页面级会话，只有一份（源和落点隔着别的节点，本地
//! 信号到不了对面）。每个 frame 各存一份就是这条设计的反面。
//!
//! `attrs.grid` 与 `attrs.class` 两条路都保留：grid 时只进 `grid`、否则只进 `flex`，
//! 因为 `.flex` 与 `.grid` 在同一个层里谁在后谁赢，同时存在会把 `display:grid` 压掉
//! （`attrs.grid` 自出生起没生效过，没人用 grid 才没人踩到）。

use crate::Ctx;
use crate::rough;
use crate::ctx::{render_accrete, render_children};
use crate::hooks::use_common_css;
use accrete::{Accept, Accrete, AccreteOps, Bind, Frame};
use leptos::attr::any_attribute::AnyAttribute;
use leptos::html::*;
use leptos::prelude::*;
use serde_json::{Map, Value};

/// 槽名（`bind.value.slot`），没有绑定就是 `None`。
fn source_of(accrete: &Frame) -> Option<String> {
    accrete.get_source().and_then(|b| b.slot.clone())
}

/// 货物的标签：源框自己声明的 `draggable.tag`。会话里放的就是那个框节点，所以标签
/// 只有一份，不会和货物分家（ADR 0034）。
fn cargo_tag(cargo: &Accrete) -> Option<&str> {
    match cargo {
        Accrete::frame(f) => f.attrs.as_ref()?.draggable.as_ref()?.tag.as_deref(),
        _ => None,
    }
}

/// 这个落点收不收这件货：`accept` 没写 tag = 全收；写了就是闭集合比对，没有 tag 的
/// 货配不上任何一个写了 tag 的落点（配不上就是配不上，不发射、不高亮）。
fn accepts(accept: &Accept, cargo: &Accrete) -> bool {
    match &accept.tag {
        None => true,
        Some(want) => cargo_tag(cargo) == Some(want.as_str()),
    }
}

/// 该来的来，不该来的占位（`()` 不产生任何 DOM 属性）。源与落点各自独立：一个框
/// 可以只拖、只收，也可以既是源又是落点。
fn drag_attrs(
    ctx: &Ctx,
    cargo: &Accrete,
    draggable: &Option<accrete::Draggable>,
    accept: &Option<Accept>,
    sink: &Option<Bind>,
    id: &str,
    over: RwSignal<bool>,
) -> (
    AnyAttribute,
    AnyAttribute,
    AnyAttribute,
    AnyAttribute,
    AnyAttribute,
    AnyAttribute,
) {
    // 源：`dragstart` 只把货物写进页面级会话，不上报——上行发生在落点。
    let label = draggable
        .as_ref()
        .and_then(|d| d.tag.clone())
        .unwrap_or_else(|| "item".into());
    let on_start = {
        let (ctx, cargo) = (ctx.clone(), cargo.clone());
        leptos::ev::on(leptos::ev::dragstart, move |ev: web_sys::DragEvent| {
            // Firefox 不在 dataTransfer 里写东西就不认这次拖；货物本身走会话，
            // 不塞 dataTransfer（那儿只放一个能看的标签）。
            if let Some(dt) = ev.data_transfer() {
                dt.set_effect_allowed("move");
                let _ = dt.set_data("text/plain", &label);
            }
            ctx.drag.set(Some(cargo.clone()));
        })
    };
    let on_end = {
        let ctx = ctx.clone();
        leptos::ev::on(leptos::ev::dragend, move |_ev: web_sys::DragEvent| {
            ctx.drag.set(None)
        })
    };

    // 落点：dragenter 与 dragover 都要 preventDefault——只挡其中一个，浏览器仍会把
    // 这次释放当成"不可落"，drop 根本不触发。两处要挂到不同的事件描述符上（包装类型
    // 跟着事件走），判断本身只有一段，所以下面两份只差事件名。
    let on_enter = {
        let (ctx, accept) = (ctx.clone(), accept.clone());
        leptos::ev::on(leptos::ev::dragenter, move |ev: web_sys::DragEvent| {
            let ok = ctx
                .drag
                .get_untracked()
                .is_some_and(|c| accept.as_ref().is_some_and(|a| accepts(a, &c)));
            if ok {
                ev.prevent_default();
                over.set(true);
            }
        })
    };
    let on_over = {
        let (ctx, accept) = (ctx.clone(), accept.clone());
        leptos::ev::on(leptos::ev::dragover, move |ev: web_sys::DragEvent| {
            let ok = ctx
                .drag
                .get_untracked()
                .is_some_and(|c| accept.as_ref().is_some_and(|a| accepts(a, &c)));
            if ok {
                ev.prevent_default();
                over.set(true);
            }
        })
    };
    let on_leave = leptos::ev::on(leptos::ev::dragleave, move |_ev: web_sys::DragEvent| {
        over.set(false)
    });
    let on_drop = {
        let (ctx, accept, sink, id) = (ctx.clone(), accept.clone(), sink.clone(), id.to_string());
        leptos::ev::on(leptos::ev::drop, move |ev: web_sys::DragEvent| {
            ev.prevent_default();
            over.set(false);
            let Some(cargo) = ctx.drag.get_untracked() else {
                return;
            };
            if !accept.as_ref().is_some_and(|a| accepts(a, &cargo)) {
                return;
            }
            let mut data = Map::new();
            // 空 id 按无身份处理（与 `node_id` 同一约定），所以在出口这一处归一。
            if let Some(id) = cargo
                .get_id()
                .as_deref()
                .filter(|s| !s.is_empty())
            {
                data.insert("id".into(), Value::String(id.to_string()));
            }
            if let Some(tag) = cargo_tag(&cargo) {
                data.insert("tag".into(), Value::String(tag.to_string()));
            }
            if let Some(sink) = &sink {
                // 帧：`sink` = 落点声明的名字，`id` = 落点自己，`data` = {id?, tag?}
                // （空即无，ADR 0012）。
                ctx.emit(sink, Some(id.clone()), Value::Object(data));
            }
            // 正常路径上源的 dragend 会清；这里再清一次，免得 drop 之后有谁把会话
            // 留在"在飞"状态。
            ctx.drag.set(None);
        })
    };

    // 该来的来，不该来的占位（`()` 不产生任何 DOM 属性）。
    let attr = |on: bool, a: AnyAttribute| if on { a } else { ().into_any_attr() };
    (
        attr(draggable.is_some(), on_start.into_any_attr()),
        attr(draggable.is_some(), on_end.into_any_attr()),
        attr(accept.is_some(), on_enter.into_any_attr()),
        attr(accept.is_some(), on_over.into_any_attr()),
        attr(accept.is_some(), on_leave.into_any_attr()),
        attr(
            accept.is_some() && sink.is_some(),
            on_drop.into_any_attr(),
        ),
    )
}

pub fn frame_(accrete: Frame, ctx: &Ctx, id: String) -> AnyView {
    let ctx = ctx.clone();
    let attrs = accrete.attrs.clone().unwrap_or_default();
    // HTML5 只在元素自带 `draggable="true"` 时才启动拖拽；只挂 dragstart 处理器不够
    // （事件永远不会来，页面上的拖动是浏览器的选择文本）。有 `attrs.draggable` 才写这个属性。
    let drag_flag = attrs.draggable.is_some().then_some("true");
    let mut css = vec!["frame"];
    if let Some(id) = &accrete.id {
        css.push(id.as_str());
    }

    let mut style = String::new();
    let mut use_grid = false;
    if let Some(g) = &attrs.grid {
        use_grid = true;
        css.push("grid");
        style = g
            .iter()
            .map(|(k, v)| format!("{}: {};", k, v.as_str().unwrap_or("")))
            .collect::<Vec<String>>()
            .join("\n");
    }
    if !use_grid {
        css.push("flex");
    }
    // `group` 的内联样式：原样写进 style，与 grid 的轨道声明并存（后者先写）。
    if let Some(s) = &attrs.style {
        for (k, v) in s {
            style.push_str(&format!("{k}: {v};\n"));
        }
    }
    let is_target = attrs.accept.is_some() && accrete.sink.is_some();
    if is_target {
        css.push("drop");
    }
    use_common_css(&mut css, &accrete);
    let base = css.join(" ");

    let over = RwSignal::new(false);
    // 只给落点订阅会话：非落点的类名闭包连 `ctx.drag.get()` 都不会求值，因此不订阅
    // （短路求值，`watch` 为假时那个信号不在任何人的依赖里）。
    let class = {
        let (ctx, base) = (ctx.clone(), base.clone());
        move || {
            let mut cls = base.clone();
            if is_target && ctx.drag.get().is_some() {
                cls.push_str(" drop-armed");
            }
            if over.get() {
                cls.push_str(" drop-over");
            }
            cls
        }
    };

    let cargo = Accrete::frame(accrete.clone());
    // 三个通道的声明都搬进闭包：属性在每次重建时要重新造（闭包不能克隆），而帧本身
    // 在返回之后就不在了。
    let sink = accrete.sink.clone();
    let draggable = attrs.draggable.clone();
    let accept = attrs.accept.clone();
    let drag = move |ctx: &Ctx| drag_attrs(ctx, &cargo, &draggable, &accept, &sink, &id, over);

    let children = accrete.children.clone().unwrap_or_default();
    // The DOM id is the marker contract: a reserved-id node (`reflux:path` and
    // friends) is read back with `getElementById`, and `placeholder` carried the
    // id before the merge (ADR 0027 keeps its behaviour). The class token above
    // stays — it is what CSS addressed — but a class is not addressable by id.
    let dom_id = accrete.id.clone();
    // The sketch skin (ADR 0038 §4): only a frame that asked for a border has this layer,
    // and only while `config.skin = sketch` — in normal mode there is no element and no
    // observer.
    let sketch = {
        let class = attrs.class.clone();
        let id = accrete.id.clone();
        move |c: &Ctx| rough::border_layer(c, class.as_ref(), id.as_deref())
    };
    let Some(source) = source_of(&accrete) else {
        let view = div()
            .attr("class", class)
            .attr("draggable", drag_flag)
            .style(style)
            .add_any_attr(drag(&ctx))
            .child(render_children(&ctx, &children))
            .child(sketch(&ctx));
        return match dom_id {
            Some(id) => view.id(id).into_any(),
            None => view.into_any(),
        };
    };

    // 数据区域：整个容器是槽的镜子。`is-fade-in-and-out` 是 core.css 里的一次性
    // 动画——元素随槽值重建，动画自己重播，不需要 JS 去加/删类。
    let inner_ctx = ctx.clone();
    move || -> AnyView {
        match inner_ctx.slot_for_data(&source).get() {
            Some(data) => {
                let view = div()
                    .attr("class", format!("{} is-fade-in-and-out", base))
                    .attr("draggable", drag_flag)
                    .style(style.clone())
                    .add_any_attr(drag(&inner_ctx))
                    .child(render_accrete(&inner_ctx, &data))
                    .child(sketch(&inner_ctx));
                match dom_id.clone() {
                    Some(id) => view.id(id).into_any(),
                    None => view.into_any(),
                }
            }
            None => {
                let view = div()
                    .attr("class", class.clone())
                    .attr("draggable", drag_flag)
                    .style(style.clone())
                    .add_any_attr(drag(&inner_ctx))
                    .child(render_children(&inner_ctx, &children))
                    .child(sketch(&inner_ctx));
                match dom_id.clone() {
                    Some(id) => view.id(id).into_any(),
                    None => view.into_any(),
                }
            }
        }
    }
    .into_any()
}