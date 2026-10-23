// forcegraph —— ADR 0007 模块契约的纯 JS 实现：3d-force-graph（vasturiano）的
// 力导向关系图。宿主只认四个导出（mount/update/resize/unmount），本模块零构建。
//
// 数据区（`bind["value"]` 的 CBOR 字节）语义归本模块：
//
//   { "nodes": [{ "id", "name"?, "kind"?, "weight"?, "color"? }],
//     "links": [{ "source", "target", "kind"?, "strength"?, "color"? }] }
//
// `nodes[].id` 是身份（增量按它去重）；`links[].source/target` 写节点 id。
// `weight`（缺省按第 0 档）决定节点直径档位，`strength`（缺省 1px）决定边宽档位，
// 映射规则见下面 buildOptions() 的注释。`kind` 只决定配色（同 kind 恒同色）。
//
// 渲染配置走 mount 的第四参 options（ADR 0017：宿主存一个动态袋，一个键都不解释）：
// 本模块把 options 铺在自己的 DEFAULTS 上——缺键用默认、未知键忽略。所以 DSL 里
// 换一套节点/边尺寸、力参数、配色都只是 canvas 节点的 options 内容，核心无感。
//
// 三条 k10r 实测踩出来的硬约束（照抄，别"优化"回去）：
//   ① `.d3AlphaMin(0)`——物理不许自动停。默认 alpha 会衰减到 alphaMin 后引擎自停，
//      而 `d3ReheatSimulation()` 只把 alpha 设回 1、不重启引擎，于是流式后半段进来
//      的节点永不参与布局（实测：20 个数据包只布局了前 6 个）。
//   ② 边端点在交给引擎之前必须自己解析成节点对象。库按引用持有 links 数组，端点还是
//      字符串时物理 tick 会去写 `.vx` 抛 TypeError，异常落在 `_animationCycle` 里、
//      而续帧是它最后一句——画布从此冻住（节点铺满、一条边都不画）。
//   ③ 一次 update = 一次 `graphData()`。一包一次等于把物理引擎按包重推。
//
// 库是 UMD（自带 three.js，暴露全局 ForceGraph3D）。**不用 <script> 标签加载**：
// UMD 的出口顺序是 CommonJS → AMD → 全局，页面里只要有人定义了 AMD 加载器（Monaco
// 就定义了 `define.amd`），它就注册成匿名 AMD 模块、全局一个字节都不落——实测症状
// 是 editor 挂载之后关系图必然 `mount rejected: window.ForceGraph3D is not a
// function`，而同一页面在没有 Monaco 时一切正常。所以：把源码当文本取来，在一个
// **把 module/exports/define 遮蔽成 undefined** 的模块作用域里求值（blob → import），
// UMD 就只剩"挂 globalThis"那条路，页面自己的 define 全程不受影响。
// 代价：页面若开了 CSP 需要放行 `blob:`；库地址要能 fetch（同源或带 CORS）。
// 默认同源 `./3d-force-graph.min.js`（v1.80.1，装配配方见 Dockerfile），同源取不到
// 才回落钉版本的 unpkg；options.libUrl 可显式覆盖。
// 部署：`examples/modules/forcegraph/{index.js,cbor.js}` + 库文件一起拷进
// `crates/ui_leptos/assets/forcegraph/`（该目录 gitignore，`trunk build` 的
// copy-dir 会把它带进 dist），示例帧见 `examples/yaml/19.canvas.forcegraph.yaml`。
import { decode } from "./cbor.js";

const LIB_URL = new URL("3d-force-graph.min.js", import.meta.url).href;
const LIB_CDN = "https://unpkg.com/3d-force-graph@1.80.1/dist/3d-force-graph.min.js";

// ---- 库按需加载（ADR 0015：模块求值期零下载，到 mount() 才拉） -------------
let libReady = null;

// 判"库在不在"用 typeof === "function"：UMD 的赋值语句没跑到时，这个全局可能是
// undefined，真值判断会把"没落位"和"落位了"混成一种。
const libLoaded = () => typeof window.ForceGraph3D === "function";

