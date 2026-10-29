use crate::Ctx;
use crate::hooks::{use_common_css, use_default};
use accrete::Diagram;
use leptos::html::Div;
use leptos::html::*;
use leptos::prelude::*;

/// mermaid 按需加载器（2026-10-14）：index.html 的 `<script>` 预加载退役——
/// 2.7MB 原始 / 772KB gz 跟着**每一次页面加载**走，哪怕整棵树里没有 diagram
/// 节点。改成首次挂载时注入 script：全局 promise 门控（多实例共享一次下载，
/// 浏览器按 URL 缓存），形状与 g2chart 模块的 ensureG2 一致。
/// `%SEL%` = CSS 选择器（JSON 串字面量），`%SRC%` = 脚本 URL 串字面量。
const MERMAID_LOADER: &str = r#"(function () {
  var sel = %SEL%;
  function run() {
    try { mermaid.init({}, sel); } catch (e) { console.warn("mermaid init failed", e); }
  }
  if (window.mermaid) { run(); return; }
  if (!window.__mermaidLoading) {
    window.__mermaidLoading = new Promise(function (res, rej) {
      var s = document.createElement("script");
      s.src = %SRC%;
      s.onload = res;
      s.onerror = function () { rej(new Error("mermaid load failed: " + s.src)); };
      document.head.appendChild(s);
    });
  }
  window.__mermaidLoading.then(run, function (e) { console.warn(String(e)); });
})();"#;

/// Mermaid 图表：数据取 `bind["value"].default`，挂载后按需加载 mermaid 再
/// `mermaid.init`。
pub fn diagram_(accrete: Diagram, _ctx: &Ctx, id: String) -> AnyView {
    let mut css = vec!["diagram"];
    use_common_css(&mut css, &accrete);
    let css = css.join(" ");

    if let Some(x) = use_default(&accrete)
        && let Some(y) = x.as_str()
    {
        let val = y.to_string();
        let id_ = id.clone();
        let nr = NodeRef::<Div>::new();
        nr.on_load(move |_el| {
            let sel = serde_json::to_string(&format!("#{id_}"))
                .unwrap_or_else(|_| "\"#\"".to_string());
            crate::dom::eval(
                &MERMAID_LOADER
                    .replace("%SEL%", &sel)
                    .replace("%SRC%", "\"/assets/mermaid.min.js\""),
            );
        });
        div()
            .id(id.as_str())
            .class(css.as_str())
            .child(val)
            .node_ref(nr)
            .into_any()
    } else {
        div().into_any()
    }
}
