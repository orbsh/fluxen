// g2chart —— Chart 封装模块：ADR 0007 契约的 ES module 入口，ADR 0009 的载荷，ADR 0051 的图例。
// 宿主 Canvas 组件 import 本文件（url 指向 /assets/g2chart/index.js）。
// G2 库是 UMD（不可 import，只能 globalThis.G2），由本模块以 <script>
// 标签自举加载——onload 门控，浏览器按 URL 缓存，多实例/重复挂载共享
// 一次下载。spec 载荷 = 数据区的 CBOR 字节（宿主统一编码），
// 解码用 ./cbor.js（最小 RFC 8949 实现，覆盖 ciborium 产出子集）。
// 部署：同目录放 g2.min.js（tarball dist）——@antv/g2 5.4.8，
// https://unpkg.com/@antv/g2@5.4.8/dist/g2.min.js
// sha256 7e7d346cab68c002a889dc6145bfc1f9ae1391be82ce8d514fb106ef3a6e6412
// （2026-10-10 由本机那份文件反查补记：仓库原先没记版本）；本目录整体
// 拷入 crates/ui_leptos/assets/g2chart/（.gitignore，配方在此注释）。
// 容器镜像按这条配方自动装配（Dockerfile：ENV G2_VERSION + sha256sum -c）。
//
// ADR 0051：图例自己画。G2 的内置图例是 canvas 上的场景节点（实测：容器里只有一个
// <canvas>，document 里零个 legend 元素），挂不上按钮，皮肤也描不了边。于是：
//   - 序列从 spec 归一层来：`view` 的 children 原样用；单个 mark 带 `color` 编码的，
//     按该编码的取值展开成每序列一个 child；
//   - 图例项 = 色块 + 名字。名字点击隐藏该序列，色块点击出该序列的选项（形态 + 颜色）；
//   - 形态是 child 的 `type`，颜色是 child 的 `style.fill` / `style.stroke` 定值
//     （实测：fill 画出 #e11d48、stroke 画出 #2563eb，同一坐标系同一次 render）；
//   - 坐标是整图的一根轴，不是序列的：右上角一个显示当前坐标的图标按钮，点一下切下一个。
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

const DEFAULT_MARKS = ["interval", "line", "area", "point"];
const DEFAULT_COORDS = ["cartesian", "theta", "polar"];

// A coordinate is the whole picture's axis, so its button shows the state instead of naming a menu.
const COORD_ICON = {
  cartesian:
    '<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">' +
    '<path d="M2.5 1.5v10h10" fill="none" stroke="currentColor" stroke-width="1.2"/>' +
    '<rect x="5" y="6.5" width="2.6" height="5" fill="currentColor"/>' +
    '<rect x="8.6" y="9" width="2.6" height="2.5" fill="currentColor" opacity=".55"/>' +
    "</svg>",
  theta:
    '<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">' +
    '<circle cx="7" cy="7" r="6" fill="none" stroke="currentColor" stroke-width="1.2" opacity=".4"/>' +
    '<path d="M7 7 L7 1 A6 6 0 0 1 12.2 10 Z" fill="currentColor"/>' +
    "</svg>",
  polar:
    '<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">' +
    '<circle cx="7" cy="7" r="6" fill="none" stroke="currentColor" stroke-width="1.2" opacity=".4"/>' +
    '<circle cx="7" cy="7" r="3" fill="none" stroke="currentColor" stroke-width="1.2" opacity=".6"/>' +
    '<path d="M7 7 L12.2 4" stroke="currentColor" stroke-width="1.2"/>' +
    "</svg>",
};

// Two bars next to each other versus one bar in two pieces: the button shows which it is, like the coordinate one.
const STACK_ICON = {
  grouped:
    '<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">' +
    '<rect x="2" y="5" width="4" height="7.5" fill="currentColor"/>' +
    '<rect x="8" y="2" width="4" height="10.5" fill="currentColor" opacity=".55"/>' +
    "</svg>",
  stacked:
    '<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">' +
    '<rect x="3" y="8.5" width="8" height="4" fill="currentColor"/>' +
    '<rect x="3" y="2" width="8" height="6" fill="currentColor" opacity=".55"/>' +
    "</svg>",
};

