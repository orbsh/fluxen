use tracing_wasm::WASMLayerConfigBuilder;

fn main() {
    // WASM panics go only to stderr by default (invisible); a console hook is required to see them in devtools
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default_with_config(
        WASMLayerConfigBuilder::new()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );
    // A mount failure (no window / no #main / a socket that will not open / a `data-transport` that does not
    // parse) is a state the page must surface, not a process-level panic. It goes to the devtools log *and*
    // into `#main`: a blank page plus a console line is how a configuration mistake gets mistaken for a
    // renderer bug.
    if let Err(e) = ui_leptos::mount() {
        tracing::error!("fluxen: mount failed: {e}");
        if let Some(doc) = web_sys::window().and_then(|w| w.document())
            && let Ok(Some(el)) = doc.query_selector("#main")
        {
            el.set_text_content(Some(&format!("fluxen: {e}")));
        }
    }
}
