// splatviewer —— ADR 0007 模块契约的纯 JS 实现：three.js + GaussianSplats3D
// 的高斯泼溅查看器。宿主只认四个导出（mount/update/resize/unmount），数据区
// 语义归本模块：
//
//   { "assets": { "scene": "/assets/splatviewer/truck.ksplat" } }
//
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

const scenes = new Map(); // ctx -> { viewer, el, raf, url }
const loaded = new Set(); // 已加载/加载中的 url（幂等）

// Viewer 自持渲染循环（selfDrivenMode）与相机控制（内置 orbit），自建 canvas
// 挂进 rootElement。注意不能用 DropInViewer——那是 three.js Group（场景对象），
// 不是 DOM 元素。
async function loadScene(ctx, url) {
  const s = scenes.get(ctx);
  if (!s || loaded.has(url)) return;
  loaded.add(url);
  const abs = new URL(url, document.baseURI).href;
  try {
    await s.viewer.addSplatScene(abs, {
      splatAlphaRemovalThreshold: 5,
      showLoadingUI: false,
    });
  } catch (e) {
    console.warn("splatviewer: load failed", url, e);
    loaded.delete(url); // 失败可重试（下次 update 同 URL 再进）
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
    gpuAcceleratedSort: true,
    antialiased: false,
    showLoadingUI: false,
    dynamicScene: false,
    sharedMemoryForWorkers: false,
  });
  viewer.init();

  const ctx = scenes.size + 1;
  scenes.set(ctx, { viewer, el, url: null });
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
  for (const [k, v] of Object.entries(assets)) {
    if (typeof v === "string") {
      if (s.url !== v) {
        s.url = v;
        loadScene(ctx, v);
      }
    }
  }
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
    const [mt, arg] = head();
    if (mt === 0) return arg;
    if (mt === 2) return read(arg); // byte string（不该出现在数据区，透传）
    if (mt === 3) return new TextDecoder().decode(read(arg));
    if (mt === 7 && arg === 20) return false;
    if (mt === 7 && arg === 21) return true;
    if (mt === 7 && arg === 22) return null;
    throw new Error(`cbor: unsupported major type ${mt}`);
  };
  const [, n] = head();
  if (n === 0) return {}; // 空 map（首帧空挂载）
  const out = {};
  for (let k = 0; k < n; k++) {
    const key = value();
    out[key] = value();
  }
  return out;
}