async function loadLib(url) {
  const res = await fetch(url, { credentials: "same-origin" });
  if (!res.ok) throw new Error(`forcegraph: lib fetch ${res.status} ${url}`);
  const src = await res.text();
  const prelude = "const module = undefined, exports = undefined, define = undefined;\n";
  const blobUrl = URL.createObjectURL(new Blob([prelude, src], { type: "text/javascript" }));
  try {
    await import(blobUrl);
  } finally {
    URL.revokeObjectURL(blobUrl); // 求值已完成，句柄不必留
  }
}

async function ensureLib(override) {
  if (libLoaded()) return;
  const url = override || LIB_URL;
  try {
    if (!libReady) libReady = loadLib(url);
    await libReady;
  } catch (e) {
    libReady = null;
    // 显式给了 libUrl 就是那一条腿：失败照抛，别偷偷换地址；
    // 默认同源那条腿失败才回落 CDN（k10r 也是本地副本 + 钉版本 CDN 两条腿）。
    if (override) throw e;
    libReady = loadLib(LIB_CDN);
    try {
      await libReady;
    } catch (e2) {
      libReady = null;
      throw e2;
    }
  }
  if (!libLoaded()) {
    throw new Error("forcegraph: lib executed but window.ForceGraph3D is " + typeof window.ForceGraph3D);
  }
}

// ---- 默认配置（options 未知键忽略、缺键用这里的值） -------------------------
const DEFAULTS = {
  // 节点直径阶梯：按**屏幕像素**给（库的尺寸全是世界单位，见 pxPerUnit 注释）
  nodeSizePx: 2,        // 第 0 档直径
  nodeSizeStepPx: 2,    // 每档 +2px（线性阶梯：末档是默认档 4 倍，不是 8 倍）
  nodeTiers: 4,         // 档数
  nodeWeightBase: 3,    // weight ≤ 它落第 0 档
  nodeWeightSpan: 2,    // 每档跨多少 weight
  // 边宽阶梯
  linkWidthPx: 1,
  linkWidthStepPx: 1,
  linkStrongAt: 4,      // strength ≥ 它进第 1 档
  linkOpacity: 0.9,     // 库默认 0.2：细圆柱在深底上几乎看不见（实测被批"看不见边"）
  linkColor: "#3a4a72",
  arrowPx: 2,           // 方向箭头长度（屏幕 px）；0 = 不画
  particles: 0,         // >0 时沿边画粒子（个数 = 强度/2）；0 = 关
  // 相机与物理：k10r 实测这组（link 120 / charge -200 / 相机 1660）收敛后整张图
  // 约占视口 80%，边均长约 90px
  cameraDistance: 1660,                   // 仅在 autoFit=false 时作为固定机位
  cameraDirection: [0.636, 0.437, 0.636], // 斜 45°：正对 z 轴会把深度压平成 2D
  charge: -200,
  linkDistance: 120,
  // 取景：新增节点后把相机拟合到整张图（图多大都填满画面）。拟合后按**拟合距离**
  // 重算像素基准，所以下面那些 px 尺寸在拟合后的画面上仍然成立。
  // autoFit=false → 用 cameraDistance/cameraDirection 的固定机位（k10r 的做法）。
  autoFit: true,
  fitPaddingPx: 40,
  fitDebounceMs: 600,
  fitSettleMs: 900,         // 每批数据之后再补一次取景：物理还在铺开，边到边收镜头
  background: "rgba(0,0,0,0)",            // 透明：露出宿主页面的底色/渐变
  // 常显标签：节点显示 `name`（缺省用 id），边显示 `kind`。悬停才给的详细信息走
  // 悬停卡片（tooltip），两者分工：标签是"这张图在说什么"，卡片是"这个点是什么"。
  // 标签是 HTML 覆盖层（pointer-events:none），不占 WebGL——文字永远清晰、改样式
  // 不用碰场景。节点数超过 labelMaxNodes 时整体关掉（大图上一千个 div 会拖帧）。
  labels: true,
  labelMaxNodes: 150,
  nodeLabelPx: 11,
  linkLabelPx: 10,
  labelColor: "#334155",
  linkLabelColor: "#94a3b8",
  labelOffsetPx: 6,         // 标签离节点球面的距离（px）
  labelDedupe: true,        // 同屏重叠时后来的标签先藏（每帧重算，位置一变就回来）
  // 配色：按 kind 的哈希取色（同一 kind 恒同色，跨实例一致）
  palette: ["#818cf8", "#34d399", "#38bdf8", "#f59e0b", "#f472b6", "#a78bfa", "#22d3ee", "#fb7185"],
  libUrl: null,         // 覆盖库地址（默认同源 ./3d-force-graph.min.js）
};

