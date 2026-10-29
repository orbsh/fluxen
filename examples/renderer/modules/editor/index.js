// editor —— ADR 0016 的代码编辑器封装模块：ADR 0007 契约的 ES module 入口。
// 宿主 Editor 组件 import 本文件（url 默认 /assets/editor/index.js）。
//
// 数据区（`bind["value"]` 的 CBOR 字节）载荷 = {code, lang?, event?}：
//   code   源码文本（"代码作为参数"）
//   lang   Monaco 语言 id（缺省 plaintext；basic-languages 的 tokenizer 高亮）
//   event  保存时上行的对事件名（缺省 "submit"；生产者可配）
//
// 保存：编辑器里 Ctrl/Cmd+S → host.send(event, CBOR({code, lang}))——走宿主
// 现有的 uplink 腿（ctx.send，带本节点 id），与表单 submit 同一条通道。
// 按键**不用 Monaco 的 addCommand**（standalone 的 keybinding service 是全局单例，
// 多个编辑器同 chord 时最后注册的那个包场，上行会带错节点 id）：改为在本宿主容器上
// 捕获 keydown，preventDefault 掉浏览器自己的保存对话框；详见 mount() 里那段注释。
//
// 库按需加载（ADR 0015）：Monaco 是 AMD 构建，本模块在 mount() 里才注入
// <script src=".../vs/loader.js">（onload 门控 + require(['vs/editor/editor.main'])），
// 模块求值本身零下载。worker 与 CSS 由 Monaco 自己按 require.toUrl 解析到同源
// 的 vs/assets/<name>.worker-<hash>.js 与 vs/editor/editor.main.css——因此
// MonacoEnvironment.getWorkerUrl 不必设（该分支是 createWorker 缺省时的兜底），
// 也正因如此**装配时不能改 worker 文件的内容哈希名**（editor.main.js 与各
// *Mode-*.js 里写死了这些名字）。
//
// 部署：本目录整体拷入 examples/renderer/assets/editor/；Monaco AMD 子集（tarball
// 的 package/min/vs/**，原样、不改名）放进 examples/renderer/assets/monaco/vs/。
// 两者都在 .gitignore 里（构建产物模式，配方见 Dockerfile）：
//   monaco-editor 0.55.1  https://registry.npmjs.org/monaco-editor/-/monaco-editor-0.55.1.tgz
//   sha1 e74c6fe5a6bf985b817d2de3eb88d56afc494a1b（tarball，13 677 219 B）
// 容器镜像按这条配方装配（Dockerfile：MONACO_VERSION + sha1sum -c）。
//
// 剪枝（ADR 0016 决策 5）：只删 vs/assets/{css,html,json,ts}.worker-*.js 四个
// 语言服务 worker（≈ 9.1 MB，剩 ≈ 6.3 MB）。**根下的 monaco.contribution-<hash>.js
// 四块与 vs/language/*/monaco.contribution.js 必须留**——editor.main.js 的 define
// 头把它们写成了 eager 依赖，删掉 editor.main 就加载不了（实测，2026-10-15）；
// 它们只是 2–5 KB 的默认值块，重的是 worker 本体。
import { decode, encode } from "./cbor.js";

// Monaco 根：按【本模块的 URL】解析（相对 src 会按文档 base 解析，页面在 / 时
// 会指错地方——g2chart 那条实测教训同款）。
const MONACO = new URL("../monaco/", import.meta.url).href.replace(/\/+$/, "");
const VS = MONACO + "/vs";
const LOADER_URL = VS + "/loader.js";

let monacoReady = null;

/// 注入 AMD loader 再 require editor.main——两条都在 mount() 里发生，
/// 模块被 import 不代表 Monaco 被下载（ADR 0015）。
function ensureMonaco() {
  if (window.monaco) return Promise.resolve(window.monaco);
  if (!monacoReady) {
    monacoReady = new Promise((res, rej) => {
      const s = document.createElement("script");
      s.src = LOADER_URL;
      s.onload = () => {
        try {
          // paths.vs 不带尾斜杠：AMD 的模块 id 形如 vs/editor/editor.main，
          // loader 按 <base>/editor/editor.main.js 拼接（Monaco 官方写法）。
          window.require.config({ paths: { vs: VS } });
          window.require(["vs/editor/editor.main"], () => res(window.monaco), rej);
        } catch (e) {
          rej(e);
        }
      };
      s.onerror = () => rej(new Error("monaco loader load failed: " + LOADER_URL));
      document.head.appendChild(s);
    });
  }
  return monacoReady;
}

