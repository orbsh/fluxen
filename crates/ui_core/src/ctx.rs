use crate::hooks::BundleState;
use crate::render::dispatch;
use accrete::{Accrete, AccreteOps};
use content::codec::ActiveCodec;
use content::{Content, PatchKind};
use leptos::prelude::*;
use serde_json::{Map, Value, to_value};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::{LazyLock, RwLock};
use transport::Transport;

/// 已注册的 stamp 定义表（ADR 0037：定义是节点树，值是整值替换）。
static TMPL: LazyLock<RwLock<accrete::template::Templates>> =
    LazyLock::new(|| RwLock::new(accrete::template::Templates::new()));

/// 全局共享状态容器：掌管布局、数据、列表与传输收发。
///
/// `bundle` is the render-time environment: `bundle_` injects its own
/// `BundleState` and clones `Ctx` for the subtree, so ownership follows the
/// clone chain; sibling branches each keep their own clone and cannot pollute
/// each other. Nested bundles shadow the outer one.
/// per-key 信号槽：外层 map 只存句柄（键集变化时才写外层），
/// 值经内层信号发布——某个键的更新不通知其他键的订阅者。
/// 列表槽：一个共享的、不可变快照。
///
/// 外层 `Arc` 让渲染闭包抱着整个列表而不拷贝它；内层 `Arc<Accrete>` 让「加一行」只需要复制
/// 指针，不必把已有的每一行都深拷贝一遍——那曾使每次 append 都 O(rows) 深拷贝，消息越多越慢
/// （O(n²)）。行的内容是共享的，所以改动一行要 `Arc::make_mut`（独占就原地改，共享时克隆这一行）。
pub type ListSlot = RwSignal<std::sync::Arc<Vec<std::sync::Arc<Accrete>>>>;
pub type DataSlot = RwSignal<Option<std::sync::Arc<Accrete>>>;
/// 裸 Value 槽（ADR 0008 local 平面）：事件数据不是展示形态，
/// 与 data（Accrete）分平面。
pub type ValueSlot = RwSignal<Option<Value>>;

/// 一个页面选项的两半（ADR 0038 §2）：`local` 是页面自己的选择，`remote` 是远端给的默认。
/// 生效值本地优先、远端回落——与 `both` 的读法是同一条规则（ADR 0031 §1）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigValue {
    pub local: Option<Value>,
    pub remote: Option<Value>,
}

impl ConfigValue {
    /// 生效值：页面选过就是页面的，否则是远端的；两半都空 = 组件自己的默认。
    pub fn effective(&self) -> Option<&Value> {
        self.local.as_ref().or(self.remote.as_ref())
    }
}

/// 一个选项的槽：per-key 信号，写一个选项不通知别的选项的订阅者。
pub type ConfigSlot = RwSignal<ConfigValue>;

#[derive(Clone)]
pub struct Ctx {
    pub transport: LeptosTransport,
    pub codec: ActiveCodec,
    pub layout: RwSignal<Accrete>,
    pub data: RwSignal<HashMap<String, DataSlot>>,
    pub list: RwSignal<HashMap<String, ListSlot>>,
    /// local 平面：`kind: local` 事件落点与订阅共用的具名 Value 槽。
    pub vals: RwSignal<HashMap<String, ValueSlot>>,
    /// 页面选项（ADR 0038 §2）：自己的命名空间，一个选项一个信号、local/remote 两半。
    /// 它不是 slot 平面的一员——slot 是应用的名字，混在一起就答不出"哪些名字是配置"。
    pub config: RwSignal<HashMap<String, ConfigSlot>>,
    /// 拖拽会话（ADR 0034）：`Some` 即「有拖拽在飞」，里面放的就是那个源框节点
    /// 本身——货物是它自己的子树，所以不再有第二份平行的结构（复制体和原体迟早
    /// 对不上）。这是渲染器里唯一一份跨组件的瞬态态：源和落点隔着别的节点，本地信号
    /// 到不了对面。它不进任何槽、不上线：「谁在飞、悬在谁上面」是浏览器的事实，不是
    /// 生产者的树。只能有这一份，组件各自维护一份就是这条设计的反面。
    pub drag: RwSignal<Option<Accrete>>,
    /// 槽位的永久 owner：Ctx::new 时捕获的 app 根作用域。
    /// 槽若建在消费帧的 Effect 作用域里，Effect 重跑会 dispose 其存储，
    /// 外层 map 里留下的就是死句柄（panic: already disposed）。
    pub(crate) owner: Owner,
    pub bundle: Option<Arc<BundleState>>,
}

