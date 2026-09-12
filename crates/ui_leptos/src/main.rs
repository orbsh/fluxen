use tracing_wasm::WASMLayerConfigBuilder;

fn main() {
    // WASM panic 默认只进 stderr（不可见），必须挂 console hook 才能在 devtools 里看到
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default_with_config(
        WASMLayerConfigBuilder::new()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );
    ui_leptos::mount();
}
