use tracing_wasm::WASMLayerConfigBuilder;

fn main() {
    // WASM panic 默认只进 stderr（不可见），必须挂 console hook 才能在 devtools 里看到
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default_with_config(
        WASMLayerConfigBuilder::new()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );
    // 挂载失败（无 window / 无 #main / WS 打不开）是页面必须呈现的状态，
    // 不是进程级 panic：原因进 devtools 日志，页面空着但不吞掉它。
    if let Err(e) = ui_leptos::mount() {
        tracing::error!("fluxen: mount failed: {e}");
    }
}
