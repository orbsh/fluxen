// 3dbrowser 的 CDN 入口 —— ADR 0007 模块契约的实现包装。
// 宿主（ui_leptos Canvas 组件）只 import 本文件，调用
//   mount(el, data, host) -> Promise<ctx> / update(ctx, data) / resize(ctx, w, h) / unmount(ctx)
// 渲染循环与输入事件归本模块（“模块自持事件循环”），wasm 字节码由
// wasm-bindgen glue（./3dbrowser.js，同目录）内部加载。
import init, * as wasm from "./3dbrowser.js";

const runtime = init(); // 首次挂载时下载/实例化 wasm；重复调用共享同一 Promise

async function ensure() {
  await runtime;
}

export async function mount(el, data, host) {
  await ensure();
  const canvas = document.createElement("canvas");
  canvas.style.width = "100%";
  canvas.style.height = "100%";
  canvas.style.display = "block";
  canvas.style.touchAction = "none";
  el.replaceChildren(canvas);

  let dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(el.clientWidth * dpr));
  const h = Math.max(1, Math.round(el.clientHeight * dpr));
  canvas.width = w;
  canvas.height = h;

  const ctx = wasm.mount(canvas, data, host);

  // rAF 循环归模块；宿主只负责 update/resize/unmount
  const view = { ctx, canvas, el, raf: 0, drag: null };
  const loop = () => {
    try {
      wasm.frame(view.ctx);
    } catch (e) {
      console.warn("3dbrowser frame stopped:", e);
      return;
    }
    view.raf = requestAnimationFrame(loop);
  };
  view.raf = requestAnimationFrame(loop);

  canvas.addEventListener("pointerdown", (ev) => {
    canvas.setPointerCapture(ev.pointerId);
    view.drag = { x: ev.clientX, y: ev.clientY };
  });
  canvas.addEventListener("pointermove", (ev) => {
    if (!view.drag) return;
    wasm.push_drag(view.ctx, ev.clientX - view.drag.x, ev.clientY - view.drag.y);
    view.drag = { x: ev.clientX, y: ev.clientY };
  });
  const endDrag = () => (view.drag = null);
  canvas.addEventListener("pointerup", endDrag);
  canvas.addEventListener("pointercancel", endDrag);
  canvas.addEventListener(
    "wheel",
    (ev) => {
      ev.preventDefault();
      wasm.push_wheel(view.ctx, -ev.deltaY * 0.01);
    },
    { passive: false },
  );

  // unmount 走 ctx id：JS 侧句柄表把 id 映射到 view（同一 id 可被 wasm 复用）
  registry.set(ctx, view);
  return ctx;
}

export function update(ctx, data) {
  wasm.update(ctx, data);
}

export function resize(ctx, w, h) {
  const view = registry.get(ctx);
  if (!view) return;
  const dpr = window.devicePixelRatio || 1;
  view.canvas.width = Math.max(1, Math.round(w * dpr));
  view.canvas.height = Math.max(1, Math.round(h * dpr));
  wasm.resize(ctx, view.canvas.width, view.canvas.height);
}

export function unmount(ctx) {
  const view = registry.get(ctx);
  if (!view) return;
  cancelAnimationFrame(view.raf);
  registry.delete(ctx);
  wasm.unmount(ctx);
  view.el.replaceChildren();
}

const registry = new Map();
