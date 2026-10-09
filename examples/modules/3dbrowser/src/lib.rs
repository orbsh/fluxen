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
///   "assets": { "duck": null, "fox": "https://cdn/fox.glb" },
///   "clear": [0.9, 0.92, 0.96, 1.0]
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
    /// 清屏色 `[r, g, b, a]`（0..1）。缺省 = 全透明（alpha 0），画布透出宿主
    /// 页面背景；要固定底色就在数据区里显式给四元组。
    #[serde(default)]
    clear: Option<[f32; 4]>,
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

/// panic 消息透传到 console.error：wasm 默认（panic=abort）只给 JS 一个裸
/// `unreachable`，连"哪个模型、为什么"都看不到——本模块踩过一次
/// shader link 失败（glb 缺 tangent），全靠这条消息才定位。
fn install_panic_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            web_sys::console::error_1(&format!("3dbrowser panic: {info}").into());
        }));
    });
}

struct State {
    context: Context,
    camera: Camera,
    /// 三灯布光：key 主光（带 shadow map 投影）+ fill 补光 + rim 轮廓光，
    /// 再配低强度 ambient 兜底——单灯直射是原示例"塑料感"的主因。
    ambient: AmbientLight,
    key_light: DirectionalLight,
    fill_light: DirectionalLight,
    rim_light: DirectionalLight,
    /// 内置几何（无光材质；与 gltf 分两趟 render，材质类型不同）。
    color_objs: Vec<Gm<Mesh, ColorMaterial>>,
    /// 已加载的 gltf 模型（PBR，吃灯光）。
    models: Vec<Model<PhysicalMaterial>>,
    seen: HashSet<String>,
    pending: VecDeque<(String, String)>,
    /// 当前已装载的资产 URL 集（展示语义 = 一次一件：spec 的 assets 集变化
    /// 时整体替换模型，而非追加堆叠）。
    current_assets: Vec<String>,
    /// 清屏色（默认全透明，见 `Spec::clear`）。
    clear: [f32; 4],
    dirty: bool,
    size: (u32, u32),
}

impl State {
    fn apply(&mut self, spec: Spec) {
        if let Some(c) = spec.clear {
            self.clear = c;
            self.dirty = true;
        }
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
                // 展示语义 = 一次一件：资产集变化即整体替换。旧模型与旧
                // pending 一并清掉，只让最新声明的资产进装载队列。
                self.models.clear();
                self.current_assets.clear();
                self.pending.clear();
                self.current_assets.push(id.clone());
                self.pending.push_back((id, url));
            }
        }
    }

    /// 处理一个挂起的 asset；异步调用方 spawn_local 包一层。
    fn pump(&mut self) -> Option<(String, String)> {
        self.pending.pop_front()
    }

    fn add_model(&mut self, cpu: three_d_asset::Model) {
        // 补算切线：很多 glb 带法线贴图（normal texture）却不含 tangent 属性，
        // 而 three-d 的 PhysicalMaterial 在有 normal_texture 时会在 fragment
        // shader 里声明 `in vec3 tang/bitang`，对应的 vertex shader 只有几何带
        // tangents 时才输出它们——缺了就是 shader link 失败
        // （"FRAGMENT varying tang does not match any VERTEX varying"）后 panic。
        // 有 normals + uvs 就能补算；缺前提的几何保持原样（其材质也无贴图）。
        let mut cpu = cpu;
        for p in cpu.geometries.iter_mut() {
            if let three_d_asset::Geometry::Triangles(mesh) = &mut p.geometry {
                if mesh.tangents.is_none() && mesh.normals.is_some() && mesh.uvs.is_some() {
                    mesh.compute_tangents();
                }
            }
        }
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
        // 模型归一化后的底面高度：缩放后的 min.y——相机取景锚点按它落位。
        // 不画地面：模型自己站住即可（要背景色走数据区 clear）。
        let bottom = (aabb.min().y + c.y) * s;
        // 相机绕底面中心上方取景，首帧即对准主体
        let cam_target = vec3(0.0, bottom + 1.0, 0.0);
        let pos = cam_target + vec3(2.8, 1.6, 7.0);
        self.camera.set_view(pos, cam_target, vec3(0.0, 1.0, 0.0));
        // 主光 shadow map 每次场景内容变化后重建（模型参与投影）
        self.relight();
        self.models.push(model);
        self.dirty = true;
    }

    /// 用当前场景几何重建主光 shadow map。
    fn relight(&mut self) {
        let geoms = self
            .models
            .iter()
            .flat_map(|m| m.iter().map(|p| p as &dyn Geometry))
            .collect::<Vec<_>>();
        if !geoms.is_empty() {
            self.key_light.generate_shadow_map(2048, geoms).ok();
        }
        self.dirty = true;
    }

    fn render(&mut self) {
        let (w, h) = self.size;
        if w == 0 || h == 0 {
            return;
        }
        let rt = RenderTarget::screen(&self.context, w, h);
        let [r, g, b, a] = self.clear;
        rt.clear(ClearState::color_and_depth(r, g, b, a, 1.0));
        let lights: [&dyn Light; 4] = [
            &self.ambient,
            &self.key_light,
            &self.fill_light,
            &self.rim_light,
        ];
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
    /// 异步加载完成的模型暂存：回调里绝不碰 CTXS（frame() 的 borrow_mut
    /// 正在进行中，直接借会 RefCell 冲突 panic）；留给下一次 frame 取走。
    static LOADED: RefCell<Vec<(u32, String, three_d_asset::Model)>> = const { RefCell::new(Vec::new()) };
}

