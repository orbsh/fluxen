//! Shared UI asset registry: the files under assets/ are registered in this crate,
//! so every UI crate can reference them across crates (manganis requires an asset to live in the crate that defines it).

use manganis::Asset;

pub const CORE_CSS: Asset = manganis::asset!("/assets/core.css");
pub const CUSTOM_CSS: Asset = manganis::asset!("/assets/custom.css");