/// UI 侧持有传输 + 下行帧信号。wasm 单线程，Rc 共享。
#[derive(Clone)]
pub struct LeptosTransport {
    /// wasm 单线程，用 SendWrapper 让 Rc 满足 leptos 组件闭包的 Send 约束
    /// （与原 WebSocketHandle 的做法一致）。
    pub inner: send_wrapper::SendWrapper<std::rc::Rc<dyn Transport>>,
    /// 尚未处理的**一批**下行帧；桥接自 transport.on() 读循环。
    ///
    /// 队列而不是"最近一帧"：读循环一次可以连续取到多帧（无界 channel 永远 ready），
    /// Effect 要等微任务边界才跑。用最新值信号时后到的帧会覆盖先到的，症状是"连上就推的
    /// 布局丢了、界面空白"——那是丢帧，不是渲染问题。顺序在这里是语义：布局必须先于
    /// 它的后续 set/append 被应用。
    pub frame: RwSignal<Vec<Vec<u8>>>,
}

impl Ctx {
    /// 装配：传入已连接的 transport（由 lib.rs 按 feature 选择实现）。
    /// 框架桥接：`spawn_local` 读流写 `frame` 信号，Effect 消费解码分发。
    /// # Panics
    /// 只在 leptos 的 owner 作用域里构造（mount/render 期）；没有 owner 就
    /// 没有响应式图，构造本身无从谈起——这是运行期不变量，不是可返回的错误。
    #[allow(clippy::expect_used)]
    pub fn new(transport: std::rc::Rc<dyn Transport>, codec: ActiveCodec) -> Self {
        let frame = RwSignal::new(Vec::new());
        let layout = RwSignal::new(Accrete::text(Default::default()));
        let data = RwSignal::new(HashMap::new());
        let list = RwSignal::new(HashMap::new());
        let vals = RwSignal::new(HashMap::new());
        let config = RwSignal::new(HashMap::new());
        let drag = RwSignal::new(None);

        // 读循环：transport.on() -> frame 信号
        {
            let mut stream = transport.on();
            let frame_writer = frame;
            leptos::task::spawn_local(async move {
                use futures::StreamExt;
                while let Some(bytes) = stream.next().await {
                    frame_writer.update(|q| q.push(bytes));
                }
            });
        }

        let ctx = Ctx {
            transport: LeptosTransport {
                inner: send_wrapper::SendWrapper::new(transport),
                frame,
            },
            codec,
            layout,
            data,
            list,
            vals,
            config,
            drag,
            owner: Owner::current().expect("Ctx::new requires a reactive owner"),
            bundle: None,
        };

        // 消费下行帧并分发
        let ctx_clone = ctx.clone();
        Effect::new(move |_| {
            let batch = ctx_clone.transport.frame.get();
            if batch.is_empty() {
                return;
            }
            // 取走整批再分发：清空要在分发前，分发本身会再触发本 Effect，空批直接返回。
            ctx_clone.transport.frame.set(Vec::new());
            for b in batch {
                // 收侧双格式自适应：帧首字节定格式，与 ?codec= 钉定的发送格式无关；
                // 解出来的一帧就是动作列表本身——信封是宿主的（ADR 0021 决策 6）。
                match ctx_clone.codec.decode_auto::<Vec<Content<Accrete>>>(&b) {
                    Ok(actions) => dispatch_msg(&actions, &ctx_clone),
                    // decode 失败若静默丢弃，症状就是"ws 正常但界面空白"——必须留痕
                    Err(e) => tracing::error!("ws frame decode failed: {e}"),
                }
            }
        });

        ctx
    }