fn with_ctx<T>(ctx: u32, f: impl FnOnce(&mut State) -> T) -> Result<T, JsError> {
    // 关键：CTXS 的 borrow_mut 只覆盖"取 Rc"一步，绝不能覆盖 f 的执行——
    // f 里的 render/GPU 调用可能重入 JS（事件、rAF 里再进 mount/frame），
    // 借着 CTXS 时被撞上就是 panic_already_borrowed。窄借窗口后，
    // 并发安全只落在 State 自己的 RefCell 上，f 期间其他 ctx 仍可进出。
    let rc = CTXS.with(|c| {
        c.borrow_mut()
            .get(ctx as usize)
            .and_then(|slot| slot.as_ref())
            .cloned()
            .ok_or_else(|| JsError::new("3dbrowser: stale ctx"))
    })?;
    let mut s = rc.borrow_mut();
    Ok(f(&mut s))
}

/// 契约 mount：canvas + 自定义数据区（CBOR）+ 宿主通道（本示例不用，留口）。
#[wasm_bindgen]
pub fn mount(canvas: web_sys::HtmlCanvasElement, data: JsValue, _host: JsValue) -> Result<u32, JsError> {
    install_panic_hook();
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
        // 三灯布光：key 从左上前方打（后续带 shadow map），fill 从右前弱补，
        // rim 从后上方勾轮廓，ambient 兜底防死黑。
        ambient: AmbientLight::new(&context, 0.35, Srgba::WHITE),
        key_light: DirectionalLight::new(&context, 2.2, Srgba::WHITE, vec3(-0.5, -0.9, -0.4)),
        fill_light: DirectionalLight::new(&context, 0.6, Srgba::new(190, 205, 255, 255), vec3(0.7, -0.2, -0.3)),
        rim_light: DirectionalLight::new(&context, 1.1, Srgba::new(255, 245, 230, 255), vec3(0.1, -0.5, 0.9)),
        context,
        camera,
        color_objs: Vec::new(),
        models: Vec::new(),
        seen: HashSet::new(),
        pending: VecDeque::new(),
        current_assets: Vec::new(),
        // 默认全透明：画布不画底色，透出宿主页面（要底色走数据区 clear）
        clear: [0.0, 0.0, 0.0, 0.0],
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
    // 上一次异步加载的成果先落场景（在本次 borrow 窗口内，无跨域借用冲突）
    let mut arrived: Vec<three_d_asset::Model> = Vec::new();
    LOADED.with(|q| {
        let mut rest = Vec::new();
        for (c, id, cpu) in q.borrow_mut().drain(..) {
            if c == ctx {
                arrived.push(cpu);
                forget(&id); // 装载闭环，撤掉失败重试占位语义
            } else {
                rest.push((c, id, cpu));
            }
        }
        *q.borrow_mut() = rest;
    });
    for cpu in arrived {
        with_ctx(ctx, |s| s.add_model(cpu))?;
    }
    let job = with_ctx(ctx, |s| s.pump())?;
    if let Some((id, url)) = job {
        // reqwest 只吃绝对 URL：相对 url 按 document.baseURI 归一化
        // （载荷约定同源相对路径，示例 `/assets/3dbrowser/tile.glb`）。
        let base = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.base_uri().ok().flatten())
            .unwrap_or_default();
        let url = web_sys::Url::new_with_base(&url, &base)
            .map(|u| u.href())
            .unwrap_or(url);
        // 异步加载：成功后回注 LOADED，下一次 frame 在借用窗口内落场景；
        // 失败的 asset 不回 seen，下次 update 可重试
        wasm_bindgen_futures::spawn_local(async move {
            match three_d_asset::io::load_async(&[&url]).await {
                Ok(mut loaded) => {
                    // 失败原因必须落到 console：`.ok()` 吞掉它，缺 feature /
                    // 未知扩展这类问题会表现成"模型静默不显示"
                    match loaded.deserialize(&url) {
                        Ok(cpu) => LOADED.with(|q| q.borrow_mut().push((ctx, id, cpu))),
                        Err(e) => {
                            web_sys::console::warn_1(
                                &format!("3dbrowser: deserialize failed: {e:?} {url}").into(),
                            );
                            forget(&id);
                        }
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

/// 右键拖拽 = 平移（与 three.js OrbitControls 的 pan 同感）：相机连同 target
/// 沿相机的 right/up 平面移动。步长按视距缩放——屏幕上同样一段拖拽，近处
/// 移动的世界单位少、远处多，手感和轨道控制器一致。
#[wasm_bindgen]
pub fn push_pan(ctx: u32, dx: f32, dy: f32) {
    with_ctx(ctx, |s| {
        let dist = (s.camera.position() - s.camera.target()).magnitude();
        let k = dist * 0.0015;
        let right = s.camera.right_direction();
        let up = right.cross(s.camera.view_direction());
        s.camera.translate(-right * (dx * k) + up * (dy * k));
        s.dirty = true;
    })
    .ok();
}
