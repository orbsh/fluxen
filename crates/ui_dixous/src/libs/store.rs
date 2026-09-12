use brick::{
    Brick, BrickOps,
    merge::{BrickOp, Concat, Delete, Replace},
};
use content::{Content, Message, Method};
#[allow(unused_imports)]
use dioxus::prelude::*;
use js_sys::wasm_bindgen::JsError;
use content::codec::ActiveCodec;
use minijinja::Environment;
use serde_json::Value;
use std::collections::HashMap;
use std::str;
use std::sync::{LazyLock, RwLock};
use transport::Transport;

static TMPL: LazyLock<RwLock<Environment>> = LazyLock::new(|| {
    let env = Environment::new();
    //env.set_auto_escape_callback(|_| AutoEscape::Json);
    RwLock::new(env)
});

#[derive(Clone)]
pub struct Status {
    pub transport: DxTransport,
    pub codec: ActiveCodec,
    pub layout: Signal<Brick>,
    pub data: Signal<HashMap<String, Brick>>,
    pub list: Signal<HashMap<String, Vec<Brick>>>,
}

/// UI 侧持有传输 + 下行帧信号。wasm 单线程，Rc 共享。
#[derive(Clone)]
pub struct DxTransport {
    pub inner: send_wrapper::SendWrapper<std::rc::Rc<dyn Transport>>,
    /// 最近一帧下行字节；桥接自 transport.on() 读循环。
    pub frame: Signal<Vec<u8>>,
}

impl Status {
    pub async fn send(&mut self, event: impl AsRef<str>, id: Option<String>, content: Value) {
        self.transport
            .inner
            .emit(transport::Event {
                name: event.as_ref().to_string(),
                id,
                data: content,
            })
            .await;
    }

    pub fn set(&mut self, name: impl AsRef<str>, brick: Brick) {
        self.data.write().insert(name.as_ref().to_string(), brick);
    }
}

fn dispatch(
    act: Message<Brick>,
    layout: &mut Signal<Brick>,
    data: &mut Signal<HashMap<String, Brick>>,
    list: &mut Signal<HashMap<String, Vec<Brick>>>,
) {
    let Message {
        sender: _,
        created: _,
        content,
    } = act;
    for c in content {
        match c {
            Content::Tmpl(x) => {
                let n = x.name;
                let d = x.data;
                let _ = TMPL
                    .write()
                    .expect("write TMPL failed")
                    .add_template_owned(n, d);
            }
            Content::Create(mut x) => {
                let env = TMPL.read().expect("read TMPL failed");
                x.data.expand(&env);
                layout.set(x.data)
            }
            Content::Set(x) => {
                let e = x.event;
                let mut d = x.data;
                let env = TMPL.read().expect("read TMPL failed");
                d.expand(&env);
                data.write().insert(e, d);
            }
            Content::Join(mut x) => {
                let env = TMPL.read().expect("read TMPL failed");
                x.data.expand(&env);
                let e = x.event;
                let d = &mut x.data;
                let vs: &dyn BrickOp = match x.method {
                    Method::Replace => &Replace,
                    Method::Concat => &Concat,
                    Method::Delete => &Delete,
                };
                if let Some(_id) = &d.get_id() {
                    let mut l = list.write();
                    let list = l.entry(e).or_default();
                    let mut is_merge = false;
                    for i in list.iter_mut() {
                        if i.cmp_id(d) {
                            is_merge = true;
                            i.merge(vs, d);
                        }
                    }
                    if !is_merge {
                        list.push(d.clone());
                    }
                } else {
                    list.write().entry(e).or_default().push(d.clone());
                }
            }
            Content::Empty => {}
        }
    }
}

pub fn use_status(transport: std::rc::Rc<dyn Transport>, codec: ActiveCodec) -> Result<Status, JsError> {
    let frame = use_signal(Vec::new);

    // 读循环：transport.on() -> frame 信号
    {
        let mut stream = transport.on();
        let mut frame_writer = frame;
        spawn(async move {
            use futures::StreamExt;
            while let Some(bytes) = stream.next().await {
                frame_writer.set(bytes);
            }
        });
    }

    let mut layout = use_signal::<Brick>(|| Brick::text(Default::default()));
    let mut data = use_signal::<HashMap<String, Brick>>(HashMap::new);
    let mut list = use_signal::<HashMap<String, Vec<Brick>>>(HashMap::new);

    let recv_codec = codec.clone();
    use_memo(move || {
        let b = &frame();
        if !b.is_empty() {
            match recv_codec.decode::<Message<Brick>>(b) {
                Ok(act) => dispatch(act, &mut layout, &mut data, &mut list),
                Err(err) => {
                    if let Ok(act) = &String::from_utf8(b.clone()) {
                        dioxus::logger::tracing::info!("deserialize error: {:#?}\n{}", err, act)
                    }
                }
            }
        }
    });

    Ok(Status {
        transport: DxTransport {
            inner: send_wrapper::SendWrapper::new(transport),
            frame,
        },
        codec,
        layout,
        data,
        list,
    })
}
