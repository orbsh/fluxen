pub mod ctx;
pub mod dom;
pub mod hooks;
pub mod render;

pub mod components;
pub mod widgets;

pub use ctx::Ctx;
use leptos::prelude::Get;

use content::codec::{ActiveCodec, CodecType};
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

/// 入口：解析 `#main` 的 host/token/codec/path，建立 WS，挂载响应式根，
/// 并把页面级上行把手（`window.fluxen.send`）交给页面上的 JS。
pub fn mount() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let loc = doc.location().unwrap();
    let mut host = "".to_owned();
    let mut token = None;
    let mut codec_type = CodecType::Cbor;
    let mut path = "/channel".to_owned();

    if let Ok(Some(ele)) = doc.query_selector("#main") {
        if let Some(h) = ele.get_attribute("data-host") {
            host = h;
        } else {
            host = loc.host().unwrap();
        }

        // 页面级配置：框架在 `#main` 上钉自己的通道路径与编码（宿主可能不是 `/channel`、
        // 也可能只认 JSON）。页面 URL 的查询串更具体，仍然优先。
        if let Some(p) = ele.get_attribute("data-path") {
            if p.starts_with('/') {
                path = p;
            } else {
                web_sys::console::warn_1(&JsValue::from_str(
                    "fluxen: data-path must start with '/' — keeping /channel",
                ));
            }
        }
        if let Some(c) = ele.get_attribute("data-codec")
            && let Ok(t) = c.parse::<CodecType>()
        {
            codec_type = t;
        }

        if let Ok(href) = loc.href()
            && let Ok(href) = web_sys::Url::new(&href)
        {
            if let Some(t) = href.search_params().get("token") {
                token = Some(t);
            }
            if let Some(codec_str) = href.search_params().get("codec") {
                if let Ok(t) = codec_str.parse::<CodecType>() {
                    codec_type = t;
                }
            }
        } else if let Some(t) = ele.get_attribute("data-token") {
            token = Some(t);
        }
    }

    let codec_str = match codec_type {
        CodecType::Json => "json",
        CodecType::Cbor => "cbor",
    };
    let query = if let Some(token) = token {
        format!("?token={}&codec={}", token, codec_str)
    } else {
        format!("?codec={}", codec_str)
    };
    let url = format!("ws://{}{}{}", host, path, query);
    let codec = ActiveCodec::new(codec_type);

    let parent = doc
        .query_selector("#main")
        .ok()
        .flatten()
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        .expect("no #main element to mount into");

    // mount_to 返回的 UnmountHandle 被 Drop 时会卸载视图——`let _ =` 会让界面挂上即被拆掉
    // （症状正是"ws 正常但界面空白"）。必须 forget 持有整个应用生命周期。
    leptos::mount::mount_to(parent, move || {
        let transport: std::rc::Rc<dyn transport::Transport> =
            std::rc::Rc::new(transport_ws::WsTransport::new(&url, codec.clone()));
        // 必须在 Leptos 的运行期里装配：`Ctx::new` 用 Executor::spawn_local，早于
        // mount_to 调用它会 panic（"before a global executor was initialized"）。
        let ctx = Ctx::new(transport, codec);
        publish_page_send(ctx.clone());
        // 响应式根：仅当 layout 变化时重建
        move || {
            let layout = ctx.layout.get();
            render::dispatch(&layout, &ctx)
        }
    })
    .forget();
}

/// 页面级上行把手：`window.fluxen.send(event, data)`。
///
/// 与 canvas 模块拿到的 `host.send` 是同一条腿——都落在 `Ctx::send` 上，
/// 页面上只有这一条上行通道。框架的客户端 runtime 用它发 hello 与 nav。
///
/// 连接还没打开时调用不会丢：`emit` 是 future，gloo 的 `Sink::poll_ready`
/// 在 `readyState == 0` 时挂起、`open` 时唤醒，所以这里只管 spawn。
fn publish_page_send(ctx: Ctx) {
    let send = Closure::<dyn FnMut(JsValue, JsValue)>::new(move |name: JsValue, data: JsValue| {
        let Some(name) = name.as_string() else {
            web_sys::console::warn_1(&JsValue::from_str(
                "fluxen.send: event name must be a string",
            ));
            return;
        };
        // `Outflow.data` 是 serde_json::Value，页面上给的是任意 JS 值：
        // undefined 之类的不可序列化值落成 null，不让它变成一个假帧。
        let data = js_sys::JSON::stringify(&data)
            .ok()
            .and_then(|text| serde_json::from_str(&String::from(text)).ok())
            .unwrap_or(Value::Null);
        let ctx = ctx.clone();
        leptos::task::spawn_local(async move {
            ctx.send(name, None, data).await;
        });
    });

    let handle = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &handle,
        &JsValue::from_str("send"),
        send.as_ref().unchecked_ref(),
    );
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Reflect::set(&window, &JsValue::from_str("fluxen"), &handle);
    }
    // 常驻到页面卸载：回收它等于把把手抽走（与 mount 的 forget 同理）。
    send.forget();
}