const views = new Map(); // id -> { editor, box, host, lang, code, event }
let nextId = 1;

function specOf(bytes) {
  if (!bytes || !bytes.length) return {};
  const v = decode(bytes);
  return v && typeof v === "object" ? v : {};
}

const str = (v, dflt) => (typeof v === "string" && v.length ? v : dflt);

export async function mount(el, data /* ArrayBuffer(CBOR) */, host) {
  const spec = specOf(data);
  const monaco = await ensureMonaco();

  el.replaceChildren();
  const box = document.createElement("div");
  box.style.width = "100%";
  box.style.height = "100%";
  el.appendChild(box);

  const view = {
    lang: str(spec.lang, "plaintext"),
    code: typeof spec.code === "string" ? spec.code : "",
    event: str(spec.event, "submit"),
    host,
    box,
    el,
  };
  view.editor = monaco.editor.create(box, {
    value: view.code,
    language: view.lang,
    // 尺寸归宿主的 ResizeObserver（mount_module 的 resize 腿）——autoLayout 自己
    // 观察容器会与它重复布局。
    automaticLayout: false,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    renderWhitespace: "none",
  });

  const id = nextId++;
  views.set(id, view);

  // 保存 = Ctrl/Cmd+S，上行 {code, lang}（事件名由载荷给，缺省 submit）。
  //
  // 不走 `editor.addCommand`：Monaco standalone 的 keybinding service 是**全局
  // 单例**，同一 chord 由最后注册的编辑器赢——实测三个编辑器各自 addCommand
  // 后，焦点在任何一个上、Ctrl+S 都只触发最后挂载那份，上行带错节点 id（生产者
  // 会把代码写回错文件）。改为在**本宿主容器**上捕获 keydown：编辑器的输入面
  // （`.native-edit-context`）在容器内，事件必经此处，因此天然按编辑器分流，
  // 也不会去抢页面别处的 Ctrl+S。preventDefault 吃掉浏览器自己的保存对话框。
  view.onKey = (e) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
    if (e.key !== "s" && e.key !== "S" && e.code !== "KeyS") return;
    e.preventDefault();
    e.stopPropagation();
    save(id);
  };
  el.addEventListener("keydown", view.onKey, true);
  return id;
}

/// 上行一份 view 的当前文本（Ctrl/Cmd+S 的落点）。
function save(id) {
  const v = views.get(id);
  if (!v) return;
  const text = v.editor.getValue();
  v.code = text; // 记住"已上行的那份"，同载荷回显不会把用户手改抹掉
  if (v.host && typeof v.host.send === "function") {
    v.host.send(v.event, encode({ code: text, lang: v.lang }));
  }
}

export function update(id, data) {
  const v = views.get(id);
  if (!v) return;
  const spec = specOf(data);
  const lang = str(spec.lang, v.lang);
  if (lang !== v.lang) {
    v.lang = lang;
    const m = v.editor.getModel();
    if (m) window.monaco.editor.setModelLanguage(m, lang);
  }
  v.event = str(spec.event, v.event);
  // 生产者权威 + 回显护栏：载荷里的 code 与"上次上行/收到的那份"不同才覆盖
  // 编辑器内容；同一份载荷重复投递不抹掉用户正在编的文本。
  if (typeof spec.code === "string" && spec.code !== v.code) {
    v.code = spec.code;
    if (v.editor.getValue() !== spec.code) v.editor.setValue(spec.code);
  }
}

export function resize(id, w, h) {
  const v = views.get(id);
  if (v && w > 0 && h > 0) v.editor.layout({ width: w, height: h });
}

export function unmount(id) {
  const v = views.get(id);
  if (!v) return;
  views.delete(id);
  try { v.el.removeEventListener("keydown", v.onKey, true); } catch (e) { /* 已拆 */ }
  try { v.editor.dispose(); } catch (e) { /* 已释放 */ }
  v.box.remove();
}