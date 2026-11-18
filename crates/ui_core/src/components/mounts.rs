//! 具名挂载点（ADR-0014 的契约收口）。
//!
//! 挂载点的两端原来靠一条 CSS 选择器字符串联系：`portal` 说 `to: "#overlay-root"`，
//! 谁挂出那个元素谁负责让选择器成立。选择器无校验、改名即断、目标不存在只有一行 warn。
//!
//! 现在挂载点有名字：`panel` 声明 `name`，`portal` 说 `to: 那个名字`。名字由框架在挂载/
//! 卸载时登记与撤销，解析走注册表；查不到就是查不到（fail-loud，不猜）。
//!
//! 旧写法仍然读：`to` 以 `#`/`.` 开头，或含空格/`>`/`[`，就当 CSS 选择器处理 —— 那是选择器的
//! 形状，不是名字的形状。名字只由字母、数字、`-`、`_`、`:` 组成。

use std::cell::RefCell;
use std::collections::HashMap;
use web_sys::Element;

thread_local! {
    /// 名字 → 元素。单线程 wasm，登记表不必共享；leptos 的 owner 顺序保证了注销先于重挂。
    static MOUNTS: RefCell<HashMap<String, Element>> = RefCell::new(HashMap::new());
    /// 登记表变过几次。portal 的解析 Effect tracked 读它 —— 挂载点晚出现时才能重解。
    static EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// `to` 是名字还是选择器？名字只由这几个字符组成；其余一律按选择器读（兼容旧帧）。
pub fn is_name(to: &str) -> bool {
    !to.is_empty()
        && !to.starts_with('#')
        && !to.starts_with('.')
        && to
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
}

/// 登记一个具名挂载点。同名的后到者覆盖先到者：一个名字一处挂载点，写两次是帧的错误。
pub fn register(name: &str, element: Element) {
    MOUNTS.with(|m| {
        if m.borrow_mut().insert(name.to_string(), element).is_some() {
            tracing::warn!("mount point {name:?} registered twice — the later one wins");
        }
    });
    EPOCH.with(|e| e.set(e.get() + 1));
}

/// 撤销登记。只在当前元素仍然是这个名字的主人时删除 —— 先挂新点再撤旧点的情况下，
/// 旧点的注销不该把新点一起抹掉。
pub fn unregister(name: &str, element: &Element) {
    let removed = MOUNTS.with(|m| match m.borrow().get(name) {
        Some(cur) if cur == element => {
            m.borrow_mut().remove(name);
            true
        }
        _ => false,
    });
    if removed {
        EPOCH.with(|e| e.set(e.get() + 1));
    }
}

/// 按名字取挂载点。查不到返回 `None`：调用方 warn 一次并等 epoch 变化，不猜。
pub fn element(name: &str) -> Option<Element> {
    MOUNTS.with(|m| m.borrow().get(name).cloned())
}

/// 登记表的版本号（tracked 读它 = 挂载点增减时重解）。
pub fn epoch() -> u64 {
    EPOCH.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests {
    use super::is_name;

    #[test]
    fn a_name_is_a_name_and_a_selector_is_a_selector() {
        assert!(is_name("rail"));
        assert!(is_name("overlay:root"));
        assert!(is_name("notice-2"));
        // 选择器的形状一律不当名字：旧帧靠这条继续工作。
        assert!(!is_name("#overlay-root"));
        assert!(!is_name(".mount"));
        assert!(!is_name("body > div"));
        assert!(!is_name(""));
    }
}