function buildOptions(raw) {
  const o = { ...DEFAULTS };
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return o;
  for (const [k, v] of Object.entries(raw)) {
    if (!(k in DEFAULTS)) {
      console.warn("forcegraph: unknown option", k);
      continue;
    }
    if (k === "palette") {
      if (Array.isArray(v) && v.length) o.palette = v.filter((c) => typeof c === "string");
      else console.warn("forcegraph: bad option", k, v);
    } else if (k === "cameraDirection") {
      if (Array.isArray(v) && v.length === 3 && v.every((n) => Number.isFinite(Number(n)))) {
        o.cameraDirection = v.map(Number);
      } else console.warn("forcegraph: bad option", k, v);
    } else if (k === "libUrl") {
      if (typeof v === "string" && v) o.libUrl = v;
      else console.warn("forcegraph: bad option", k, v);
    } else if (typeof v === typeof DEFAULTS[k]) {
      o[k] = v;
    } else {
      // 一个坏配置不该把图渲染成白屏：说一声、这一项用默认
      console.warn("forcegraph: bad option", k, v);
    }
  }
  return o;
}

// ---- 尺寸换算 ---------------------------------------------------------------
// 尺寸按屏幕像素定，而库的尺寸全是世界单位，所以先算"1 世界单位 = 多少像素"。
// 基准 = **本 canvas 自己的高度** + 相机距离 + fov（不是 window.innerHeight：
// canvas 是嵌在页面里的一块，按视口折算会让 2px 变成 4px）。autoFit 拟合后
// 相机距离会变，refit() 会用拟合后的距离重算一遍，像素承诺因此仍然成立。
function pxPerUnitAt(s, dist) {
  const fov = s.Graph.camera()?.fov || 45;
  const h = s.Graph.height() || s.el.clientHeight || 800;
  return h / (2 * dist * Math.tan((fov * Math.PI) / 360));
}

function cameraDistanceOf(s) {
  const p = s.Graph.cameraPosition();
  const t = p.lookAt || { x: 0, y: 0, z: 0 };
  return Math.hypot(p.x - t.x, p.y - t.y, p.z - t.z) || s.opt.cameraDistance;
}

// 取景 + 像素基准重算（两趟）：先让相机拟合整张图，读出拟合后的相机距离，再按
// 那个距离重算像素基准并重造节点球（nodeVal 只在造对象时读），最后再拟合一次。
// 于是"节点 2px 直径"指的是拟合后画面上的 2px，而不是"默认机位下 2px"。
function refit(s) {
  if (!s.opt.autoFit || !s.graphData.nodes.length) return;
  const now = performance.now();
  if (now - s.lastFitAt < s.opt.fitDebounceMs) return; // 流式多帧时不逐帧甩镜头
  s.lastFitAt = now;
  s.Graph.zoomToFit(0, s.opt.fitPaddingPx);
  const k = pxPerUnitAt(s, cameraDistanceOf(s));
  if (!Number.isFinite(k) || k <= 0) return;
  if (Math.abs(k - s.pxPerUnit) / s.pxPerUnit > 0.05) {
    s.pxPerUnit = k;
    s.Graph.graphData(s.graphData); // 重造球体：nodeVal 读的是新的 pxPerUnit
    s.Graph.zoomToFit(0, s.opt.fitPaddingPx);
  }
}

// 尾随取景：一批数据进来时物理才刚开始铺开，那一刻拟合出来的范围偏小（镜头太近、
// 图挤在一角）。塌实之后补一次 —— 图长出来的过程里镜头跟着走，最后落在一个填满
// 画面的位置上。抖动的只有相机，节点本身不受影响。
function scheduleFit(s) {
  if (!s.opt.autoFit) return;
  clearTimeout(s.fitTimer);
  s.fitTimer = setTimeout(() => {
    s.fitTimer = null;
    s.lastFitAt = 0; // 尾随这一趟必须真跑，绕过防抖
    refit(s);
  }, s.opt.fitSettleMs);
}

