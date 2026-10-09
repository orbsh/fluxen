// splatviewer —— ADR 0007 模块契约的纯 JS 实现：three.js + GaussianSplats3D
// 的高斯泼溅查看器。宿主只认四个导出（mount/update/resize/unmount），数据区
// 数据区语义归本模块：
//
//   { "assets": { "scene": "/assets/splatviewer/truck.ksplat" },
//     "camera": { "up": [..], "position": [..], "lookAt": [..] } }
//
// camera 可选：3DGS 数据集的坐标系朝向各异（多数是 Y 向下），GS3D 的默认
// 相机（[0,10,15] 看向原点、up=[0,1,0]）常常是歪斜的，需按场景给出。
// 单场景展示（最新 key 生效，重复发送同 URL 幂等跳过）。three.js 与
// @mkkellogg/gaussian-splats-3d 走 CDN import map（esm.sh），本模块零构建。
// 产物部署：本文件复制到 crates/ui_leptos/assets/splatviewer/index.js 即可
// （模块纯 JS，无 wasm 产物）。

// 宿主经动态 import 加载本模块，import() 的相对说明符按本文件 URL 解析，
// 故裸包名需要运行时 import map 或完整 URL。这里直接用完整 CDN URL，
// 不依赖页面级 import map（宿主页面无 <script type="importmap"> 的控制权）。
const THREE_URL = "https://esm.sh/three@0.170.0";
const GS3D_URL = "https://esm.sh/@mkkellogg/gaussian-splats-3d@0.4.7?deps=three@0.170.0";

const THREE = await import(THREE_URL);
const GaussianSplats3D = await import(GS3D_URL);

const scenes = new Map(); // ctx -> { viewer, el, url, count, started, loaded }
// 单调递增的实例号：用 scenes.size+1 会在 unmount 之后撞号（存活实例 2、
// size 回落到 1，下一个 mount 又拿到 2），两个宿主会共用同一 scene 条目。
let nextCtx = 1;

// Viewer 自持渲染循环（selfDrivenMode）与相机控制（内置 orbit），自建 canvas
// 挂进 rootElement。注意不能用 DropInViewer——那是 three.js Group（场景对象），
// 不是 DOM 元素。
async function loadScene(ctx, url) {
  const s = scenes.get(ctx);
  // 幂等集按实例存：模块级 Set 会让"两个 box 用同一场景"或"卸了再挂同一
  // 场景"永远加载不上（第二处直接跳过）
  if (!s || s.loaded.has(url)) return;
  s.loaded.add(url);
  const abs = new URL(url, document.baseURI).href;
  try {
    // 换场景 = 先摘旧的再挂新的：addSplatScene 是追加语义，直接 add 会让
    // 多个场景叠在同一个 viewer 里（truck + bonsai 互相穿插），与"换 URL
    // 切场景"的声明不符。removeSplatScenes 收下标数组，null 会直接抛
    // （内部对入参做 for..of），所以按本实例已挂的场景数生成下标。
    if (s.count > 0) {
      await s.viewer.removeSplatScenes([...Array(s.count).keys()], false);
      s.count = 0;
    }
    await s.viewer.addSplatScene(abs, {
      splatAlphaRemovalThreshold: 5,
      showLoadingUI: false,
    });
    s.count += 1;
    // selfDrivenMode 的渲染循环必须显式 start()：init() 不启动它，漏掉
    // 就是画布永远是空白（GS3D 内部没有任何地方调用 this.start()）。
    if (!s.started) {
      s.viewer.start();
      s.started = true;
    }
  } catch (e) {
    console.warn("splatviewer: load failed", url, e);
    s.loaded.delete(url); // 失败可重试（下次 update 同 URL 再进）
  }
}

export async function mount(el, data /* ArrayBuffer(CBOR) */, _host) {
  // 数据区是 CBOR 编码的 {assets:{...}}；复用 3dbrowser 的约定，但纯 JS
  // 模块自带极简 CBOR 解码（只解本模块关心的子集：map/utf8/null/bool/int）。
  const spec = decodeCbor(new Uint8Array(data));
  const assets = (spec && spec.assets) || {};

  el.replaceChildren();
  const host = document.createElement("div");
  host.style.width = "100%";
  host.style.height = "100%";
  host.style.position = "relative";
  el.appendChild(host);

  const viewer = new GaussianSplats3D.Viewer({
    rootElement: host,
    selfDrivenMode: true,
    // gpuAcceleratedSort: true 在本机（含 headless 与用户的真机浏览器）会让
    // 场景"加载成功但一帧不画"：splat 数据全部就位（splatCount/visible 正常、
    // rAF 一直在跑），但 renderer.info.render.calls 恒为 0 —— GPU 排序路径
    // 没产出排好序的索引缓冲，绘制被跳过，画布全透明（透出宿主背景，看起来
    // 像"空白"）。改 CPU 排序（sort worker）后立刻正常出图。
    gpuAcceleratedSort: false,
    antialiased: false,
    showLoadingUI: false,
    dynamicScene: false,
    sharedMemoryForWorkers: false,
  });
  viewer.init();

  const ctx = nextCtx++;
  scenes.set(ctx, { viewer, el, url: null, count: 0, started: false, loaded: new Set() });
  // 初始数据区可能已经带着相机参数（挂载前槽里就有值），先应用一次
  applyCamera(ctx, spec && spec.camera);
  // 消费初始资产（通常为空挂载，url 后续 update 到达）
  for (const [k, v] of Object.entries(assets)) {
    if (typeof v === "string") {
      scenes.get(ctx).url = v;
      loadScene(ctx, v);
    }
  }
  return ctx;
}

