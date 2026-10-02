// g2chart —— ADR 0009 的 Chart 封装模块：ADR 0007 契约的 ES module 入口。
// 宿主 Chart/Canvas 组件 import 本文件（url 默认 /assets/g2chart/index.js）。
// G2 库是 UMD（不可 import，只能 globalThis.G2），由本模块以 <script>
// 标签自举加载——onload 门控，浏览器按 URL 缓存，多实例/重复挂载共享
// 一次下载。spec 载荷 = bind["value"] 的 CBOR 字节（宿主统一编码），
// 解码用 ./cbor.js（最小 RFC 8949 实现，覆盖 ciborium 产出子集）。
// 部署：同目录放 g2.min.js（tarball dist，版本固定）；本目录整体
// 拷入 crates/ui_leptos/assets/g2chart/（.gitignore，配方在此注释）。
import { decode } from "./cbor.js";

// UMD 只能 script 标签加载；src 必须按【本模块的 URL】解析
// （new URL + import.meta.url）——script 的相对 src 按文档 base 解析，
// 页面在 / 时 `./g2.min.js` 指向 /g2.min.js = 404（CDP 实测抓到）。
const G2_URL = new URL("g2.min.js", import.meta.url).href;
let g2ready = null;
function ensureG2() {
  if (window.G2) return Promise.resolve();
  if (!g2ready) {
    g2ready = new Promise((res, rej) => {
      const s = document.createElement("script");
      s.src = G2_URL;
      s.onload = res;
      s.onerror = () => rej(new Error("g2 load failed: " + G2_URL));
      document.head.appendChild(s);
    });
  }
  return g2ready;
}

const views = new Map();
let nextId = 1;

function specOf(bytes) {
  if (!bytes || !bytes.length) return {};
  const v = decode(bytes);
  return v && typeof v === "object" ? v : {};
}

export async function mount(el, data, _host) {
  await ensureG2();
  const box = document.createElement("div");
  box.style.width = "100%";
  box.style.height = "100%";
  el.replaceChildren(box);
  const chart = new window.G2.Chart({ container: box, autoFit: true });
  chart.options(specOf(data));
  await chart.render();
  const id = nextId++;
  views.set(id, { chart, box });
  return id;
}

export function update(id, data) {
  const v = views.get(id);
  if (!v) return;
  // 流式追加（token 进 spec 的 data 数组）：整 spec 重设 + 差量重绘。
  // G2 v5 没有 chart.update()（实测 typeof === "undefined"，调用抛
  // TypeError：宿主只打 warn，图不动）——options() 只做 spec 树差量、
  // 不落笔，重绘必须显式 render()。增量只在同实例续用下成立：
  // render() 复用已有 mark/composition，画布层由 G2 渲染插件按脏对象
  // 区域重绘（引擎自带 dirty rect 由它接管）。
  v.chart.options(specOf(data));
  return v.chart.render();
}

export function resize(id, w, h) {
  const v = views.get(id);
  if (v && w > 0 && h > 0) v.chart.changeSize(w, h);
}

export function unmount(id) {
  const v = views.get(id);
  if (!v) return;
  v.chart.destroy();
  v.box.remove();
  views.delete(id);
}