function specOf(bytes) {
  if (!bytes || !bytes.length) return {};
  const v = decode(bytes);
  return v && typeof v === "object" ? v : {};
}

// ---------------------------------------------------------------- colour maths
// The panel speaks oklch; the scale hands out hex. Both directions, no library.

function srgbToLinear(v) {
  return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
}

/// A colour as the browser or the scale hands it over: `#rrggbb` or `rgb(r, g, b)`. Anything else is unknown.
function channelsOf(color) {
  const s = String(color || "").trim();
  const hex = /^#?([0-9a-f]{6})$/i.exec(s);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  const rgb = /^rgba?\(\s*([0-9.]+)\s*,\s*([0-9.]+)\s*,\s*([0-9.]+)/i.exec(s);
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])];
  return null;
}

function hexToOklch(color) {
  const channels = channelsOf(color);
  if (!channels) return null;
  const [r8, g8, b8] = channels;
  const r = srgbToLinear(r8 / 255);
  const g = srgbToLinear(g8 / 255);
  const b = srgbToLinear(b8 / 255);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const mm = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * mm - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * mm + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * mm - 0.808675766 * s;
  const c = Math.hypot(A, B);
  const h = c < 1e-6 ? 0 : (Math.atan2(B, A) * 180) / Math.PI;
  return [L, c, (h + 360) % 360];
}

function oklchCss(l, c, h) {
  return `oklch(${l.toFixed(2)} ${c.toFixed(2)} ${Math.round(h)})`;
}

// ---------------------------------------------------------------- the spec

// Keys that describe the VIEW move up when a single-mark base is normalised; keys that describe a MARK stay
// on each child. Nothing else is interpreted — everything else rides along untouched.
const VIEW_KEYS = ["coordinate", "axis", "scale", "interaction", "animate", "padding", "theme", "legend"];

/// What the legend calls a series: its `name`, else the one value its own colour channel carries, else where it
/// sits in the list (a chart with no colour channel has no better name).
function seriesNameOf(child, index) {
  if (!child) return `#${index + 1}`;
  if (typeof child.name === "string" && child.name) return child.name;
  const field = child.encode && typeof child.encode.color === "string" ? child.encode.color : null;
  if (field && Array.isArray(child.data)) {
    const values = new Set(child.data.map((row) => String(row[field])));
    if (values.size === 1) return [...values][0];
  }
  return `#${index + 1}`;
}

/// Two shapes are per-series switchable; a third is not, and pretending otherwise would take a working chart
/// apart. What decides:
///
///  - **series** — every child carries one value of its colour channel (or there is a single mark to split by
///    that channel). One legend item per series, and the item owns that series' form and colour.
///  - **layers** — the children are LAYERS over one shared series set (the radar: an `area` fill plus a `line`
///    stroke, both coloured by `name`). Changing one child's `type` would destroy the composite, so the legend
///    lists the series and can hide them (by filtering the view's data), and nothing else.
///
/// A child whose colour channel spans more than one value is a layer; that is the whole test.
function colorFieldOf(node) {
  const encode = (node && node.encode) || {};
  return typeof encode.color === "string" ? encode.color : null;
}

function valuesIn(data, field) {
  const seen = new Set();
  for (const row of data || []) seen.add(String(row[field]));
  return seen;
}

