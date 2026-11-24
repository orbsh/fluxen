use tracing_wasm::WASMLayerConfigBuilder;

fn main() {
    // WASM panics go only to stderr by default (invisible); a console hook is required to see them in devtools
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default_with_config(
        WASMLayerConfigBuilder::new()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );
    // A mount failure (no window / no #main / WS won't open) is a state the page must surface,
    // not a process-level panic: the cause goes to the devtools log, the page stays blank, but it isn't swallowed.
    if let Err(e) = ui_leptos::mount() {
        tracing::error!("fluxen: mount failed: {e}");
    }
}