function tierOf(weight, base, span, tiers) {
  if (typeof weight !== "number" || !Number.isFinite(weight)) return 0;
  return Math.max(0, Math.min(tiers - 1, Math.floor((weight - base) / span)));
}

function nodeRadiusWorld(s, n) {
  const o = s.opt;
  const dia = o.nodeSizePx + o.nodeSizeStepPx * tierOf(n.weight, o.nodeWeightBase, o.nodeWeightSpan, o.nodeTiers);
  return dia / 2 / (s.pxPerUnit || 1);
}

function linkWidthWorld(s, l) {
  const o = s.opt;
  const strong = typeof l.strength === "number" && l.strength >= o.linkStrongAt;
  const px = strong ? o.linkWidthPx + o.linkWidthStepPx : o.linkWidthPx;
  return px / (s.pxPerUnit || 1);
}

// ---- 配色与提示 -------------------------------------------------------------
function hashOf(str) {
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return Math.abs(h);
}

function paletteColor(s, kind) {
  const pal = s.opt.palette.length ? s.opt.palette : DEFAULTS.palette;
  if (kind === undefined || kind === null || kind === "") return pal[0];
  return pal[hashOf(String(kind)) % pal.length];
}

const esc = (v) =>
  String(v).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

// ---- 悬停卡片与常显标签的样式 -----------------------------------------------
// 库自带一个 .float-tooltip-kap 外壳（padding/border-radius/background 各一套），
// 里面再套我们自己的卡片就变成双层框、看着脏。这里把它抹平，只留我们这一层：
// 无边框、无阴影、一个半透明底。
const STYLE_ID = "forcegraph-module-style";
const CSS = `
.fg-host { position: relative; }
.fg-host .float-tooltip-kap {
  padding: 0; border: 0; border-radius: 0; background: transparent;
  box-shadow: none; color: inherit; font: inherit; max-width: none;
}
.fg-labels { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
.fg-label {
  position: absolute; left: 0; top: 0; white-space: nowrap;
  transform: translate(-50%, -50%); will-change: transform;
  line-height: 1.2;
}
`;

function ensureStyles() {
  if (document.getElementById(STYLE_ID)) return;
  const st = document.createElement("style");
  st.id = STYLE_ID;
  st.textContent = CSS;
  document.head.appendChild(st);
}

// 悬停卡片：单层、无阴影、两列对齐。内容仍由数据给（name/kind/weight…）。
function tooltip(title, pairs) {
  const rows = pairs
    .filter(([, v]) => v !== undefined && v !== null && v !== "")
    .map(
      ([k, v]) =>
        `<div style="display:flex;gap:.75rem;justify-content:space-between"><span style="opacity:.62">${esc(k)}</span><span>${esc(v)}</span></div>`,
    )
    .join("");
  return (
    `<div style="background:rgba(15,23,42,.9);color:#e2e8f0;border-radius:.45rem;` +
    `padding:.4rem .55rem;font:12px/1.45 system-ui,-apple-system,sans-serif;min-width:7.5rem">` +
    `<div style="font-weight:600;margin-bottom:.15rem">${esc(title)}</div>${rows}</div>`
  );
}

// ---- 常显标签（HTML 覆盖层） -------------------------------------------------
// 投影自己算：相机矩阵就在 Graph.camera() 上，乘两下比调库的 getScreenCoords 省一次
// 坐标空间猜谜（那个返回的是 canvas 像素、不是 CSS 像素）。返回值 x/y 归一化到
// [0,1]，w 是裁剪空间的 w —— w <= 0 表示点在相机背后，标签必须藏起来，否则会
// 镜像出现在错误位置。
function project(s, x, y, z) {
  const cam = s.Graph.camera();
  if (!cam) return null;
  const v = cam.matrixWorldInverse.elements;
  const p = cam.projectionMatrix.elements;
  const vx = v[0] * x + v[4] * y + v[8] * z + v[12];
  const vy = v[1] * x + v[5] * y + v[9] * z + v[13];
  const vz = v[2] * x + v[6] * y + v[10] * z + v[14];
  const vw = v[3] * x + v[7] * y + v[11] * z + v[15];
  const cx = p[0] * vx + p[4] * vy + p[8] * vz + p[12] * vw;
  const cy = p[1] * vx + p[5] * vy + p[9] * vz + p[13] * vw;
  const cw = p[3] * vx + p[7] * vy + p[11] * vz + p[15] * vw;
  if (!cw) return null;
  return { x: (cx / cw + 1) / 2, y: (1 - cy / cw) / 2, w: cw };
}

