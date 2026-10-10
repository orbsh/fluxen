use crate::Ctx;
use accrete::classify::Classify;
use accrete::{AccreteOps, Canvas, Chart};
use leptos::html::*;
use leptos::prelude::*;
use serde_json::Value;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// 外部渲染模块的原位挂载（ADR 0007 宿主 + ADR 0009 Chart 共用通道）。
///
/// 核心不知道模块是什么：url 指向 ES module，须导出
/// mount/update/resize/unmount 四函数；数据区（`bind["value"]` 惯例）
/// 以 CBOR 字节推给模块，语义完全归模块（3dbrowser 用
/// `{primitives, assets}`，g2chart 用 G2 spec）。渲染循环与输入事件归
/// 模块自己，宿主只提供容器、载荷变化通知与尺寸。
///
/// canvas 与 chart 的差异全在三个入参：url、容器样式（chart 用 class
/// 无显式尺寸）、其余同构——因此共享一个 mount_module，不复制挂载逻辑。
pub fn canvas_(accrete: Canvas, ctx: &Ctx, id: String) -> AnyView {
    let style = accrete
        .attrs
        .as_ref()
        .map(|a| a.size_style())
        .unwrap_or_default();
    let css = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.get_class().clone().map(|c| c.join(" ")))
        .unwrap_or_default();
    let (source, inline) = bind_value(&accrete);
    mount_module(&id, &accrete.url, style, css, source, inline, ctx)
}

/// G2 图表：载荷 = G2 spec（GoG 词汇的纯 JSON），渲染器 = url 指向的
/// 封装模块（ADR 0009，默认 /assets/g2chart/index.js）。eval 注入已
/// 退役——挂载走 canvas 同一条 import(url) 契约通道。
pub fn chart_(accrete: Chart, ctx: &Ctx, id: String) -> AnyView {
    let css = accrete
        .attrs
        .as_ref()
        .and_then(|a| a.get_class().clone().map(|c| c.join(" ")))
        .unwrap_or_default();
    let (source, inline) = bind_value(&accrete);
    mount_module(&id, &accrete.url, String::new(), css, source, inline, ctx)
}

/// `bind["value"]` → (source 槽名, inline default)。
fn bind_value(a: &impl AccreteOps) -> (Option<String>, Option<Value>) {
    let b = a.get_bind().and_then(|b| b.get("value")).cloned();
    match b {
        Some(accrete::Bind {
            variant: accrete::BindVariant::Source { source, .. },
            ..
        }) => (Some(source), None),
        Some(accrete::Bind { default, .. }) => (None, default),
        None => (None, None),
    }
}

/// 空载荷判据（ADR 0012）：宿主为缺失/空槽合成的那个值——`Null` 或零条目
/// map，即下文 `payload` 的"尚无数据"。空的 data 区 = 不占位：容器收起，
/// 载荷第一次非空时按节点尺寸撑开。除 `{}`/`Null` 之外的任何值（含空串、
/// 空数组）都算"有数据"，语义只看数据有没有，不猜内容。
fn payload_is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Object(m) => m.is_empty(),
        _ => false,
    }
}

/// 不占位时容器的样式（ADR 0012）。用内联样式而不是 `hide` 类：`.f` 与
/// `.hide` 同 div 实测互相压制（见 Pitfalls），内联无此歧义。
const HIDDEN_STYLE: &str = "display: none;";