    pub async fn send(&self, event: impl AsRef<str>, id: Option<String>, content: Value) {
        self.transport
            .inner
            .emit(transport::Event {
                name: event.as_ref().to_string(),
                id,
                data: content,
            })
            .await;
    }

    pub fn set(&self, name: impl AsRef<str>, accrete: Accrete) {
        self.slot_for_data(name.as_ref())
            .set(Some(Arc::new(accrete)));
    }

    /// One way out for every write (ADR 0031 §2): the sink's `scope` decides the
    /// destination, and nothing else does. `local` assigns in this page and
    /// wraps the payload as `Outflow {dest: slot, id?, data}` so a subscriber
    /// cannot tell a remote write from an in-page emit (ADR 0010/0011);
    /// `remote` is the existing upward event; `both` does the two.
    /// False when the sink has no address — the caller keeps its own fallback.
    pub fn emit(&self, sink: &accrete::Bind, id: Option<String>, payload: Value) -> bool {
        let Some(slot) = sink.slot.clone() else {
            return false;
        };
        if sink.scope.local() {
            let wrapped = to_value(&content::Outflow {
                dest: slot.clone(),
                id: id.clone(),
                data: payload.clone(),
            })
            .unwrap_or(Value::Null);
            self.slot_for_value(&slot).set(Some(wrapped));
        }
        if sink.scope.remote() {
            let ctx = self.clone();
            leptos::task::spawn_local(async move {
                ctx.send(slot, id, payload).await;
            });
        }
        true
    }

    /// 取（或首次时建）某 source 的列表槽。外层 map 用 untracked 读：
    /// 订阅本槽即可，键集变化不该惊动 reader。
    pub fn slot_for_list(&self, source: &str) -> ListSlot {
        if let Some(s) = self.list.get_untracked().get(source).copied() {
            return s;
        }
        // 建在根 owner 下：存储与 Ctx 同寿，Effect/render 作用域轮换不会 dispose 它
        let slot = self
            .owner
            .with(|| RwSignal::new(std::sync::Arc::new(Vec::new())));
        let name = source.to_string();
        self.list.update(|m| {
            m.entry(name).or_insert(slot);
        });
        self.list
            .get_untracked()
            .get(source)
            .copied()
            .unwrap_or(slot)
    }

    /// 取（或首次时建）某 source 的数据槽（语义同 slot_for_list）。
    pub fn slot_for_data(&self, source: &str) -> DataSlot {
        if let Some(s) = self.data.get_untracked().get(source).copied() {
            return s;
        }
        let slot = self.owner.with(|| RwSignal::new(None));
        let name = source.to_string();
        self.data.update(|m| {
            m.entry(name).or_insert(slot);
        });
        self.data
            .get_untracked()
            .get(source)
            .copied()
            .unwrap_or(slot)
    }

    /// 取（或首次时建）某 slot 的本地值槽（ADR 0008 local 平面，
    /// 语义同 slot_for_data——per-key 信号、建在根 owner 下）。
    pub fn slot_for_value(&self, source: &str) -> ValueSlot {
        if let Some(s) = self.vals.get_untracked().get(source).copied() {
            return s;
        }
        let slot = self.owner.with(|| RwSignal::new(None));
        let name = source.to_string();
        self.vals.update(|m| {
            m.entry(name).or_insert(slot);
        });
        self.vals
            .get_untracked()
            .get(source)
            .copied()
            .unwrap_or(slot)
    }