function normalize(spec) {
  const view = {};
  for (const k of VIEW_KEYS) if (k in spec) view[k] = spec[k];

  if (Array.isArray(spec.children) && spec.children.length) {
    let span = 0;
    for (const child of spec.children) {
      const field = colorFieldOf(child);
      if (!field) continue;
      span = Math.max(span, valuesIn(child.data || spec.data, field).size);
    }
    if (span > 1) {
      const field = colorFieldOf(spec.children[0]) || colorFieldOf(spec.children.find((c) => colorFieldOf(c)));
      const names = [...valuesIn(field ? spec.data : [], field)].map(String);
      return {
        mode: "layers",
        view,
        colorField: field,
        data: spec.data || [],
        children: spec.children.map((child) => Object.assign({}, child)),
        items: names.map((name) => ({ name, color: null, hidden: false })),
      };
    }
    return {
      mode: "series",
      view,
      items: spec.children.map((child, i) => ({
        name: seriesNameOf(child, i),
        mark: (child && child.type) || "interval",
        child: Object.assign({}, child),
        color: null,
        hidden: false,
      })),
    };
  }

  const field = colorFieldOf(spec);
  if (field && Array.isArray(spec.data)) {
    const groups = new Map();
    for (const row of spec.data) {
      const key = String(row[field]);
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(row);
    }
    return {
      mode: "series",
      view,
      items: [...groups.entries()].map(([name, rows]) => ({
        name,
        mark: spec.type || "interval",
        child: Object.assign({}, spec, { data: rows }),
        color: null,
        hidden: false,
      })),
    };
  }

  // One series: still one legend item, because that item is the only place to change its form.
  return {
    mode: "series",
    view,
    items: [
      {
        name: spec.name || field || "—",
        mark: spec.type || "interval",
        child: Object.assign({}, spec),
        color: null,
        hidden: false,
      },
    ],
  };
}

/// Streaming replaces the spec, not the reader's choices: marks, colours and what is hidden stay.
function mergeItems(old, fresh) {
  const byName = new Map(old.map((item) => [item.name, item]));
  return fresh.map((item) => {
    const had = byName.get(item.name);
    if (!had) return item;
    const merged = Object.assign({}, item, { color: had.color, hidden: had.hidden, smooth: had.smooth });
    if (item.mark != null) merged.mark = had.mark;
    return merged;
  });
}

function buildSpec(state) {
  const root = Object.assign({}, state.view);
  // The legend above the plot is ours (ADR 0051 §1). G2 draws its own whenever a colour channel exists, so it is
  // switched off on every render, whatever the frame asked for — two legends, one of them unclickable, is worse
  // than the frame's `legend` option being ignored.
  root.legend = false;
  // The coordinate is stated, never deleted: `options()` patches the spec tree, so removing the node leaves the
  // last one in place and switching BACK to a plane chart repaints the polar one (measured: theta -> cartesian
  // stayed round until `coordinate: {type: "cartesian"}` was written out).
  const coord = state.coords[state.coord];
  root.coordinate = { type: coord || "cartesian" };
  root.type = "view";

  if (state.mode === "layers") {
    const hidden = new Set(state.items.filter((i) => i.hidden).map((i) => i.name));
    const field = state.colorField;
    root.data = field ? state.data.filter((row) => !hidden.has(String(row[field]))) : state.data;
    root.children = state.children.map((child) => Object.assign({}, child));
    return root;
  }

  const visible = state.items.filter((item) => !item.hidden);
  const folded = foldBars(state, visible);
  if (folded) {
    root.children = [folded];
    return root;
  }

  const children = [];
  for (const item of visible) {
    const child = Object.assign({}, item.child);
    for (const k of VIEW_KEYS) delete child[k];
    child.type = item.mark;
    if (item.color) {
      child.style = Object.assign({}, child.style, { fill: item.color, stroke: item.color });
    }
    // `smooth` is G2's own line shape; a straight line is the absence of it, so sharp needs no value.
    if (item.smooth && (item.mark === "line" || item.mark === "area")) {
      child.style = Object.assign({}, child.style, { shape: "smooth" });
    }
    children.push(child);
  }
  root.children = children;
  return root;
}

/// Bars of two or more series only read as columns when they are ONE layer. `dodgeX` (side by side) and
/// `stackY` (one on top of another) are transforms of a layer, and the grouping channel lives in that layer:
/// separate children have nothing to be dodged against, so they paint over each other and a two-series bar chart
/// looks like a one-series one. Folding the all-interval series back into the single interval child they were
/// split from gives the transforms something to work with; each series' colour rides in the scale, and hiding a
/// series means leaving its rows out. A mixed chart (bars plus a line) does not fold — the forms have to stay
/// apart — and keeps the old overlaying children.
function foldBars(state, visible) {
  if (visible.length < 2) return null;
  if (!visible.every((item) => item.mark === "interval")) return null;
  const first = visible[0].child || {};
  const field = colorFieldOf(first) || "series";
  const data = [];
  for (const item of visible) {
    const rows = item.child && Array.isArray(item.child.data) ? item.child.data : [];
    for (const row of rows) data.push(Object.assign({}, row, { [field]: item.name }));
  }
  const child = Object.assign({}, first, {
    type: "interval",
    data,
    encode: Object.assign({}, first.encode, { color: field }),
    transform: [{ type: state.stack === "stacked" ? "stackY" : "dodgeX" }],
  });
  const colors = visible.map((item) => item.color || (item.child && item.child.style && item.child.style.fill));
  if (colors.every(Boolean)) {
    child.scale = Object.assign({}, first.scale, {
      color: { domain: visible.map((item) => item.name), range: colors },
    });
  }
  return child;
}

