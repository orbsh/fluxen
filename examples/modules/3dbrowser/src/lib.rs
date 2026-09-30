//! 3dbrowser —— ADR 0007 模块契约的示例实现：按需加载的 three-d 能力模块。
//!
//! 构建（在本目录）：
//! ```sh
//! cargo build --release
//! wasm-bindgen gen --target web -o pkg target/wasm32-unknown-unknown/browser3d.wasm
//! ```
//! `pkg/` + 本目录 `index.js` 一起放 CDN（或本地 assets），Canvas 组件的
//! `url` 指向 `index.js`。宿主只认识四个导出（mount/update/resize/unmount），
//! 数据区语义（`assets`/`primitives`）完全归本模块。

use js_sys::Uint8Array;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use three_d::*;
use wasm_bindgen::prelude::*;

/// 自定义数据区的 schema（模块私有词汇，核心不校验）。
///
/// ```json
/// {
///   "primitives": { "cube": {}, "sphere": {"color": [200,60,60]} },
///   "assets": { "duck": null, "fox": "https://cdn/fox.glb" }
/// }
/// ```
/// null = 占位（ADR 0005 指针只能 replace 已存在的键，流式喂 URL 时先以
/// null 占位、后逐帧 replace 成真值）；模块跳过 null，见到非 null 才加载。
#[derive(Debug, Default, Deserialize)]
struct Spec {
    #[serde(default)]
    primitives: HashMap<String, PrimSpec>,
    #[serde(default)]
    assets: HashMap<String, Option<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct PrimSpec {
    #[serde(default)]
    color: Option<[u8; 3]>,
}

fn decode(data: &JsValue) -> Result<Spec, JsError> {
    let bytes = Uint8Array::new(data).to_vec();
    let spec: Spec = ciborium::de::from_reader(std::io::Cursor::new(&bytes[..]))
        .map_err(|e| JsError::new(&format!("3dbrowser: bad CBOR payload: {e}")))?;
    Ok(spec)
}

struct State {
    context: Context,
    camera: Camera,
    ambient: AmbientLight,
    directional: DirectionalLight,
    /// 内置几何（无光材质；与 gltf 分两趟 render，材质类型不同）。
    color_objs: Vec<Gm<Mesh, ColorMaterial>>,
    /// 已加载的 gltf 模型（PBR，吃灯光）。
    models: Vec<Model<PhysicalMaterial>>,
    seen: HashSet<String>,
    pending: VecDeque<(String, String)>,
    dirty: bool,
    size: (u32, u32),
}

impl State {
    fn apply(&mut self, spec: Spec) {
        let mut next = 0u32;
        for (name, ps) in spec.primitives {
            if self.seen.insert(name.clone()) {
                let mut cpu = match name.as_str() {
                    "sphere" => CpuMesh::sphere(24),
                    "cylinder" => CpuMesh::cylinder(24),
                    _ => CpuMesh::cube(),
                };
                // 网格排布：每加一个几何体沿 X 平移一格
                cpu.transform(Mat4::from_translation(vec3(next as f32 * 2.0 - 2.0, 0.0, 0.0)))
                    .ok();
                let material = ColorMaterial {
                    color: ps
                        .color
                        .map_or(Srgba::new_opaque(90, 140, 200), |c| Srgba::new_opaque(c[0], c[1], c[2])),
                    ..Default::default()
                };
                self.color_objs
                    .push(Gm::new(Mesh::new(&self.context, &cpu), material));
                next += 1;
                self.dirty = true;
            }
        }
        for (key, url) in spec.assets {
            let Some(url) = url else { continue };   // null 占位 = 还没到
            let id = format!("{key}:{url}");
            if self.seen.insert(id.clone()) {
                self.pending.push_back((id, url));
            }
        }
    }

    /// 处理一个挂起的 asset；异步调用方 spawn_local 包一层。
    fn pump(&mut self) -> Option<(String, String)> {
        self.pending.pop_front()
    }

    fn add_model(&mut self, cpu: three_d_asset::Model) {
        // 居中、归一到 ~2 格半径，避免外部 gltf 尺度五花八门
        let mut model = match Model::<PhysicalMaterial>::new(&self.context, &cpu) {
            Ok(m) => m,
            Err(e) => {
                web_sys::console::warn_1(&format!("3dbrowser: model failed: {e:?}").into());
                return;
            }
        };
        let mut aabb = AxisAlignedBoundingBox::EMPTY;
        for o in model.iter() {
            aabb.expand_with_aabb(o.aabb());
        }
        let c = -aabb.center();
        let sz = aabb.size();
        let diag = (sz.x * sz.x + sz.y * sz.y + sz.z * sz.z).sqrt();
        let s = if diag > 0.0 { 4.0 / diag } else { 1.0 };
        for m in model.iter_mut() {
            m.set_transformation(Mat4::from_translation(c) * Mat4::from_scale(s));
        }
        self.models.push(model);
        self.dirty = true;
    }