    /// 取（或首次时建）某选项的槽（语义同 slot_for_value——per-key 信号、建在根 owner 下）。
    /// 读一个还没出现过的选项会为它建槽：没有槽就无从订阅，而"没写过"与"不存在"在这里
    /// 本来就是同一种状态，区别只在值是不是空。
    pub fn config_slot(&self, name: &str) -> ConfigSlot {
        if let Some(s) = self.config.get_untracked().get(name).copied() {
            return s;
        }
        let slot = self.owner.with(|| RwSignal::new(ConfigValue::default()));
        let key = name.to_string();
        self.config.update(|m| {
            m.entry(key).or_insert(slot);
        });
        self.config
            .get_untracked()
            .get(name)
            .copied()
            .unwrap_or(slot)
    }

    /// 生效的选项值（本地优先、远端回落）。读即订阅这一个选项——别的选项变了不重跑。
    pub fn config_get(&self, name: &str) -> Option<Value> {
        self.config_slot(name).get().effective().cloned()
    }

    /// 页面自己的写法（ADR 0038 §2）：指针第一段是选项名，其余写进这个选项的 `local`
    /// 半边；选项级的 `null` 清掉本地半边，把选项交还给远端。写会创建路径上的容器——
    /// 写有路径，没有"打错目标"这回事（`patch` 的拒绝语义是另一件事）。
    pub fn set_config(&self, path: &str, value: Value) -> Result<(), String> {
        let segs = pointer_segments(path)?;
        let Some((name, rest)) = segs.split_first() else {
            return Err(format!("config path names no option: {path:?}"));
        };
        let slot = self.config_slot(name);
        let cur = slot.get_untracked();
        let local = if rest.is_empty() && value.is_null() {
            None
        } else {
            let mut acc = cur.local.clone().unwrap_or(Value::Null);
            write_into(&mut acc, rest, value)?;
            Some(acc)
        };
        let next = ConfigValue {
            local,
            remote: cur.remote.clone(),
        };
        if next != cur {
            slot.set(next);
        }
        Ok(())
    }

    /// 把每个选项镜像成文档根上的 `data-<name>`（ADR 0038 §3）：一个选项一个 effect，
    /// 写了设属性、清了摘属性，别的选项不跟着重写。选项名原样进属性名（`skin` →
    /// `data-skin`），所以 CSS 只认一个选择器。
    pub fn mirror_config(&self) {
        let Some(root) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
        else {
            tracing::warn!("mirror_config: no document root to mirror onto");
            return;
        };
        // 键集变了才长 effect：已经挂上的选项继续用自己那份跟值。
        let seen: std::rc::Rc<std::cell::RefCell<std::collections::HashSet<String>>> =
            Default::default();
        let ctx = self.clone();
        let owner = self.owner.clone();
        Effect::new(move |_| {
            for name in ctx.config.get().keys().cloned().collect::<Vec<_>>() {
                if !seen.borrow_mut().insert(name.clone()) {
                    continue;
                }
                let slot = ctx.config_slot(&name);
                let el = root.clone();
                let attr = format!("data-{name}");
                owner.with(|| {
                    let el = el.clone();
                    let attr = attr.clone();
                    Effect::new(move |_| {
                        // 只订阅这一个选项
                        let v = slot.get().effective().cloned();
                        match v {
                            Some(v) => {
                                let text = match &v {
                                    Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                };
                                if el.set_attribute(&attr, &text).is_err() {
                                    tracing::warn!("mirror_config: cannot set {attr}");
                                }
                            }
                            None => {
                                el.remove_attribute(&attr).ok();
                            }
                        }
                    });
                });
            }
        });
    }
}

/// 一帧里的一个成员落进远程半边：`null` 清掉它，值没变就返回 None（不写、不通知）。
fn merge_remote(cur: &ConfigValue, incoming: &Value) -> Option<ConfigValue> {
    let next = ConfigValue {
        local: cur.local.clone(),
        remote: if incoming.is_null() {
            None
        } else {
            Some(incoming.clone())
        },
    };
    (next != *cur).then_some(next)
}

/// JSON Pointer（RFC 6901）分段，`~1`/`~0` 反转义——与 `Accrete::with_pointer` 同一套语法。
fn pointer_segments(path: &str) -> Result<Vec<String>, String> {
    if !path.starts_with('/') {
        return Err(format!("config path must start with '/': {path:?}"));
    }
    Ok(path[1..]
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect())
}

