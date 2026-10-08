use crate::hooks::FormState;
use crate::render::dispatch;
use accrete::{Accrete, AccreteOps};
use content::codec::ActiveCodec;
use content::{Content, Message, PatchKind};
use leptos::prelude::*;
use serde_json::{Value, to_value};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::{LazyLock, RwLock};
use transport::Transport;

/// 已注册模板表（ADR 0006：`{{key}}` 纯槽位替换，无模板引擎）。
static TMPL: LazyLock<RwLock<accrete::template::Templates>> =
    LazyLock::new(|| RwLock::new(accrete::template::Templates::new()));

/// 全局共享状态容器：掌管布局、数据、列表与传输收发。
///
/// `form` 是渲染期动态环境：`form_` 注入自己的 `FormState` 后克隆本结构
/// 渲染子树，归属沿克隆链传播；兄弟分支各持自己的克隆，互不污染。
/// 嵌套表单靠遮蔽生效（内层覆盖外层）。
/// per-key 信号槽：外层 map 只存句柄（键集变化时才写外层），
/// 值经内层信号发布——某个键的更新不通知其他键的订阅者。
pub type ListSlot = RwSignal<std::sync::Arc<Vec<Accrete>>>;
pub type DataSlot = RwSignal<Option<std::sync::Arc<Accrete>>>;
/// 裸 Value 槽（ADR 0008 local 平面）：事件数据不是展示形态，
/// 与 data（Accrete）分平面。
pub type ValueSlot = RwSignal<Option<Value>>;

#[derive(Clone)]
pub struct Ctx {
    pub transport: LeptosTransport,
    pub codec: ActiveCodec,
    pub layout: RwSignal<Accrete>,
    pub data: RwSignal<HashMap<String, DataSlot>>,
    pub list: RwSignal<HashMap<String, ListSlot>>,
    /// local 平面：`kind: local` 事件落点与订阅共用的具名 Value 槽。
    pub vals: RwSignal<HashMap<String, ValueSlot>>,
    /// 槽位的永久 owner：Ctx::new 时捕获的 app 根作用域。
    /// 槽若建在消费帧的 Effect 作用域里，Effect 重跑会 dispose 其存储，
    /// 外层 map 里留下的就是死句柄（panic: already disposed）。
    pub(crate) owner: Owner,
    pub form: Option<Arc<FormState>>,
}

/// UI 侧持有传输 + 下行帧信号。wasm 单线程，Rc 共享。
#[derive(Clone)]
pub struct LeptosTransport {
    /// wasm 单线程，用 SendWrapper 让 Rc 满足 leptos 组件闭包的 Send 约束
    /// （与原 WebSocketHandle 的做法一致）。
    pub inner: send_wrapper::SendWrapper<std::rc::Rc<dyn Transport>>,
    /// 最近一帧下行字节；桥接自 transport.on() 读循环。
    pub frame: RwSignal<Vec<u8>>,
}