export function update(ctx, data) {
  const s = scenes.get(ctx);
  if (!s) return;
  const spec = decodeCbor(new Uint8Array(data));
  const assets = (spec && spec.assets) || {};
  applyCamera(ctx, spec && spec.camera);
  for (const [k, v] of Object.entries(assets)) {
    if (typeof v === "string") {
      if (s.url !== v) {
        s.url = v;
        loadScene(ctx, v);
      }
    }
  }
}

// 相机参数（数据区可选）：`{ camera: { up:[x,y,z], position:[x,y,z], lookAt:[x,y,z] } }`
// GS3D 的默认相机是 position [0,10,15] / lookAt [0,0,0] / up [0,1,0]，而 3DGS
// 数据集普遍是 Y 向下的坐标系——默认视角因此常常歪斜甚至倒置。官方 demo 对
// 每个场景都硬编码了这三个值（见 GaussianSplats3D/demo/<scene>.html），本模块
// 把它们做成数据区参数，由生产者按场景给出。
function applyCamera(ctx, cam) {
  const s = scenes.get(ctx);
  if (!s || !cam || typeof cam !== "object") return;
  const v = s.viewer;
  const set3 = (vec, arr) => {
    if (vec && Array.isArray(arr) && arr.length === 3) vec.set(arr[0], arr[1], arr[2]);
  };
  set3(v.camera.up, cam.up);
  set3(v.camera.position, cam.position);
  if (Array.isArray(cam.lookAt) && cam.lookAt.length === 3) {
    set3(v.controls && v.controls.target, cam.lookAt);
    v.camera.lookAt(cam.lookAt[0], cam.lookAt[1], cam.lookAt[2]);
  }
  if (v.controls && typeof v.controls.update === "function") v.controls.update();
}

export function resize(_ctx, _w, _h) {
  // DropInViewer 自监听容器尺寸（ResizeObserver 内置），无需转发。
}

export function unmount(ctx) {
  const s = scenes.get(ctx);
  if (!s) return;
  scenes.delete(ctx);
  try { s.viewer.dispose(); } catch (_e) { /* 已释放 */ }
  s.el.replaceChildren();
}

// ---- 极简 CBOR 解码（RFC 8949 子集）--------------------------------
// 只覆盖本模块数据区会出现的形式：map(1..n)、utf8 string、null、
// 16/32-bit int。数组/浮点/tag/其他 major type 一律抛错——数据区 schema
// 漂移时宁可响亮失败，不要静默错配。
function decodeCbor(buf) {
  let i = 0;
  const read = (n) => {
    const v = buf.subarray(i, i + n);
    i += n;
    return v;
  };
  const uint = (n) => read(n).reduce((a, b) => (a << 8) | b, 0) >>> 0;
  const head = () => {
    const ib = buf[i++];
    const mt = ib >> 5;
    const ai = ib & 0x1f;
    let arg;
    if (ai < 24) arg = ai;
    else if (ai === 24) arg = uint(1);
    else if (ai === 25) arg = uint(2);
    else if (ai === 26) arg = uint(4);
    else throw new Error(`cbor: unsupported additional info ${ai}`);
    return [mt, arg];
  };
  const value = () => {
    // 先窥头字节：mt 7（简单值 / 浮点）要按 additional info 分派，且
    // f32(ai=26)/f64(ai=27) 必须按 IEEE754 解读——serde 的 CBOR 编码把
    // f64 写成 ai=27，而 head() 只认 ai ≤ 26，遇小数即抛
    // "unsupported additional info 27"（camera 的小数就是踩在这上面）。
    const ib = buf[i];
    const mt7 = ib >> 5;
    const ai7 = ib & 0x1f;
    if (mt7 === 7) {
      i += 1;
      const ieee = (n, get) => {
        const v = read(n);
        return new DataView(v.buffer, v.byteOffset, n)[get](0);
      };
      if (ai7 === 20) return false;
      if (ai7 === 21) return true;
      if (ai7 === 22) return null;
      if (ai7 === 26) return ieee(4, "getFloat32");
      if (ai7 === 27) return ieee(8, "getFloat64");
      throw new Error(`cbor: unsupported simple value ${ai7}`);
    }
    const [mt, arg] = head();
    if (mt === 0) return arg;
    if (mt === 1) return -1 - arg; // 负整数：truck 的 camera 有 -5 / -1
    if (mt === 2) return read(arg); // byte string（不该出现在数据区，透传）
    if (mt === 3) return new TextDecoder().decode(read(arg));
    if (mt === 4) {
      // array：camera 的 up/position/lookAt 都是 [x,y,z]
      const arr = [];
      for (let k = 0; k < arg; k++) arr.push(value());
      return arr;
    }
    if (mt === 5) {
      // map：数据区里 assets 的值就是一层嵌套 map，
      // {assets:{scene:"/…"}} 的 assets 分支必须能解
      const out = {};
      for (let k = 0; k < arg; k++) {
        const key = value();
        out[key] = value();
      }
      return out;
    }
    throw new Error(`cbor: unsupported major type ${mt}`);
  };
  // 顶层就是 map（0xa0 = 空 map，循环零次即 {}）
  const top = head();
  if (top[0] !== 5) throw new Error(`cbor: top-level must be a map, got major type ${top[0]}`);
  const out = {};
  for (let k = 0; k < top[1]; k++) {
    const key = value();
    out[key] = value();
  }
  return out;
}