/// 挂载全逻辑：解析载荷闭包 → import(url) → 契约检查 → mount →
/// update 订阅 → ResizeObserver → unmount。失败路径一律 warn + 空容器
/// （fail-loud，对齐 ADR 0003 惯例）。
fn mount_module(
    id: &str,
    url: &str,
    style: String,
    css: String,
    source: Option<String>,
    inline_default: Option<Value>,
    ctx: &Ctx,
) -> AnyView {
    // 载荷解析：inline default 直用；kind:source 读槽内节点（其
    // bind.value.default 优先，否则整个节点 JSON）。
    // 流式喂数据 = 对源槽 set/patch（ADR 0005 指针协议），零新机制。
    // 空槽/无载荷 = `{}`：模块的 spec 全字段有默认，空 map 是合法
    // "尚无数据"；喂 null 会让严格 decode 的模块 mount 即拒——而
    // "先挂空布局、后写数据槽"正是流式标准顺序（e2e 抓到的缺陷）。
    let empty = Value::Object(Default::default());
    let inline = inline_default.unwrap_or(Value::Null);
    let payload: Rc<dyn Fn() -> Value> = {
        let source = source.clone();
        let ctx = ctx.clone();
        let empty = empty.clone();
        Rc::new(move || {
            if let Some(src) = &source {
                return match ctx.slot_for_data(src).get_untracked().map(|a| (*a).clone()) {
                    Some(node) => node
                        .get_bind()
                        .and_then(|b| b.get("value"))
                        .and_then(|v| v.default.clone())
                        .unwrap_or_else(|| serde_json::to_value(&node).unwrap_or(empty.clone())),
                    None => empty.clone(),
                };
            }
            if inline.is_null() {
                empty.clone()
            } else {
                inline.clone()
            }
        })
    };

    // 占位随载荷生效（ADR 0012）：空的 data 区 = 不占位。判据同 `payload` 的
    // "尚无数据"（见 `payload_is_empty`）——这里只是把它从载荷语义延伸到占位
    // 语义，零新词汇、wire 不变。
    // 这里只给**构建那一刻**的值（模块挂载前、以及行重建后都由它定）；挂载
    // 之后的同步在下面 update Effect 里做：可响应式属性（`.style(闭包)`）要求
    // 闭包 `Send`，而判空必须读槽（`Ctx` 非 Send），所以走 Effect 的命令式写入。
    // 撑开不重挂：容器从隐藏转为可见、尺寸就位后，由模块自己的
    // ResizeObserver → resize() 接手重画，import 不重跑、GL 上下文不丢。
    let style_now = if payload_is_empty(&payload()) {
        HIDDEN_STYLE.to_string()
    } else {
        style.clone()
    };
    // 撑开时写回容器的值（节点尺寸），交给 update Effect。
    let size_style = style;

    let nr = NodeRef::<Div>::new();
    let el_id = id.to_string();
    let ctx2 = ctx.clone();
    let url = url.to_string();
    nr.on_load(move |el| {
        let el: web_sys::HtmlElement = el.dyn_into().expect("div is element");
        // 幂等护栏：原地 rebuild（兄弟节点 patch 引发的行 effect 重跑）会再次
        // 触发 NodeRef 的 load → on_load 重跑 → 模块被无谓地 unmount + mount
        // （canvas 换新、GL 状态丢失）。
        //
        // 状态机写在元素属性上：无 → "pending" → "true"，护栏只认 "true" 且
        // **容器里还有本模块写进去的内容**。只看标记会漏掉一类失效：行视图
        // 原地重建时会把宿主 div 的子节点按视图（空）重写，把模块挂进去的
        // canvas 抹掉（重建后 div 里只剩 tachys 的占位注释），此时标记若还
        // 在，护栏就直接跳过 → 永久空白。splatviewer 这类 CDN 模块 import
        // 慢，行重建必然落在挂载窗口内，所以只有它暴露；3dbrowser 是本地
        // wasm、毫秒级，重建落在挂载之后，从未踩到。
        // 内容没了就当没挂过，重新挂（模块 mount 自己会 replaceChildren）。
        let mounted_ok = el.get_attribute("data-module-mounted").as_deref() == Some("true")
            && el.child_element_count() > 0;
        if mounted_ok {
            return;
        }
        el.set_attribute("data-module-mounted", "pending")
            .expect("set mounted marker");
        let id = el_id.clone();
        let url = url.clone();
        let ctx2 = ctx2.clone();
        let payload0 = Rc::clone(&payload);
        let source0 = source.clone();
        leptos::task::spawn_local(async move {
            // mount 真正推给模块的载荷。下面 update 的基线必须是**它**，不能
            // 到那一步再重新读一次槽：槽写入若落在 import/mount 这个窗口里
            // （append 行与 set 载荷背靠背发出就是常态），重读会把新值当成
            // 基线，Effect 判定"没变化"→ update 永不触发 → 模块停在挂载时
            // 的空 spec 上。症状：宿主标了 mounted、canvas 在，资产却一个
            // 都不请求（本地 wasm 模块 import 窗口被写者间隔掩盖，CDN 模块必现）。
            let mounted_payload = payload0();
            // 初始数据（CBOR 字节）——mount 的 data 参数
            let data0 = encode(&mounted_payload);

            // 宿主通道：host.send(event, cborBytes) → 现有动作帧上行。
            // 闭包持 Rc<Ctx> + id，事件名与解码后的 JSON 一起交给
            // ctx.send（与表单 submit 同一条 uplink）。
            let host = js_sys::Object::new();
            let cb = Closure::<dyn FnMut(String, js_sys::Uint8Array)>::new(
                {
                    let ctx = ctx2.clone();
                    let id = id.clone();
                    move |ev: String, bytes: js_sys::Uint8Array| {
                        let buf = bytes.to_vec();
                        let decoded: Value = content::codec::ActiveCodec::Cbor
                            .decode(&buf)
                            .unwrap_or(Value::Null);
                        let ctx = ctx.clone();
                        let id = id.clone();
                        leptos::task::spawn_local(async move {
                            ctx.send(ev, Some(id), decoded).await;
                        });
                    }
                },
            );
            let _ = js_sys::Reflect::set(&host, &JsValue::from_str("send"), cb.as_ref());
            // cb 不 drop（closure 转成 JS 函数后被模块持有，泄漏归模块生命周期管）
            cb.into_js_value(); // 让所有权交给 JS，宿主不再 retain

            // 动态 import(url)。任何契约失败 = warn + 空容器（fail-loud，不猜）。
            let module = match import_module(&url).await {
                Some(m) => m,
                None => {
                    tracing::warn!("module host {id}: import {url:?} failed");
                    el.remove_attribute("data-module-mounted").ok();
                    return;
                }
            };
            let names = ["mount", "update", "resize", "unmount"];
            let mut contract: [Option<js_sys::Function>; 4] = [None, None, None, None];
            for (slot, name) in contract.iter_mut().zip(names) {
                *slot = js_sys::Reflect::get(&module, &JsValue::from_str(name))
                    .ok()
                    .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            }
            let [Some(mount), Some(update), Some(resize), Some(unmount)] = contract else {
                tracing::warn!("module host {id}: module {url:?} misses contract exports");
                el.remove_attribute("data-module-mounted").ok();
                return;
            };

            // mount(el, data, host) → ctx id（同步或 Promise<ctx>）
            let args = js_sys::Array::new();
            args.push(&JsValue::from(el.clone()));
            args.push(&data0.into());
            args.push(&JsValue::from(host));
            let ret = match js_sys::Reflect::apply(&mount, &JsValue::UNDEFINED, &args) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("module host {id}: mount threw: {e:?}");
                    el.remove_attribute("data-module-mounted").ok();
                    return;
                }
            };
            let cid: JsValue = if let Ok(p) = ret.clone().dyn_into::<js_sys::Promise>() {
                match wasm_bindgen_futures::JsFuture::from(p).await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("module host {id}: mount rejected: {e:?}");
                        el.remove_attribute("data-module-mounted").ok();
                        return;
                    }
                }
            } else {
                ret
            };
            // 挂载真正成功后才置 "true"（护栏只认它）；此前是 "pending" ——
            // 进行中的挂载不会被 rebuild 触发的 on_load 重复发起。
            el.set_attribute("data-module-mounted", "true").ok();

            // 数据区变化 → update(cid, cbor)：Effect 订阅源槽，比对载荷。
            {
                // 基线 = 挂载时推给模块的那份，不是此刻重读的槽值（见上）。
                let mut last = mounted_payload;
                let cid = cid.clone();
                let update = update.clone();
                let id = id.clone();
                let ctx = ctx2.clone();
                let src = source0.clone();
                let el_eff = el.clone();
                Effect::new(move |_| {
                    if let Some(src) = &src {
                        let _ = ctx.slot_for_data(src).get();
                    }
                    let p = payload0();
                    // ADR 0012：占位随载荷生效——空载荷收起容器，非空写回节点
                    // 尺寸（`size_style`）。每次运行都写（幂等）：载荷若在挂载
                    // 完成前就到了，构建那一刻的值还是"空"，而这一跑同时是
                    // p == last 的情形（进不了下面的 update 分支），它是唯一的
                    // 同步机会。撑开本身不重挂——尺寸变化由模块自己的
                    // ResizeObserver → resize() 接手。
                    let want = if payload_is_empty(&p) {
                        HIDDEN_STYLE.to_string()
                    } else {
                        size_style.clone()
                    };
                    el_eff.set_attribute("style", &want).ok();
                    if p != last {
                        let d = encode(&p);
                        last = p;
                        let a = js_sys::Array::new();
                        a.push(&cid);
                        a.push(&d.into());
                        if let Err(e) = js_sys::Reflect::apply(&update, &JsValue::UNDEFINED, &a) {
                            tracing::warn!("module host {id}: update threw: {e:?}");
                        }
                    }
                });
            }

            // 尺寸归宿主：ResizeObserver 观察容器（css 像素）→ resize(cid, w, h)。
            // dpr 换算与 canvas.width 设置归模块（见各 index.js）。
            {
                let cid_ro = cid.clone();
                let cb = Closure::<dyn FnMut(js_sys::Array, JsValue)>::new(
                    {
                        let resize = resize.clone();
                        let id = id.clone();
                        move |entries: js_sys::Array, _obs: JsValue| {
                            let e = js_sys::Reflect::get(&entries, &JsValue::from(0))
                                .ok()
                                .unwrap_or(JsValue::UNDEFINED);
                            let rect = js_sys::Reflect::get(&e, &JsValue::from_str("contentRect"))
                                .ok()
                                .unwrap_or(JsValue::UNDEFINED);
                            let num = |k: &str| {
                                js_sys::Reflect::get(&rect, &JsValue::from_str(k))
                                    .ok()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0)
                            };
                            let a = js_sys::Array::new();
                            a.push(&cid_ro);
                            a.push(&JsValue::from(num("width")));
                            a.push(&JsValue::from(num("height")));
                            if let Err(err) =
                                js_sys::Reflect::apply(&resize, &JsValue::UNDEFINED, &a)
                            {
                                tracing::warn!("module host {id}: resize threw: {err:?}");
                            }
                        }
                    },
                );
                let ro = web_sys::ResizeObserver::new(cb.as_ref().unchecked_ref());
                // ro 若不可用（旧浏览器）只是不转发尺寸，不影响挂载
                if let Ok(ro) = ro {
                    ro.observe(&el);
                    cb.forget(); // observer 长期持有 callback
                    let unmount = unmount.clone();
                    let ro_disconnect = ro.clone();
                    on_cleanup(move || {
                        ro_disconnect.disconnect();
                        let a = js_sys::Array::new();
                        a.push(&cid);
                        let _ = js_sys::Reflect::apply(&unmount, &JsValue::UNDEFINED, &a);
                    });
                } else {
                    let unmount = unmount.clone();
                    on_cleanup(move || {
                        let a = js_sys::Array::new();
                        a.push(&cid);
                        let _ = js_sys::Reflect::apply(&unmount, &JsValue::UNDEFINED, &a);
                    });
                }
            }
        });
    });

    div()
        .id(id)
        .class(css.as_str())
        .style(style_now)
        .node_ref(nr)
        .into_any()
}

