/// Lightweight DOM helpers: wrap the common web_sys operations for components to use.
/// (The old `el(tag)` helper built bare nodes on the DOM and has no callers left — components all go through leptos views.)

pub fn set_class(el: &web_sys::Element, class: &str) {
    let _ = el.set_attribute("class", class);
}

pub fn set_id(el: &web_sys::Element, id: &str) {
    let _ = el.set_attribute("id", id);
}

pub fn mount(parent: &web_sys::Node, child: &web_sys::Node) {
    let _ = parent.append_child(child);
}

/// Run a piece of JS in global scope (chart/diagram/placeholder fade/rack scroll etc.).
pub fn eval(js: &str) {
    let _ = js_sys::eval(js);
}