function makeLabel(s, kind) {
  const d = document.createElement("div");
  d.className = "fg-label";
  if (kind === "link") {
    d.style.font = `${s.opt.linkLabelPx}px/1.2 system-ui, -apple-system, sans-serif`;
    d.style.color = s.opt.linkLabelColor;
  } else {
    d.style.font = `600 ${s.opt.nodeLabelPx}px/1.2 system-ui, -apple-system, sans-serif`;
    d.style.color = s.opt.labelColor;
  }
  return d;
}

function ensureOverlay(s) {
  if (!s.labelsEl) {
    s.labelsEl = document.createElement("div");
    s.labelsEl.className = "fg-labels";
    s.el.appendChild(s.labelsEl);
    startLabels(s);
  }
  return s.labelsEl;
}

function addNodeLabel(s, node) {
  if (!s.opt.labels || s.nodeLabels.has(node)) return;
  // 超过上限就不再标：一千个 div 每帧重排会拖垮渲染，宁可只画不标（说一次就算）
  if (s.graphData.nodes.length > s.opt.labelMaxNodes) {
    if (!s.labelCapped) {
      s.labelCapped = true;
      console.info(
        `forcegraph: ${s.graphData.nodes.length} 个节点超过 labelMaxNodes=${s.opt.labelMaxNodes}，只画不标`,
      );
      dropLabels(s);
    }
    return;
  }
  const el = makeLabel(s, "node");
  el.textContent = node.name ?? node.id;
  ensureOverlay(s).appendChild(el);
  s.nodeLabels.set(node, el);
}

function addLinkLabel(s, link) {
  if (!s.opt.labels || s.linkLabels.has(link)) return;
  if (s.graphData.nodes.length > s.opt.labelMaxNodes) return;
  const name = link.kind ?? link.name;
  if (name === undefined || name === null || name === "") return; // 没名字的边不占位
  const el = makeLabel(s, "link");
  el.textContent = String(name);
  ensureOverlay(s).appendChild(el);
  s.linkLabels.set(link, el);
}

function dropLabels(s) {
  if (s.labelRaf) cancelAnimationFrame(s.labelRaf);
  s.labelRaf = null;
  s.nodeLabels.clear();
  s.linkLabels.clear();
  if (s.labelsEl) {
    s.labelsEl.remove();
    s.labelsEl = null;
  }
}

// 重叠就藏：29 个标签挤在一张紧凑的图上必然互相压字。占位表每帧重建——
// 节点标签优先（它们回答"这是什么"），边标签补空位；位置一变标签自己就回来。
function placeLabel(el, cx, cy, taken, dedupe) {
  const w = el.__w || 40;
  const h = el.__h || 12;
  const left = cx - w / 2;
  const top = cy - h / 2;
  if (dedupe && taken.some((r) => left < r.r && left + w > r.l && top < r.b && top + h > r.t)) {
    el.style.display = "none";
    return;
  }
  el.style.display = "";
  el.style.transform = `translate(-50%, -50%) translate(${cx}px, ${cy}px)`;
  if (dedupe) taken.push({ l: left, t: top, r: left + w, b: top + h });
}