/// `import(new URL(url, document.baseURI))`——相对 url 也能用（CDN 绝对地址直通）。
/// URL 解析整个放在 JS 侧完成：`new URL(...)` 返回 URL 对象而非字符串，
/// Rust 侧 dyn_into::<JsString>() 永远失败（e2e 首跑抓到的缺陷）。
async fn import_module(url: &str) -> Option<JsValue> {
    let escaped = url.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("import(new URL(\"{escaped}\", document.baseURI).href)");
    let p = match js_sys::eval(&script) {
        Ok(v) => match v.dyn_into::<js_sys::Promise>() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("canvas import: eval did not return a Promise for {script}");
                return None;
            }
        },
        Err(e) => {
            tracing::warn!(
                "canvas import: eval threw: {:?}",
                e.as_string().unwrap_or_else(|| format!("{e:?}"))
            );
            return None;
        }
    };
    match wasm_bindgen_futures::JsFuture::from(p).await {
        // 注意：ES module namespace 对象原型链为 null，dyn_into::<Object>()
        // （instanceof 语义）对它必然失败——保留 JsValue，契约查找走
        // Reflect::get，对任意接收者有效。
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(
                "canvas import: promise rejected: {:?}",
                e.as_string().unwrap_or_else(|| format!("{e:?}"))
            );
            None
        }
    }
}

/// Value → CBOR 字节（JS Uint8Array → wasm Vec<u8>）；边界传数据不传对象。
fn encode(v: &Value) -> js_sys::Uint8Array {
    let mut out = Vec::new();
    match ciborium::ser::into_writer(v, &mut out) {
        Ok(()) => js_sys::Uint8Array::from(&out[..]),
        Err(e) => {
            tracing::warn!("canvas: payload encode failed: {e}");
            js_sys::Uint8Array::new(&JsValue::NULL)
        }
    }
}