/// 把值写进 `segs` 指定的位置，缺的容器按需长出来。空路径 = 整值替换。
fn write_into(root: &mut Value, segs: &[String], value: Value) -> Result<(), String> {
    let Some((seg, rest)) = segs.split_first() else {
        *root = value;
        return Ok(());
    };
    match root {
        Value::Object(map) => {
            let child = map.entry(seg.clone()).or_insert(Value::Null);
            write_into(child, rest, value)
        }
        Value::Array(arr) => {
            let at: usize = seg
                .parse()
                .map_err(|_| format!("config path segment {seg:?} is not an array index"))?;
            let child = arr
                .get_mut(at)
                .ok_or_else(|| format!("config path index {at} is out of range"))?;
            write_into(child, rest, value)
        }
        Value::Null => {
            // 没写过 = 空：在这一段长出一个对象，再往下走
            *root = Value::Object(Map::new());
            write_into(root, segs, value)
        }
        _ => Err(format!("config path hits a scalar at segment {seg:?}")),
    }
}

fn dispatch_msg(actions: &[Content<Accrete>], ctx: &Ctx) {
    // 每帧就是动作列表：没有消息级 `ev`，也就没有"这帧是不是给我"的闸
    // （ADR 0021 决策 6——信封连同它的 ev 归宿主）。`Influx.slot` 是数据槽名，
    // 属渲染内部寻址，与帧的形状无关。
    for c in actions {
        match c {
            Content::Tmpl(x) => {
                // 注册即校验（ADR 0037）：树上每个 `$name` 与 params 必须一一对上，
                // 定义是一次解析出来的树。拒绝时定义不入表——下一次同名注册仍是
                // 一次干净的尝试，而不是悄悄沿用上一份。
                if let Err(e) = TMPL
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .register(&x.name, &x.params.iter().map(String::as_str).collect::<Vec<_>>(), &x.data)
                {
                    tracing::warn!("{e}");
                }
            }
            Content::Create(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().unwrap_or_else(std::sync::PoisonError::into_inner);
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                tracing::info!("create: layout set, root = {:.100?}", d);
                // Effect 内写信号一律 set/untracked；tracked 读见上注释
                ctx.layout.set(d);
            }
            Content::Set(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().unwrap_or_else(std::sync::PoisonError::into_inner);
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                // 环的一个端点可以在这里出现（值里引用了某个槽）；另一个端点在别处写入。
                // 每次写入都问一次：这个值会不会让渲染绕回自己。
                if let Some(path) = slots::note(&x.slot, &d, true) {
                    tracing::error!(
                        "slot cycle {} — the value would render itself; dropped",
                        path.join(" -> ")
                    );
                    continue;
                }
                ctx.slot_for_data(&x.slot).set(Some(Arc::new(d)));
            }
            Content::Append(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().unwrap_or_else(std::sync::PoisonError::into_inner);
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                // untracked 读：dispatch_msg 运行在消费 frame 的 Effect 里，
                // tracked 读会让 Effect 订阅自己写入的槽 → 自激循环。
                let slot = ctx.slot_for_list(&x.slot);
                let cur = slot.get_untracked();
                let mut list = std::sync::Arc::unwrap_or_clone(cur);
                // 行身份在数据里（ADR 0005：`data.id`）；空串 id = 位置行
                // （无身份，可重复追加），与 DSL 的 no-id 约定一致
                let id = d.get_id().clone().filter(|s| !s.is_empty());
                if let Some(id) = &id {
                    // 行 id 唯一性在边界守住（ADR 0005）：重复 append 拒收，
                    // keyed 渲染身份不留给下游打架。
                    if list.iter().any(|i| i.get_id().as_deref() == Some(id.as_str())) {
                        tracing::warn!(
                            "append rejected: event {:?} already has row id {:?}",
                            x.slot,
                            id
                        );
                        continue;
                    }
                }
                // 落点：0 = 末尾；正数从尾往内数（+1 = 最后一个之前）；负数从头往内数
                // （-1 = 头部）。域外拒收并计数——夹取会把行放到调用方没要求的位置。
                let Some(spot) = content::append_index(list.len(), x.position) else {
                    tracing::warn!(
                        "append rejected: event {:?} has {} rows, position {} is out of range",
                        x.slot,
                        list.len(),
                        x.position
                    );
                    continue;
                };
                if let Some(path) = slots::note(&x.slot, &d, false) {
                    tracing::error!(
                        "slot cycle {} — the row would render itself; append dropped",
                        path.join(" -> ")
                    );
                    continue;
                }
                list.insert(spot, std::sync::Arc::new(d));
                slot.set(Arc::new(list));
            }
            Content::Remove(x) => {
                let slot = ctx.slot_for_list(&x.slot);
                let cur = slot.get_untracked();
                let mut list = std::sync::Arc::unwrap_or_clone(cur);
                let before = list.len();
                list.retain(|i| i.get_id().as_deref() != Some(x.id.as_str()));
                if list.len() != before {
                    slot.set(Arc::new(list));
                } else {
                    tracing::warn!("remove: slot {:?} has no row id {:?}", x.slot, x.id);
                }
            }
            Content::Patch(x) => {
                // 平面路由（ADR 0005）："" = layout 根；id 设 = 列表行；
                // 其余 = 具名数据槽。
                let result = if x.slot.is_empty() {
                    let mut d = ctx.layout.get_untracked();
                    let r = apply_patch(&mut d, x);
                    // 写回 layout 信号，否则响应式根（lib.rs tracked get）永不重建
                    if r.is_ok() {
                        ctx.layout.set(d);
                    }
                    r
                } else if let Some(id) = &x.id {
                    let slot = ctx.slot_for_list(&x.slot);
                    let cur = slot.get_untracked();
                    let mut list = std::sync::Arc::unwrap_or_clone(cur);
                    let r = match list
                        .iter_mut()
                        .find(|i| i.get_id().as_deref() == Some(id.as_str()))
                    {
                        // 行是共享的：独占时原地改，别处还持有时只克隆这一行。
                        Some(row) => apply_patch(std::sync::Arc::make_mut(row), x),
                        None => Err(format!("no row {id:?} in {:?}", x.slot)),
                    };
                    if r.is_ok() {
                        slot.set(Arc::new(list));
                    }
                    r
                } else {
                    let slot = ctx.slot_for_data(&x.slot);
                    match slot.get_untracked().map(|a| std::sync::Arc::unwrap_or_clone(a)) {
                        Some(mut d) => {
                            let r = apply_patch(&mut d, x);
                            if r.is_ok() {
                                slot.set(Some(Arc::new(d)));
                            }
                            r
                        }
                        None => Err(format!("no data slot {:?}", x.slot)),
                    }
                };
                if let Err(e) = result {
                    // 未命中是合法降级（补丁可能先于其 create 到达），warn 保证可观测。
                    tracing::warn!("patch {x:?} skipped: {e}");
                }
            }
            Content::Config(x) => {
                // 页面选项（ADR 0038 §2）：按成员合并，且只写远程半边——一帧永远不覆盖
                // 页面自己的选择（ADR 0031 §1 给 `both` 定的优先级）。
                for (name, v) in &x.data {
                    let slot = ctx.config_slot(name);
                    let cur = slot.get_untracked();
                    if let Some(next) = merge_remote(&cur, v) {
                        tracing::info!("config {name:?} = {:.120?}", v);
                        slot.set(next);
                    }
                }
            }
            Content::Empty => {}
        }
    }
}

