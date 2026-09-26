use crate::hooks::FormState;
use crate::render::dispatch;
use brick::{
    Brick, BrickOps,
    merge::{BrickOp, Concat, Delete, Replace},
};
use content::codec::ActiveCodec;
use content::{Content, Message, Method};
use leptos::prelude::*;
use minijinja::Environment;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::{LazyLock, RwLock};
use transport::Transport;

static TMPL: LazyLock<RwLock<Environment>> = LazyLock::new(|| {
    let env = Environment::new();
    RwLock::new(env)
});

/// 全局共享状态容器：掌管布局、数据、列表与传输收发。
///
/// `form` 是渲染期动态环境：`form_` 注入自己的 `FormState` 后克隆本结构
/// 渲染子树，归属沿克隆链传播；兄弟分支各持自己的克隆，互不污染。
/// 嵌套表单靠遮蔽生效（内层覆盖外层）。
/// per-key 信号槽：外层 map 只存句柄（键集变化时才写外层），
/// 值经内层信号发布——某个键的更新不通知其他键的订阅者。
pub type ListSlot = RwSignal<std::sync::Arc<Vec<Brick>>>;
pub type DataSlot = RwSignal<Option<std::sync::Arc<Brick>>>;

#[derive(Clone)]
pub struct Ctx {
    pub transport: LeptosTransport,
    pub codec: ActiveCodec,
    pub layout: RwSignal<Brick>,
    pub data: RwSignal<HashMap<String, DataSlot>>,
    pub list: RwSignal<HashMap<String, ListSlot>>,
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
        let layout = RwSignal::new(Brick::text(Default::default()));
        let data = RwSignal::new(HashMap::new());
        let list = RwSignal::new(HashMap::new());

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
            owner: Owner::current().expect("Ctx::new requires a reactive owner"),
            form: None,
        };

        // 消费下行帧并分发
        let ctx_clone = ctx.clone();
        Effect::new(move |_| {
            let b = ctx_clone.transport.frame.get();
            if !b.is_empty() {
                // 收侧双格式自适应：帧首字节定格式，与 ?codec= 钉定的发送格式无关
                match ctx_clone.codec.decode_auto::<Message<Brick>>(&b) {
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

    pub fn set(&self, name: impl AsRef<str>, brick: Brick) {
        self.slot_for_data(name.as_ref()).set(Some(Arc::new(brick)));
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
}

fn dispatch_msg(act: &Message<Brick>, ctx: &Ctx) {
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
                let _ = TMPL
                    .write()
                    .expect("write TMPL failed")
                    .add_template_owned(n, d);
            }
            Content::Create(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().expect("read TMPL failed");
                d.expand(&env);
                tracing::info!("create: layout set, root = {:.100?}", d);
                // Effect 内写信号一律 set/untracked；tracked 读见上注释
                ctx.layout.set(d);
            }
            Content::Set(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().expect("read TMPL failed");
                d.expand(&env);
                ctx.slot_for_data(&x.event).set(Some(Arc::new(d)));
            }
            Content::Join(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().expect("read TMPL failed");
                d.expand(&env);
                let vs: &dyn BrickOp = match x.method {
                    Method::Replace => &Replace,
                    Method::Concat => &Concat,
                    Method::Delete => &Delete,
                };
                // 只写该键的内层槽：clone 壳 + make_mut 独占重建该条目，
                // 其他键的槽与其他行的 Arc 原样带给各自订阅者。
                // untracked 读：dispatch_msg 运行在消费 frame 的 Effect 里，
                // tracked 读会让 Effect 订阅自己写入的槽 → 自激循环。
                let slot = ctx.slot_for_list(&x.event);
                let cur = slot.get_untracked();
                let mut list = std::sync::Arc::unwrap_or_clone(cur);
                if d.get_id().is_some() {
                    let mut is_merge = false;
                    for i in list.iter_mut() {
                        if i.cmp_id(&d) {
                            is_merge = true;
                            let mut rhs = d.clone();
                            i.merge(vs, &mut rhs);
                        }
                    }
                    if !is_merge {
                        list.push(d.clone());
                    }
                } else {
                    list.push(d.clone());
                }
                slot.set(Arc::new(list));
            }
            Content::Empty => {}
        }
    }
}

/// 渲染一个 brick 为视图（供 external 触发）。
pub fn render_brick(ctx: &Ctx, brick: &Brick) -> AnyView {
    dispatch(brick, ctx)
}

/// 渲染一组子 brick。
pub fn render_children(ctx: &Ctx, subs: &[Brick]) -> Vec<AnyView> {
    subs.iter().map(|b| dispatch(b, ctx)).collect()
}