function positionLabels(s) {
  const rect = s.el.getBoundingClientRect();
  const w = rect.width;
  const h = rect.height;
  s.dedupeTaken = s.dedupeTaken || [];
  const taken = s.dedupeTaken;
  taken.length = 0;
  const inView = (x, y) => x > -60 && y > -60 && x < w + 60 && y < h + 60;

  for (const [node, el] of s.nodeLabels) {
    const p = project(s, node.x ?? 0, node.y ?? 0, node.z ?? 0);
    if (!p || p.w <= 0) {
      el.style.display = "none";
      continue;
    }
    const cx = p.x * w;
    const cy = p.y * h;
    if (!inView(cx, cy)) {
      el.style.display = "none";
      continue;
    }
    // 抬到球面之上：球半径换算成屏幕像素，再加 labelOffsetPx
    const rise = nodeRadiusWorld(s, node) * (s.pxPerUnit || 1) + s.opt.labelOffsetPx;
    placeLabel(el, cx, cy - rise, taken, s.opt.labelDedupe);
  }
  for (const [link, el] of s.linkLabels) {
    const a = project(s, link.source.x ?? 0, link.source.y ?? 0, link.source.z ?? 0);
    const b = project(s, link.target.x ?? 0, link.target.y ?? 0, link.target.z ?? 0);
    if (!a || !b || a.w <= 0 || b.w <= 0) {
      el.style.display = "none";
      continue;
    }
    const cx = ((a.x + b.x) / 2) * w;
    const cy = ((a.y + b.y) / 2) * h;
    if (!inView(cx, cy)) {
      el.style.display = "none";
      continue;
    }
    placeLabel(el, cx, cy, taken, s.opt.labelDedupe);
  }
}

function startLabels(s) {
  if (s.labelRaf) return;
  const tick = () => {
    if (!graphs.has(s.id)) {
      s.labelRaf = null;
      return;
    }
    positionLabels(s);
    s.labelRaf = requestAnimationFrame(tick);
  };
  s.labelRaf = requestAnimationFrame(tick);
}

// ---- 实例表 -----------------------------------------------------------------
const graphs = new Map(); // ctx -> state
let nextId = 1; // 单调递增：用 size+1 会在 unmount 之后撞号（两个宿主共用一条状态）

const endId = (v) => (v && typeof v === "object" ? String(v.id) : String(v));

function payloadOf(bytes) {
  if (!bytes) return {};
  if (!(bytes.byteLength ?? bytes.length)) return {};
  const v = decode(bytes);
  return v && typeof v === "object" ? v : {};
}

// 增量：整份快照重复送达是常态（一个 update 一帧载荷），按 id 去重才是幂等的。
// 端点解析、一次推进、reheat —— 三条硬约束都在这里（见文件头 ①②③）。
function ingest(s, payload) {
  const nodes = Array.isArray(payload?.nodes) ? payload.nodes : [];
  const links = Array.isArray(payload?.links) ? payload.links : [];
  let addedNodes = 0;
  let addedLinks = 0;
  let dropped = 0;

  for (const raw of nodes) {
    if (!raw || raw.id === undefined || raw.id === null) continue;
    const id = String(raw.id);
    if (s.nodeById.has(id)) continue;
    const node = { ...raw, id };
    s.nodeById.set(id, node);
    s.graphData.nodes.push(node);
    addNodeLabel(s, node);
    addedNodes++;
  }

  for (const raw of links) {
    if (!raw) continue;
    const sk = endId(raw.source);
    const tk = endId(raw.target);
    const source = s.nodeById.get(sk);
    const target = s.nodeById.get(tk);
    // ② 端点找不到就整条丢：d3 遇到悬空端点会抛 node not found，同样冻死画布
    if (!source || !target) {
      dropped++;
      continue;
    }
    const key = `${sk}\u0000${tk}\u0000${raw.kind ?? ""}`;
    if (s.linkKeys.has(key)) continue;
    s.linkKeys.add(key);
    const link = { ...raw, source, target };
    s.graphData.links.push(link);
    addLinkLabel(s, link);
    addedLinks++;
  }

  if (addedNodes || addedLinks) {
    s.Graph.graphData(s.graphData); // ③ 一次 update 一次推进
    s.Graph.d3ReheatSimulation(); // 新点平滑扩散，旧节点继续受力（拖拽同一套力反馈）
    refit(s); // 取景跟着图走（autoFit）+ 像素基准重算
    scheduleFit(s); // 塌实后再补一次取景（批量数据到达时物理还没铺开）
  }
  if (dropped) console.warn(`forcegraph: ${dropped} 条边的端点不在图上（帧里的 id 笔误？）`);
}