/// Route a patch's op onto the target Accrete node (Value-domain addressing).
fn apply_patch(target: &mut Accrete, p: &content::PatchOp) -> Result<(), String> {
    match p.op {
        PatchKind::Replace => target.replace_at(&p.path, p.value.clone()),
        PatchKind::Append => target.append_at(&p.path, p.value.clone()),
    }
}

/// Slot references, and the cycle they would close.
///
/// A slot's value that renders a node reading slot T gives the edge S → T. If the graph has a
/// cycle, rendering never finishes: S draws T, T draws back to S, and the wasm stack dies somewhere
/// unrelated (measured: `RangeError: Maximum call stack size exceeded` reported inside a navbar,
/// while the loop was `pages` → `island:x` → `pages`).
///
/// The check is here, at the WRITE, on data alone. It cannot live in the render functions: the loop
/// was measured entering each view closure at depth 1, one after another — never nested — so no
/// render-time stack (depth counter, in-flight set) can see it.
mod slots {
    use accrete::{Accrete, AccreteOps};
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    thread_local! {
        static REFS: RefCell<HashMap<String, HashSet<String>>> = RefCell::new(HashMap::new());
        /// Reused across calls: the walk allocates nothing beyond this set.
        static SCRATCH: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    }

    /// Every slot a value reads, walked directly over the tree — no serialising, no intermediate
    /// shape. `borrow_children` is the trait method every variant answers, so this stays correct
    /// when a variant is added.
    fn collect(accrete: &Accrete, out: &mut HashSet<String>) {
        // Both channels address a slot; which plane it lives in does not matter
        // to the cycle check, only that the read follows the slot.
        for b in [accrete.get_source(), accrete.get_sink()].into_iter().flatten() {
            if let Some(slot) = &b.slot {
                out.insert(slot.clone());
            }
        }
        if let Some(children) = accrete.borrow_children() {
            for c in children {
                collect(c, out);
            }
        }
    }