// ---------------------------------------------------------------- the legend

function el(tag, style, text) {
  const node = document.createElement(tag);
  if (style) node.setAttribute("style", style);
  if (text != null) node.textContent = text;
  return node;
}

/// The colour a series is drawn in right now: our override, else what the colour scale says, else unknown.
/// `null` is honest rather than a guess — the swatch then says "no explicit colour" and the panel starts
/// from a neutral triple.
function seriesColor(view, item, index) {
  if (item.color) return item.color;
  try {
    const scale = view.chart.getScale().color;
    if (scale && typeof scale.map === "function") {
      const mapped = scale.map(item.name);
      if (typeof mapped === "string") return mapped;
    }
  } catch (e) {
    /* no colour channel on this chart */
  }
  return null;
}

export async function mount(host, data, _host2, options) {
  await ensureG2();
  const opts = options && typeof options === "object" ? options : {};
  const declared = Array.isArray(opts.coordinate) && opts.coordinate.length ? opts.coordinate : DEFAULT_COORDS.slice();
  const marks = Array.isArray(opts.marks) && opts.marks.length ? opts.marks : DEFAULT_MARKS.slice();

  const spec = specOf(data);
  const norm = normalize(spec);
  const start = (spec.coordinate && spec.coordinate.type) || "cartesian";
  const coords = declared.includes(start) ? declared.slice() : [start, ...declared];

  const root = el("div", "display:flex;flex-direction:column;width:100%;height:100%;min-height:0;");
  const legend = el("div", "display:flex;flex-wrap:wrap;gap:10px;padding:0 4px 4px;align-items:center;flex:0 0 auto;");
  // The plot is the part that takes what is left; the chart box is ABSOLUTELY inside it, because a
  // percentage-height child of an auto-height parent feeds the canvas's own height back into the parent and
  // the picture grows on every repaint (measured: 640 -> 816 -> 1776 px on one row).
  const plot = el("div", "position:relative;flex:1 1 auto;min-height:150px;min-width:0;");
  const box = el("div", "position:absolute;inset:0;");
  plot.appendChild(box);
  root.appendChild(legend);
  root.appendChild(plot);
  host.replaceChildren(root);

  const chart = new window.G2.Chart({ container: box, autoFit: true });
  const view = {
    chart,
    root,
    legend,
    plot,
    // The whole normalized shape rides in the state: which mode it is (series / layers), the layers themselves,
    // and the data they share. Dropping any of them turns a layered radar into a series chart with no marks.
    // `stack` is chart-level, like the coordinate: it says how the bars of ALL series compose, so it cannot live
    // in a per-series panel.
    state: Object.assign({}, norm, { coords, marks, coord: coords.indexOf(start), stack: "grouped" }),
    menu: null,
    scrim: null,
    pending: null,
  };
  const id = nextId++;
  views.set(id, view);

  await paint(view);
  return id;
}

async function paint(view) {
  view.chart.options(buildSpec(view.state));
  await view.chart.render();
  drawLegend(view);
}

/// Dragging a slider is a stream of values; the chart is repainted on a trailing edge rather than per event.
function schedulePaint(view) {
  if (view.pending) return;
  view.pending = setTimeout(() => {
    view.pending = null;
    paint(view);
  }, 60);
}

