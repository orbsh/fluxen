//! 公共 UI 资源注册表：assets/ 下文件在此 crate 注册，
//! 供各 UI crate 跨 crate 引用（manganis 要求资源位于定义它的 crate 内）。

use manganis::Asset;

pub const MAIN_CSS: Asset = manganis::asset!("/assets/main.css");
pub const CUSTOM_CSS: Asset = manganis::asset!("/assets/custom.css");