// ---- 契约 -------------------------------------------------------------------
export async function mount(el, data, _host, options) {
  const opt = buildOptions(options);
  await ensureLib(opt.libUrl);

  const s = {
    id: 0,
    el,
    opt,
    Graph: null,
    graphData: { nodes: [], links: [] },
    nodeById: new Map(),
    linkKeys: new Set(),
    pxPerUnit: 1,
    lastFitAt: 0,
    fitTimer: null,
    labelsEl: null,
    nodeLabels: new Map(),
    linkLabels: new Map(),
    labelRaf: null,
    labelCapped: false,
  };

  // 宿主类名 + 一份样式表：覆盖库自己的 .float-tooltip-kap 外壳，另给标签层定位
  el.classList.add("fg-host");
  ensureStyles();

  const Graph = window.ForceGraph3D()(el)
    .backgroundColor(opt.background)
    .showNavInfo(false) // 相机是库默认的 trackball：左键旋转 / 右键平移 / 滚轮缩放
    .nodeRelSize(1) // 固定 1：半径 = ∛nodeVal（世界单位）
    .d3AlphaMin(0) // ① 物理不许自动停
    .nodeVal((n) => Math.pow(nodeRadiusWorld(s, n), 3))
    .nodeColor((n) => n.color || paletteColor(s, n.kind))
    .nodeLabel((n) => tooltip(n.name || `#${n.id}`, [["类型", n.kind], ["权重", n.weight]]))
    .linkColor((l) => l.color || s.opt.linkColor)
    .linkLabel((l) => tooltip(`${endId(l.source)} → ${endId(l.target)}`, [["关系", l.kind], ["强度", l.strength]]))
    .linkWidth((l) => linkWidthWorld(s, l))
    .linkOpacity(opt.linkOpacity)
    .linkDirectionalArrowLength(opt.arrowPx > 0 ? opt.arrowPx / s.pxPerUnit : 0)
    .linkDirectionalArrowRelPos(1)
    .linkDirectionalParticles(opt.particles > 0 ? (l) => Math.max(1, Math.round((l.strength || 1) / 2)) : 0)
    .linkDirectionalParticleWidth(opt.particles > 0 ? 1 / s.pxPerUnit : 0)
    .linkDirectionalParticleSpeed(0.004);
  s.Graph = Graph;

  // 尺寸/物理/相机：力参数与相机是一对（图摊多大 vs 屏幕上多大），见 DEFAULTS 注释。
  s.pxPerUnit = pxPerUnitAt(s, opt.cameraDistance);
  Graph.d3Force("charge").strength(opt.charge);
  Graph.d3Force("link").distance(opt.linkDistance);
  if (!opt.autoFit) {
    // 固定机位（k10r 的做法）：相机距离与方向由 options 给死。
    const dir = opt.cameraDirection;
    const len = Math.hypot(dir[0], dir[1], dir[2]) || 1;
    Graph.cameraPosition({
      x: (opt.cameraDistance * dir[0]) / len,
      y: (opt.cameraDistance * dir[1]) / len,
      z: (opt.cameraDistance * dir[2]) / len,
    });
  }
  // autoFit 时首个载荷进来后由 refit() 接管相机（拟合整张图 + 重算像素基准）。
  // 宿主容器此时已被尺寸样式撑开；resize 腿随后按内容盒校准。
  if (el.clientWidth > 0 && el.clientHeight > 0) Graph.width(el.clientWidth).height(el.clientHeight);

  const id = nextId++;
  s.id = id;
  graphs.set(id, s);
  ingest(s, payloadOf(data));
  return id;
}

export function update(id, data) {
  const s = graphs.get(id);
  if (!s) return;
  let payload;
  try {
    payload = payloadOf(data);
  } catch (e) {
    // update 腿解不开不是挂载失败：说一声、这一帧丢掉，图保持上一份
    console.warn("forcegraph: bad CBOR payload on update", e);
    return;
  }
  ingest(s, payload);
}

export function resize(id, w, h) {
  const s = graphs.get(id);
  if (!s || !(w > 0) || !(h > 0)) return;
  s.Graph.width(w).height(h);
}

export function unmount(id) {
  const s = graphs.get(id);
  if (!s) return;
  graphs.delete(id);
  clearTimeout(s.fitTimer);
  dropLabels(s);
  try {
    s.Graph.pauseAnimation?.();
    // _destructor 是 kapsule 暴露的拆卸口（清 scene / controls / renderer）；
    // 不调它 = 每个卸载的实例留一份 WebGL 上下文，浏览器十几个就耗尽。
    s.Graph._destructor?.();
  } catch (e) {
    console.warn("forcegraph: teardown", e);
  }
  s.el.replaceChildren();
}