    /// Records what `slot` will reference and returns the cycle that would close, if any.
    /// `replacing` = a `set` replaces the contents; otherwise the references are added to them.
    ///
    /// Cost on the hot path is one walk of the value with no allocation beyond a reused set; a value
    /// that reads nothing (a text row, most data frames) returns here without touching the graph.
    pub fn note(slot: &str, accrete: &Accrete, replacing: bool) -> Option<Vec<String>> {
        SCRATCH.with(|s| {
            let mut refs = s.borrow_mut();
            refs.clear();
            collect(accrete, &mut refs);
            if refs.is_empty() {
                // Reads nothing: this value cannot be part of a cycle, and the graph is untouched.
                return None;
            }
            let refs = &*refs;
            REFS.with(|r| {
                let mut graph = r.borrow_mut();
                if replacing {
                    graph.insert(slot.to_string(), refs.clone());
                } else {
                    graph
                        .entry(slot.to_string())
                        .or_default()
                        .extend(refs.iter().cloned());
                }
                let mut path = vec![slot.to_string()];
                let mut seen = HashSet::from([slot.to_string()]);
                if walk(&graph, slot, slot, &mut path, &mut seen) {
                    Some(path)
                } else {
                    None
                }
            })
        })
    }

    fn walk(
        graph: &HashMap<String, HashSet<String>>,
        from: &str,
        target: &str,
        path: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) -> bool {
        let Some(next) = graph.get(from) else {
            return false;
        };
        for n in next {
            if n == target {
                path.push(n.clone());
                return true;
            }
            if !seen.insert(n.clone()) {
                continue;
            }
            path.push(n.clone());
            if walk(graph, n, target, path, seen) {
                return true;
            }
            path.pop();
        }
        false
    }
}

/// 渲染深度守卫（wasm 里递归渲染会把栈吃干，症状是 `memory access out of bounds` 或
/// `Maximum call stack size exceeded` —— 崩在 malloc 或 `Option<String>::clone` 里，
/// 与真正的递归点隔着几十层，看上去像内存 bug）。
///
/// 一旦某棵数据能渲染到自己（槽里的值里又有绑定同一个槽的节点），崩溃就是必然的，
/// 而 wasm 崩掉之后**整页**都失去响应，不是某一个区域坏掉。所以这里限深：超限时
/// 打一行指出是哪个节点、哪个 id，然后渲染成空——一个坏节点不该带走整张页面。
const MAX_RENDER_DEPTH: usize = 64;

