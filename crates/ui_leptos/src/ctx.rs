use crate::render::dispatch;
use brick::{
    Brick, BrickOps,
    merge::{BrickOp, Concat, Delete, Replace},
};
use content::{Content, Message, Method};
use leptos::prelude::*;
use content::codec::ActiveCodec;
use minijinja::Environment;
use serde_json::Value;
use crate::hooks::FormState;
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
#[derive(Clone)]
pub struct Ctx {
    pub transport: LeptosTransport,
    pub codec: ActiveCodec,
    pub layout: RwSignal<Brick>,
    pub data: RwSignal<HashMap<String, Arc<Brick>>>,
    pub list: RwSignal<HashMap<String, Arc<Vec<Brick>>>>,
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
            form: None,
        };

        // 消费下行帧并分发
        let ctx_clone = ctx.clone();
        Effect::new(move |_| {
            let b = ctx_clone.transport.frame.get();
            if !b.is_empty() {
                match ctx_clone.codec.decode::<Message<Brick>>(&b) {
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
        self.data.update(|d| {
            d.insert(name.as_ref().to_string(), Arc::new(brick));
        });
    }
}

fn dispatch_msg(act: &Message<Brick>, ctx: &Ctx) {
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
                ctx.layout.set(d);
            }
            Content::Set(x) => {
                let mut d = x.data.clone();
                let env = TMPL.read().expect("read TMPL failed");
                d.expand(&env);
                ctx.data
                    .update(|m| {
                        m.insert(x.event.clone(), Arc::new(d));
                    });
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
                // 克隆 map 只复制 Rc 壳；改动的那一条经 make_mut 独占重建，
                // 其余条目的 Rc 原样带给所有订阅者。
                ctx.list.update(|m| {
                    let list = Arc::make_mut(m.entry(x.event.clone()).or_default());
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
                });
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
