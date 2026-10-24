/// 轻量 DOM 助手：封装 web_sys 的常用操作，供组件使用。
/// （旧的 `el(tag)` 助手在 DOM 上建裸节点，已无人调用——组件一律走 leptos 的 view。）

pub fn set_class(el: &web_sys::Element, class: &str) {
    let _ = el.set_attribute("class", class);
}

pub fn set_id(el: &web_sys::Element, id: &str) {
    let _ = el.set_attribute("id", id);
}

pub fn mount(parent: &web_sys::Node, child: &web_sys::Node) {
    let _ = parent.append_child(child);
}

/// 在全局作用域执行一段 JS（chart/diagram/placeholder fade/rack scroll 等）。
pub fn eval(js: &str) {
    let _ = js_sys::eval(js);
}
