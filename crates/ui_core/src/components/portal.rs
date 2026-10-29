use crate::Ctx;
use crate::ctx::render_children;
use accrete::Portal;
use leptos::html::*;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;

/// 渲染位置传送（ADR 0014）：children 渲染到 `attrs.to` 选中的元素里，就地只留一个
/// 空锚点。只做位置——不碰显隐、不碰遮罩，那是 layer 的事。
///
/// 目标解析在布局信号上重跑：挂载点可能比 portal 晚出现，也可能被换成另一个元素。
/// 目标没解析到 = 不渲染 + warn（fail-loud，不猜）；目标换了就先放下旧挂载再挂新处。
/// `mount_to` 起新 owner，但槽信号都建在根 owner 下（见 Ctx::slot_for_*），不受影响。
///
/// Effect 闭包要 Send，而挂载句柄 / DOM 元素都不是：按 leptos::portal::Portal 的同款
/// 做法用 SendWrapper 包住（单线程 wasm，装进去只是过编译期这一关）。
pub fn portal_(accrete: Portal, ctx: &Ctx) -> AnyView {
    let to = accrete
        .attrs
        .as_ref()
        .map(|a| a.to.clone())
        .unwrap_or_default();
    let children: Vec<accrete::Accrete> = accrete.children.clone().unwrap_or_default();
    let cur: SendWrapper<Rc<RefCell<Option<web_sys::Element>>>> =
        SendWrapper::new(Rc::new(RefCell::new(None)));
    // 挂载句柄：UnmountHandle<N::State> 的量名取决于 children 合成的视图类型，这里只靠
    // 它"放下即卸载"（UnmountHandle 实现了 Drop → mountable.unmount()），故类型擦除成
    // Box<dyn Any>，不写死 State。
    let handle: SendWrapper<Rc<RefCell<Option<Box<dyn Any>>>>> =
        SendWrapper::new(Rc::new(RefCell::new(None)));

    {
        let ctx = ctx.clone();
        let cur = cur.clone();
        let handle = handle.clone();
        Effect::new(move |_| {
            // 订阅布局：目标随布局出现 / 更换
            let _ = ctx.layout.get();
            let found = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.query_selector(&to).ok().flatten());
            let same = match (found.as_ref(), cur.borrow().as_ref()) {
                (Some(a), Some(b)) => a == b,
                (None, None) => true,
                _ => false,
            };
            if same {
                return;
            }
            if let Some(h) = handle.borrow_mut().take() {
                drop(h); // Drop → unmount
            }
            *cur.borrow_mut() = found.clone();
            match found {
                Some(el) => {
                    // mount_to 要 web_sys::HtmlElement（按值）且 N::State: Mountable——
                    // AnyView 的 State 不满足，故 children 包在一个具体标签里。
                    let parent: web_sys::HtmlElement = el.unchecked_into();
                    let views = render_children(&ctx, &children);
                    let h = leptos::mount::mount_to(parent, move || div().child(views));
                    *handle.borrow_mut() = Some(Box::new(h));
                }
                None => tracing::warn!("portal: target {to:?} not found"),
            }
        });
    }
    {
        let handle = handle.clone();
        on_cleanup(move || {
            if let Some(h) = handle.borrow_mut().take() {
                drop(h);
            }
        });
    }
    span().into_any()
}