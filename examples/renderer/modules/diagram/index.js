// diagram —— mermaid 封装模块：ADR 0007 契约的 ES module 入口（mermaid 由本模块
// 按需加载，ADR 0015：模块求值期零下载，到 mount() 才拉）。
//
// 载荷（`bind["value"]` 的 CBOR 字节）语义归本模块：
//
//   "sequenceDiagram\n  A->>B: hi"        纯字符串，或
//   {"code": "<mermaid 源码>"}            对象形态（给以后带主题/方向的写法留位）
//
// 为什么它是模块而不是框架里的一个节点：**种类要住在数据里**。框架认识
// `canvas` 与它的 url，不认识 mermaid——换流程图库是换 url，不是改 fluxen。
// 原来那个 `diagram` 节点类型（内联加载器 + dom::eval）已删除，这就是它的替代。
//
// 部署：本目录整体拷入 <assets>/diagram/，mermaid.min.js 放 assets 根
// （与原来的 /assets/mermaid.min.js 同一份，页面其它地方也在用）。
import { decode } from "./cbor.js";

// 相对本模块解析：模块在 /assets/diagram/，库在 /assets/mermaid.min.js。
const LIB_URL = new URL("../mermaid.min.js", import.meta.url).href;

let lib = null;
function ensureLib() {
  if (window.mermaid) return Promise.resolve(window.mermaid);
  if (!lib) {
    lib = new Promise((res, rej) => {
      const s = document.createElement("script");
      s.src = LIB_URL;
      s.onload = () => {
        // 关掉自动扫描：这个 bundle 找不到调用方之外还有谁要渲染，
        // 开着会在每次脚本加载后重扫整页（实测把已渲染的图重画一遍）。
        try {
          window.mermaid.initialize({ startOnLoad: false });
        } catch (_) {}
        res(window.mermaid);
      };
      s.onerror = () => rej(new Error("mermaid load failed: " + LIB_URL));
      document.head.appendChild(s);
    });
  }
  return lib;
}

function srcOf(bytes) {
  if (!bytes || !bytes.length) return "";
  const v = decode(bytes);
  if (typeof v === "string") return v;
  if (v && typeof v === "object") {
    return v.code ?? v.src ?? v.default ?? v.source ?? "";
  }
  return "";
}

const views = new Map();
let nextId = 1;

async function draw(view) {
  view.el.replaceChildren();
  const box = document.createElement("div");
  box.className = "mermaid";
  box.textContent = view.src;
  view.el.appendChild(box);
  if (!view.src) return;
  const m = await ensureLib();
  // v10 是 run()，v9 是 init()；两个都留着是因为库的版本由部署决定，
  // 而这个模块不该跟着库升级一起改。
  if (typeof m.run === "function") {
    await m.run({ nodes: [box] });
  } else if (typeof m.init === "function") {
    m.init(undefined, box);
  }
}

export async function mount(el, data, _host, _options) {
  const view = { el, src: srcOf(data) };
  const id = nextId++;
  views.set(id, view);
  await draw(view);
  return id;
}

export async function update(id, data) {
  const view = views.get(id);
  if (!view) return;
  const next = srcOf(data);
  if (next === view.src) return;
  view.src = next;
  await draw(view);
}

// 图宽由容器给（mermaid 的 svg 自带 max-width:100%），没有需要主动处理的尺寸。
export function resize(_id, _w, _h) {}

export function unmount(id) {
  const view = views.get(id);
  if (!view) return;
  view.el.replaceChildren();
  views.delete(id);
}