thread_local! {
    static RENDER_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct DepthGuard;

impl Drop for DepthGuard {
    fn drop(&mut self) {
        RENDER_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// 渲染一个 accrete 为视图（供 external 触发）。
pub fn render_accrete(ctx: &Ctx, accrete: &Accrete) -> AnyView {
    let depth = RENDER_DEPTH.with(|d| {
        let v = d.get() + 1;
        d.set(v);
        v
    });
    if depth > MAX_RENDER_DEPTH {
        RENDER_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        tracing::error!(
            "render depth {} exceeded at {} (id {:?}) — this node renders itself; drawing nothing",
            MAX_RENDER_DEPTH,
            accrete.get_type(),
            accrete.get_id()
        );
        return ().into_any();
    }
    let _guard = DepthGuard;
    dispatch(accrete, ctx)
}

/// 渲染一组子 accrete。
pub fn render_children(ctx: &Ctx, subs: &[Accrete]) -> Vec<AnyView> {
    subs.iter().map(|b| render_accrete(ctx, b)).collect()
}

#[cfg(test)]
mod config_tests {
    use super::{ConfigValue, merge_remote, pointer_segments, write_into};
    use serde_json::{Value, json};

    fn remote(v: Value) -> ConfigValue {
        ConfigValue {
            local: None,
            remote: Some(v),
        }
    }

    #[test]
    fn a_new_member_lands_in_the_remote_half() {
        let next = merge_remote(&ConfigValue::default(), &json!("sketch")).unwrap();
        assert_eq!(next.remote, Some(json!("sketch")));
        assert_eq!(next.local, None);
    }

    #[test]
    fn an_equal_member_is_not_written() {
        let cur = remote(json!("sketch"));
        assert_eq!(merge_remote(&cur, &json!("sketch")), None);
    }

    #[test]
    fn null_clears_the_remote_half_and_keeps_the_local_one() {
        let cur = ConfigValue {
            local: Some(json!("normal")),
            remote: Some(json!("sketch")),
        };
        let next = merge_remote(&cur, &Value::Null).unwrap();
        assert_eq!(next.remote, None);
        assert_eq!(next.local, Some(json!("normal")));
    }

    #[test]
    fn clearing_an_absent_half_writes_nothing() {
        let cur = ConfigValue {
            local: Some(json!("normal")),
            remote: None,
        };
        assert_eq!(merge_remote(&cur, &Value::Null), None);
    }

    #[test]
    fn local_wins_and_remote_is_the_fallback() {
        let both = ConfigValue {
            local: Some(json!("normal")),
            remote: Some(json!("sketch")),
        };
        assert_eq!(both.effective(), Some(&json!("normal")));
        let only_remote = remote(json!("sketch"));
        assert_eq!(only_remote.effective(), Some(&json!("sketch")));
        assert_eq!(ConfigValue::default().effective(), None);
    }

    #[test]
    fn a_pointer_creates_the_object_it_names() {
        let mut acc = Value::Null;
        let segs = pointer_segments("/density/number").unwrap();
        write_into(&mut acc, &segs, json!(4)).unwrap();
        assert_eq!(acc, json!({"density": {"number": 4}}));
    }

    #[test]
    fn a_pointer_through_a_scalar_is_refused() {
        let mut acc = json!({"density": "compact"});
        let segs = pointer_segments("/density/number").unwrap();
        assert!(write_into(&mut acc, &segs, json!(4)).is_err());
    }

    #[test]
    fn a_path_without_a_leading_slash_is_refused() {
        assert!(pointer_segments("skin").is_err());
    }

    #[test]
    fn pointer_escapes_are_unescaped() {
        assert_eq!(
            pointer_segments("/a~1b/c~0d").unwrap(),
            vec!["a/b".to_string(), "c~d".to_string()]
        );
    }
}