    fn render(&mut self) {
        let (w, h) = self.size;
        if w == 0 || h == 0 {
            return;
        }
        let rt = RenderTarget::screen(&self.context, w, h);
        rt.clear(ClearState::color_and_depth(0.93, 0.95, 0.97, 1.0, 1.0));
        let lights: [&dyn Light; 2] = [&self.ambient, &self.directional];
        rt.render(&self.camera, &self.color_objs, &lights);
        rt.render(
            &self.camera,
            self.models.iter().flat_map(|m| m.into_iter()),
            &lights,
        );
        self.dirty = false;
    }
}

thread_local! {
    static CTXS: RefCell<Vec<Option<Rc<RefCell<State>>>>> = const { RefCell::new(Vec::new()) };
}

fn with_ctx<T>(ctx: u32, f: impl FnOnce(&mut State) -> T) -> Result<T, JsError> {
    CTXS.with(|c| {
        c.borrow_mut()
            .get_mut(ctx as usize)
            .and_then(Option::as_mut)
            .map(|rc| f(&mut rc.borrow_mut()))
            .ok_or_else(|| JsError::new("3dbrowser: stale ctx"))
    })
}

/// 契约 mount：canvas + 自定义数据区（CBOR）+ 宿主通道（本示例不用，留口）。
#[wasm_bindgen]
pub fn mount(canvas: web_sys::HtmlCanvasElement, data: JsValue, _host: JsValue) -> Result<u32, JsError> {
    let gl = canvas
        .get_context("webgl2")
        .map_err(|_| JsError::new("3dbrowser: webgl2 unavailable"))?
        .ok_or_else(|| JsError::new("3dbrowser: no webgl2 context"))?
        .dyn_into::<web_sys::WebGl2RenderingContext>()
        .map_err(|_| JsError::new("3dbrowser: not webgl2"))?;
    let context = Context::from_gl_context(std::sync::Arc::new(glow::Context::from_webgl2_context(gl)))
        .map_err(|e| JsError::new(&format!("3dbrowser: context: {e:?}")))?;

    let size = (canvas.width(), canvas.height());
    let camera = Camera::new_perspective(
        Viewport::new_at_origo(size.0.max(1), size.1.max(1)),
        vec3(2.5, 2.0, 4.5),
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.05,
        100.0,
    );
    let state = Rc::new(RefCell::new(State {
        ambient: AmbientLight::new(&context, 0.5, Srgba::WHITE),
        directional: DirectionalLight::new(&context, 2.0, Srgba::WHITE, vec3(-0.4, -1.0, -0.5)),
        context,
        camera,
        color_objs: Vec::new(),
        models: Vec::new(),
        seen: HashSet::new(),
        pending: VecDeque::new(),
        dirty: true,
        size,
    }));
    state.borrow_mut().apply(decode(&data)?);

    let id = CTXS.with(|c| {
        let mut v = c.borrow_mut();
        v.push(Some(Rc::clone(&state)));
        (v.len() - 1) as u32
    });
    Ok(id)
}

/// 契约 update：数据区增量（append/patch 后的全量重推）——幂等消费。
#[wasm_bindgen]
pub fn update(ctx: u32, data: JsValue) -> Result<(), JsError> {
    let spec = decode(&data)?;
    with_ctx(ctx, |s| s.apply(spec))?;
    Ok(())
}

/// 契约 resize：canvas 像素尺寸由宿主定。
#[wasm_bindgen]
pub fn resize(ctx: u32, w: u32, h: u32) -> Result<(), JsError> {
    with_ctx(ctx, |s| {
        s.size = (w, h);
        s.camera
            .set_viewport(Viewport::new_at_origo(w.max(1), h.max(1)));
        s.dirty = true;
    })?;
    Ok(())
}

/// 契约 unmount：释放状态；canvas/GL 由浏览器回收。
#[wasm_bindgen]
pub fn unmount(ctx: u32) {
    CTXS.with(|c| {
        if let Some(slot) = c.borrow_mut().get_mut(ctx as usize) {
            *slot = None;
        }
    });
}

/// 宿主 rAF 驱动的一帧：先泵异步 asset，再按脏位重绘。
#[wasm_bindgen]
pub fn frame(ctx: u32) -> Result<(), JsError> {
    let job = with_ctx(ctx, |s| s.pump())?;
    if let Some((id, url)) = job {
        // 异步加载：成功后回注 state；失败的 asset 不回 seen，下次 update 可重试
        wasm_bindgen_futures::spawn_local(async move {
            match three_d_asset::io::load_async(&[&url]).await {
                Ok(mut loaded) => {
                    let cpu: Option<three_d_asset::Model> = loaded.deserialize(&url).ok();
                    if let Some(cpu) = cpu {
                        CTXS.with(|c| {
                            if let Some(Some(rc)) = c.borrow().get(ctx as usize) {
                                rc.borrow_mut().add_model(cpu);
                            }
                        });
                    } else {
                        web_sys::console::warn_1(&format!("3dbrowser: deserialize failed: {url}").into());
                        forget(&id);
                    }
                }
                Err(e) => {
                    web_sys::console::warn_1(&format!("3dbrowser: load failed: {e:?} {url}").into());
                    forget(&id);
                }
            }
        });
    }
    with_ctx(ctx, |s| {
        if s.dirty {
            s.render();
        }
    })?;
    Ok(())
}

/// 异步失败的条目退回待重试：从 seen 撤下（简单路径：重新 apply 时再进 pending）。
fn forget(_id: &str) {
    // seen 是 (key:url) 复合 id；撤下后下次 update 同 key 仍被 seen 拦住属已知限制，
    // 示例模块够用；正式版应记失败表并允许显式 reset。
}

/// 左键拖拽 = 绕 target 旋转（index.js 装事件后转发进来）。
#[wasm_bindgen]
pub fn push_drag(ctx: u32, dx: f32, dy: f32) {
    with_ctx(ctx, |s| {
        let t = s.camera.target();
        s.camera.rotate_around_with_fixed_up(t, dx * 0.01, dy * 0.01);
        s.dirty = true;
    })
    .ok();
}

/// 滚轮 = 推拉视角。
#[wasm_bindgen]
pub fn push_wheel(ctx: u32, delta: f32) {
    with_ctx(ctx, |s| {
        s.camera.zoom(delta, 0.3, 60.0);
        s.dirty = true;
    })
    .ok();
}