function drawLegend(view) {
  const { legend, state } = view;
  legend.replaceChildren();
  state.items.forEach((item, index) => {
    const row = el("span", "display:inline-flex;align-items:center;gap:4px;");
    row.setAttribute("data-series", item.name);

    const shown = seriesColor(view, item, index);
    // A layered spec (the radar) has no per-series form to offer: its children are one composite, so the swatch
    // is a readout rather than a button (ADR 0051).
    const switchable = state.mode === "series";
    const swatch = el(
      switchable ? "button" : "span",
      "width:12px;height:12px;padding:0;border:none;border-radius:3px;flex:0 0 auto;" +
        (switchable ? "cursor:pointer;" : "") +
        "box-shadow:inset 0 0 0 1px rgba(0,0,0,.18);" +
        (shown
          ? "background:" + shown + ";"
          : "background:repeating-linear-gradient(45deg,transparent 0 2px,rgba(0,0,0,.18) 2px 4px);")
    );
    swatch.className = "g2c-swatch";
    if (switchable) {
      swatch.type = "button";
      swatch.title = "形态与颜色";
      swatch.setAttribute("aria-label", `${item.name}：形态与颜色`);
      swatch.addEventListener("click", (event) => {
        event.stopPropagation();
        openMenu(view, item, swatch);
      });
    } else {
      swatch.title = `${item.name}：这一张由帧里的多层 mark 合成，没有单序列的形态可换`;
    }

    const name = el(
      "span",
      "font-size:12px;cursor:pointer;user-select:none;" +
        (item.hidden ? "opacity:.45;text-decoration:line-through;" : ""),
      item.name
    );
    name.setAttribute("role", "button");
    name.title = item.hidden ? "显示" : "隐藏";
    name.addEventListener("click", () => {
      item.hidden = !item.hidden;
      paint(view);
    });

    row.appendChild(swatch);
    row.appendChild(name);
    legend.appendChild(row);
  });
  drawCoordButton(view);
  drawStackButton(view);
}

function drawCoordButton(view) {
  const { plot, state } = view;
  const old = plot.querySelector(".g2c-coord");
  if (old) old.remove();
  if (state.coords.length < 2) return;
  const current = state.coords[state.coord];
  const button = el(
    "button",
    "position:absolute;top:2px;right:2px;z-index:5;width:20px;height:20px;padding:0;display:flex;" +
      "align-items:center;justify-content:center;border:none;border-radius:4px;background:transparent;" +
      "color:inherit;opacity:.5;cursor:pointer;"
  );
  button.type = "button";
  button.className = "g2c-coord";
  button.innerHTML = COORD_ICON[current] || COORD_ICON.cartesian;
  button.title = `坐标：${current} · 点击切换`;
  button.setAttribute("aria-label", `坐标 ${current}，点击切换`);
  button.addEventListener("mouseenter", () => {
    button.style.opacity = "1";
  });
  button.addEventListener("mouseleave", () => {
    button.style.opacity = ".5";
  });
  button.addEventListener("click", (event) => {
    event.stopPropagation();
    state.coord = (state.coord + 1) % state.coords.length;
    paint(view);
  });
  plot.appendChild(button);
}

/// Only when there is something to compose: two or more series, all of them bars. One bar per x has no grouping
/// question to ask, and a bars-plus-line chart has no stacking answer.
function drawStackButton(view) {
  const { plot, state } = view;
  const old = plot.querySelector(".g2c-stack");
  if (old) old.remove();
  const visible = state.items.filter((item) => !item.hidden);
  if (visible.length < 2 || !visible.every((item) => item.mark === "interval")) return;
  const stacked = state.stack === "stacked";
  const button = el(
    "button",
    "position:absolute;top:2px;right:26px;z-index:5;width:20px;height:20px;padding:0;display:flex;" +
      "align-items:center;justify-content:center;border:none;border-radius:4px;background:transparent;" +
      "color:inherit;opacity:.5;cursor:pointer;"
  );
  button.type = "button";
  button.className = "g2c-stack";
  button.innerHTML = stacked ? STACK_ICON.stacked : STACK_ICON.grouped;
  button.title = stacked ? "柱：堆叠 · 点击改成分组" : "柱：分组 · 点击改成堆叠";
  button.setAttribute("aria-label", button.title);
  button.addEventListener("mouseenter", () => {
    button.style.opacity = "1";
  });
  button.addEventListener("mouseleave", () => {
    button.style.opacity = ".5";
  });
  button.addEventListener("click", (event) => {
    event.stopPropagation();
    state.stack = stacked ? "grouped" : "stacked";
    paint(view);
  });
  plot.appendChild(button);
}