impl Ctx {
    /// 装配：传入已连接的 transport（由 lib.rs 按 feature 选择实现）。
    /// 框架桥接：`spawn_local` 读流写 `frame` 信号，Effect 消费解码分发。
    pub fn new(transport: std::rc::Rc<dyn Transport>, codec: ActiveCodec) -> Self {
        let frame = RwSignal::new(Vec::new());
        let layout = RwSignal::new(Accrete::text(Default::default()));
        let data = RwSignal::new(HashMap::new());
        let list = RwSignal::new(HashMap::new());
        let vals = RwSignal::new(HashMap::new());

        // 读循环：transport.on() -> frame 信号
        {
            let mut stream = transport.on();
            let frame_writer = frame;
            leptos::task::spawn_local(async move {
                use futures::StreamExt;
                while let Some(bytes) = stream.next().await {
                    frame_writer.set(bytes);
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
            owner: Owner::current().expect("Ctx::new requires a reactive owner"),
            form: None,
        };

        // 消费下行帧并分发
        let ctx_clone = ctx.clone();
        Effect::new(move |_| {
            let b = ctx_clone.transport.frame.get();
            if !b.is_empty() {
                // 收侧双格式自适应：帧首字节定格式，与 ?codec= 钉定的发送格式无关
                match ctx_clone.codec.decode_auto::<Message<Accrete>>(&b) {
                    Ok(act) => dispatch_msg(&act, &ctx_clone),
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

    /// emit 统一落点（ADR 0008，形状修订见 ADR 0010）：`Event` 上行
    /// transport（现状不变），`Local` 不上行——写入 local 平面具名值槽
    /// 的载荷是与上行同形的包装对象 `Outflow {event, id?, data}`：事件
    /// 数据形状由事件定义、不由落点决定（同一事件从 event 改绑 local，
    /// 订阅方 path/键零改动）。`event` 名取 `Local.event`，缺省 = 槽名
    /// （槽即频道）。发射侧忽略自己的 `path` 字段（写必整值——指针提取
    /// 是订阅侧的事）。其他变体返回 false，调用方保持自己的兜底。
    pub fn emit(&self, variant: &accrete::BindVariant, id: Option<String>, payload: Value) -> bool {
        match variant {
            accrete::BindVariant::Event { event } => {
                let event = event.clone();
                let ctx = self.clone();
                leptos::task::spawn_local(async move {
                    ctx.send(event, id, payload).await;
                });
                true
            }
            accrete::BindVariant::Local { slot, event, .. } => {
                let wrapped = to_value(&content::Outflow {
                    event: event.clone().unwrap_or_else(|| slot.clone()),
                    id,
                    data: payload,
                })
                .unwrap_or(Value::Null);
                self.slot_for_value(slot).set(Some(wrapped));
                true
            }
            _ => false,
        }
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
}

fn dispatch_msg(act: &Message<Accrete>, ctx: &Ctx) {
    // 操作通道过滤：非渲染类帧不触碰 layout/data/list（ADR 0003）。
    // Influx.event 是数据槽名，属渲染内部寻址，与此处消息级 ev 平面不同。
    if act.ev != content::EV_DRAW {
        tracing::debug!("frame ev = {:?} (not draw), skipped", act.ev);
        return;
    }
    for c in &act.content {
        match c {
            Content::Tmpl(x) => {
                let n = x.name.clone();
                let d = x.data.clone();
                TMPL.write()
                    .expect("write TMPL failed")
                    .insert(n, d);
            }
            Content::Create(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().expect("read TMPL failed");
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                tracing::info!("create: layout set, root = {:.100?}", d);
                // Effect 内写信号一律 set/untracked；tracked 读见上注释
                ctx.layout.set(d);
            }
            Content::Set(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().expect("read TMPL failed");
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                ctx.slot_for_data(&x.event).set(Some(Arc::new(d)));
            }
            Content::Append(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().expect("read TMPL failed");
                for w in d.expand(&env) {
                    tracing::warn!("{w}");
                }
                // untracked 读：dispatch_msg 运行在消费 frame 的 Effect 里，
                // tracked 读会让 Effect 订阅自己写入的槽 → 自激循环。
                let slot = ctx.slot_for_list(&x.event);
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
                            x.event,
                            id
                        );
                        continue;
                    }
                }
                list.push(d.clone());
                slot.set(Arc::new(list));
            }
            Content::Remove(x) => {
                let slot = ctx.slot_for_list(&x.event);
                let cur = slot.get_untracked();
                let mut list = std::sync::Arc::unwrap_or_clone(cur);
                let before = list.len();
                list.retain(|i| i.get_id().as_deref() != Some(x.id.as_str()));
                if list.len() != before {
                    slot.set(Arc::new(list));
                } else {
                    tracing::warn!("remove: event {:?} has no row id {:?}", x.event, x.id);
                }
            }
            Content::Patch(x) => {
                // 平面路由（ADR 0005）："" = layout 根；id 设 = 列表行；
                // 其余 = 具名数据槽。
                let result = if x.event.is_empty() {
                    let mut d = ctx.layout.get_untracked();
                    let r = apply_patch(&mut d, x);
                    // 写回 layout 信号，否则响应式根（lib.rs tracked get）永不重建
                    if r.is_ok() {
                        ctx.layout.set(d);
                    }
                    r
                } else if let Some(id) = &x.id {
                    let slot = ctx.slot_for_list(&x.event);
                    let cur = slot.get_untracked();
                    let mut list = std::sync::Arc::unwrap_or_clone(cur);
                    let r = match list
                        .iter_mut()
                        .find(|i| i.get_id().as_deref() == Some(id.as_str()))
                    {
                        Some(row) => apply_patch(row, x),
                        None => Err(format!("no row {id:?} in {:?}", x.event)),
                    };
                    if r.is_ok() {
                        slot.set(Arc::new(list));
                    }
                    r
                } else {
                    let slot = ctx.slot_for_data(&x.event);
                    match slot.get_untracked().map(|a| std::sync::Arc::unwrap_or_clone(a)) {
                        Some(mut d) => {
                            let r = apply_patch(&mut d, x);
                            if r.is_ok() {
                                slot.set(Some(Arc::new(d)));
                            }
                            r
                        }
                        None => Err(format!("no data slot {:?}", x.event)),
                    }
                };
                if let Err(e) = result {
                    // 未命中是合法降级（补丁可能先于其 create 到达），warn 保证可观测。
                    tracing::warn!("patch {x:?} skipped: {e}");
                }
            }
            Content::Inject(x) => {
                // 远程注入 vals 槽（ADR 0011）：写必整值；同值短路——写读环
                // （生产端把收到的值回显回来）在第一跳终止，代价是连通知也
                // 不发（值槽本是电平语义，ADR 0008 §4）。形状取自
                // `InjectOp::outflow`：与浏览器自发 emit 写的是同一个对象，
                // 订阅方分不出这次切换是谁写的。
                let v = to_value(x.outflow()).unwrap_or(Value::Null);
                let slot = ctx.slot_for_value(&x.slot);
                if !inject_writes(slot.get_untracked().as_ref(), &v) {
                    tracing::debug!("inject {:?}: same value, skipped", x.slot);
                } else {
                    tracing::info!("inject {:?} = {:.120?}", x.slot, v);
                    slot.set(Some(v));
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

/// inject 是否落写（ADR 0011 §2）：同值不写——写读环（生产端把收到的值回显
/// 回来）在第一跳终止；值槽本是电平语义，同值两次写订阅方看到的值不变，
/// 这里连通知也不发。抽成纯函数以便单测（真正的写路径要反应式运行时）。
fn inject_writes(current: Option<&Value>, incoming: &Value) -> bool {
    current != Some(incoming)
}

/// 渲染一个 accrete 为视图（供 external 触发）。
pub fn render_accrete(ctx: &Ctx, accrete: &Accrete) -> AnyView {
    dispatch(accrete, ctx)
}

/// 渲染一组子 accrete 为 keyed children（§14 容器子节点级隔离）。
///
/// 与 rack 行渲染同构：key = 节点 id（无 id 回退 `#{idx}`），每子树独立
/// Owner（挂在容器当前的 Owner 下）+ 惰性闭包 dispatch。容器因任一子
/// 节点变化重跑时，keyed diff 只重建变化/新增的行，未变子树的闭包、
/// Owner、DOM（含模块宿主）跨重建存活。行内数据更新走子组件自己的
/// 槽订阅，不经这里。
pub fn render_children(ctx: &Ctx, subs: &[Accrete]) -> AnyView {
    if subs.is_empty() {
        // 与 keyed 空列表同效，且让调用点的 unwrap_or_default 保持可用
        return ().into_any();
    }
    // 与 rack_ 同构的外层形态：keyed 列表必须构建在动态视图（Effect 上下文）
    // 之内——直接在组件 build 路径同步构建 keyed，嵌套容器的内层行挂载会
    // 触发 wasm 内存越界 panic（e2e 抓到：嵌套 case 的 create 直接崩）。
    // 每子树独立 Owner 的挂载点 = 本动态闭包执行时的 reactive owner（容器
    // 自身的作用域），容器重建时 keyed diff 不清理未变行的 Owner，子树存活。
    let keys: Vec<(String, Accrete)> = subs
        .iter()
        .enumerate()
        .map(|(idx, child)| {
            let key = child
                .get_id()
                .clone()
                .unwrap_or_else(|| format!("#{idx}"));
            (key, child.clone())
        })
        .collect();
    let c = ctx.clone();
    // 行 Owner 的挂载点：Ctx 的根 owner（与 Ctx 同寿）——root dispatch 的
    // render effect 是「拆建」语义（tachys Render for F::rebuild = build 新 +
    // unmount 旧），若把行 Owner 挂在 root effect scope，每次 root 重建都会
    // dispose 行的 Owner/Memo/订阅。挂到根 owner 后，行内 memo/订阅跨 root
    // 重建存活，keyed diff + memo 才能真正隔离。
    //
    // keyed 必须建在动态视图（Effect 上下文）之内——这不是权宜而是结构性
    // 约束：同步建在 build 路径上必崩（实测 2026-10-08，e2e：main 帧发出后
    // 立即 wasm 越界，行 view/memo 5 秒各重算 315 次且每轮 prev=None——
    // memo 所在 scope 被循环拆建，layout 信号只写过 1 次，排除 dispatch
    // 回写自激）。行 Owner 挂根 owner 不能修复：外层动态闭包（rack_ 形态）
    // 是 tachys keyed 同步构建时的必要缓冲层。代价是 keyed 的 rebuild =
    // 全量拆建，Memo 行级隔离在此形态下结构性不可达——布局平面更新的
    // 隔离走生产端纪律：流式内容绑数据槽（kind: source），见 PLAN §14。
    let po = ctx.owner.clone();
    (move || -> AnyView {
        let parent_owner = po.clone();
        let c = c.clone();
        let keys = keys.clone();
    leptos::tachys::view::keyed::keyed(
        keys,
        |(k, _): &(String, Accrete)| k.clone(),
        move |_i, row: (String, Accrete)| {
            let (key, node) = row;
            let owner = parent_owner.with(Owner::new);
            let view = owner.with(|| {
                    // 行内更新腿：行 Memo（rack 同构）tracked 读 layout、按键
                    // 找回本行当前节点。Memo 值相等时不通知，行动态闭包不重
                    // 跑——兄弟节点变化引发的 layout 更新就不会触发本行
                    // dispatch/重挂（模块宿主存活的关键）。Memo 必须建在行动
                    // Owner 作用域内（跨 keyed diff 存活），且 keyed 需在动态
                    // 视图（Effect 上下文）内构建——此前的两次失败分别死于
                    // 「Memo 在父 build 路径上随容器重建」与「keyed 不在 effect
                    // 上下文导致 memo 订阅链不生效」。
                    let c2 = c.clone();
                    let key2 = key.clone();
                    let memo = Memo::new_with_compare(
                        move |_prev: Option<&Accrete>| {
                            let root = c2.layout.get();
                            let found = find_by_key(&root, &key2);
                            found.unwrap_or_else(|| node.clone())
                        },
                        |prev: Option<&Accrete>, next: Option<&Accrete>| match (prev, next) {
                            (Some(a), Some(b)) => a != b,
                            (None, None) => false,
                            _ => true,
                        },
                    );
                    let c3 = c.clone();
                    move || -> AnyView { dispatch(&memo.get(), &c3) }
                    .into_any()
            });
            (
                move |_index: usize| {},
                leptos::tachys::reactive_graph::OwnedView::new_with_owner(view, owner),
            )
        },
    )
    .into_any()
    })
    .into_any()
}

/// 在 layout 树里按键（节点 id 或 `#{idx}` 位置兜底）找回当前节点。
///
/// 递归遍历整棵树：行 key 只保证在"本容器子层"唯一，而行闭包不知道自己
/// 容器在树中的路径，无法限定查找域——递归全树是唯一可行的找回方式。
/// id 按 UI 惯例全局唯一（行身份纪律），无 id 的 `#{idx}` 在深层容器间
/// 可能撞键，撞时取先序遍历第一个——与旧行为一致，风险可接受。
fn find_by_key(root: &Accrete, key: &str) -> Option<Accrete> {
    fn walk(node: &Accrete, key: &str) -> Option<Accrete> {
        let children = node.borrow_children()?;
        for (idx, child) in children.iter().enumerate() {
            let k = child
                .get_id()
                .clone()
                .unwrap_or_else(|| format!("#{idx}"));
            if k == key {
                return Some(child.clone());
            }
            if let Some(hit) = walk(child, key) {
                return Some(hit);
            }
        }
        None
    }
    walk(root, key)
}

#[cfg(test)]
mod tests {
    use super::inject_writes;
    use serde_json::json;

    #[test]
    fn inject_writes_when_the_slot_is_empty() {
        assert!(inject_writes(None, &json!("about")));
    }

    #[test]
    fn inject_skips_an_equal_value() {
        let cur = json!({"event": "page", "data": "about"});
        assert!(!inject_writes(Some(&cur), &cur));
    }

    #[test]
    fn inject_writes_a_changed_value() {
        let cur = json!({"event": "page", "data": "about"});
        assert!(inject_writes(
            Some(&cur),
            &json!({"event": "page", "data": "home"})
        ));
    }
}