// ---------------------------------------------------------------- the per-series panel

function closeMenu(view) {
  if (view.menu) view.menu.remove();
  if (view.scrim) view.scrim.remove();
  if (view.onKey) document.removeEventListener("keydown", view.onKey, true);
  view.menu = null;
  view.scrim = null;
  view.onKey = null;
}

function anchorOf(view, name) {
  return view.legend.querySelector(`[data-series="${CSS.escape(name)}"] button`);
}

function openMenu(view, item, anchor) {
  closeMenu(view);
  const state = view.state;
  const rect = anchor.getBoundingClientRect();

  const panel = el(
    "div",
    "position:fixed;z-index:60;display:flex;flex-direction:column;gap:6px;padding:8px;min-width:210px;" +
      "background:var(--surface,#fff);color:inherit;border-radius:8px;font-size:12px;" +
      "box-shadow:0 6px 20px rgba(0,0,0,.18),inset 0 0 0 1px rgba(0,0,0,.10);"
  );
  panel.className = "g2c-menu";
  panel.style.left = `${Math.round(rect.left)}px`;
  panel.style.top = `${Math.round(rect.bottom + 6)}px`;

  // 形态：帧允许的那几个 mark 词，一个也不多
  const marksRow = el("div", "display:flex;gap:4px;flex-wrap:wrap;align-items:center;");
  marksRow.appendChild(el("span", "opacity:.6;width:34px;", "形态"));
  for (const mark of state.marks) {
    const button = el(
      "button",
      "padding:1px 6px;font-size:11px;border:none;border-radius:4px;cursor:pointer;" +
        (mark === item.mark ? "background:rgba(0,0,0,.12);font-weight:600;" : "background:transparent;opacity:.7;"),
      mark
    );
    button.type = "button";
    button.addEventListener("click", async (event) => {
      event.stopPropagation();
      item.mark = mark;
      await paint(view);
      openMenu(view, item, anchorOf(view, item.name) || anchor);
    });
    marksRow.appendChild(button);
  }
  panel.appendChild(marksRow);

  // 拐角：一条折线是走直段还是走曲线。它属于这一条自己的形状，与形态、颜色同一层级——两个序列可以一个折角一个
  // 平滑。柱子没有拐角可谈，所以这个只在 line / area 时出现。
  if (item.mark === "line" || item.mark === "area") {
    const cornerRow = el("div", "display:flex;gap:4px;flex-wrap:wrap;align-items:center;");
    cornerRow.appendChild(el("span", "opacity:.6;width:34px;", "拐角"));
    for (const [value, label] of [
      [false, "折角"],
      [true, "平滑"],
    ]) {
      const on = Boolean(item.smooth) === value;
      const button = el(
        "button",
        "padding:1px 6px;font-size:11px;border:none;border-radius:4px;cursor:pointer;" +
          (on ? "background:rgba(0,0,0,.12);font-weight:600;" : "background:transparent;opacity:.7;"),
        label
      );
      button.type = "button";
      button.className = value ? "g2c-smooth" : "g2c-sharp";
      button.addEventListener("click", async (event) => {
        event.stopPropagation();
        item.smooth = value;
        await paint(view);
        openMenu(view, item, anchorOf(view, item.name) || anchor);
      });
      cornerRow.appendChild(button);
    }
    panel.appendChild(cornerRow);
  }

  // 颜色：三根滑块，与渲染器的 color 字段同一套形状（L/C/H 各一滑块加一个数字框）
  const shown = seriesColor(view, item, state.items.indexOf(item));
  const seed = hexToOklch(shown) || [0.7, 0.12, 200];
  const terms = { l: seed[0], c: seed[1], h: seed[2] };

  const swatch = el("span", "width:14px;height:14px;border-radius:3px;box-shadow:inset 0 0 0 1px rgba(0,0,0,.18);");
  swatch.style.background = oklchCss(terms.l, terms.c, terms.h);
  const colorRow = el("div", "display:flex;align-items:center;gap:6px;");
  colorRow.appendChild(el("span", "opacity:.6;width:34px;", "颜色"));
  colorRow.appendChild(swatch);
  panel.appendChild(colorRow);

  // The bars this panel shows are the field's bars: `custom.css` paints `.color-row[data-term]` from these three
  // numbers, so the module carries the same class and writes the same variables rather than its own copy of the
  // gradients (ADR 0051 §24, ADR 0052 §3).
  const paintBars = () => {
    panel.style.setProperty("--l", terms.l.toFixed(4));
    panel.style.setProperty("--c", terms.c.toFixed(4));
    panel.style.setProperty("--h", String(Math.round(terms.h)));
  };
  paintBars();

  const apply = () => {
    item.color = oklchCss(terms.l, terms.c, terms.h);
    swatch.style.background = item.color;
    paintBars();
    schedulePaint(view);
  };

  const rows = [
    ["L", 0, 1, 0.01, (v) => (terms.l = v)],
    ["C", 0, 0.4, 0.01, (v) => (terms.c = v)],
    ["H", 0, 360, 1, (v) => (terms.h = v)],
  ];
  for (const [name, min, max, step, set] of rows) {
    const key = name.toLowerCase();
    const row = el("div", "display:flex;align-items:center;gap:6px;");
    // The class and the term are what `custom.css` keys the bar on; the renderer's own colour field draws the
    // same two attributes (widgets/color.rs).
    row.className = "color-row";
    row.dataset.term = name;
    row.appendChild(el("span", "opacity:.6;width:34px;", name));

    const slider = el("input", "flex:1 1 auto;min-width:90px;");
    slider.type = "range";
    slider.min = String(min);
    slider.max = String(max);
    slider.step = String(step);
    slider.value = String(name === "H" ? Math.round(terms.h) : terms[key]);

    const field = el("input", "width:56px;font-size:11px;");
    field.type = "number";
    field.min = String(min);
    field.max = String(max);
    field.step = String(step);
    field.value = slider.value;

    slider.addEventListener("input", () => {
      set(parseFloat(slider.value));
      field.value = name === "H" ? String(Math.round(terms.h)) : terms[key].toFixed(2);
      apply();
    });
    field.addEventListener("input", () => {
      const value = parseFloat(field.value);
      if (!isFinite(value)) return;
      set(value);
      slider.value = String(value);
      apply();
    });

    row.appendChild(slider);
    row.appendChild(field);
    panel.appendChild(row);
  }

  const scrim = el("div", "position:fixed;inset:0;z-index:55;");
  scrim.className = "g2c-scrim";
  scrim.addEventListener("click", () => closeMenu(view));
  const onKey = (event) => {
    if (event.key === "Escape") closeMenu(view);
  };
  document.addEventListener("keydown", onKey, true);

  document.body.appendChild(scrim);
  document.body.appendChild(panel);
  view.menu = panel;
  view.scrim = scrim;
  view.onKey = onKey;
}

// ---------------------------------------------------------------- the module contract

export function update(id, data) {
  const view = views.get(id);
  if (!view) return Promise.resolve();
  const spec = specOf(data);
  const norm = normalize(spec);
  const start = (spec.coordinate && spec.coordinate.type) || "cartesian";
  view.state.view = norm.view;
  view.state.mode = norm.mode;
  view.state.children = norm.children || [];
  view.state.data = norm.data || [];
  view.state.colorField = norm.colorField || null;
  view.state.items = mergeItems(view.state.items, norm.items);
  if (view.state.coords.includes(start)) view.state.coord = view.state.coords.indexOf(start);
  // G2 v5 has no chart.update() (measured: options() alone paints nothing); options() diffs the spec tree and
  // render() is what puts it on the canvas.
  view.chart.options(buildSpec(view.state));
  return view.chart.render().then(() => drawLegend(view));
}

export function resize(id, w, h) {
  const view = views.get(id);
  if (view && w > 0 && h > 0) view.chart.changeSize(w, h);
}

export function unmount(id) {
  const view = views.get(id);
  if (!view) return;
  if (view.pending) clearTimeout(view.pending);
  closeMenu(view);
  view.chart.destroy();
  view.root.remove();
  views.delete(id);
}