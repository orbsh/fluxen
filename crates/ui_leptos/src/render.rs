use accrete::Accrete;
use ui_leptos_macro::gen_dispatch;

gen_dispatch! {
    file = "../accrete/src/lib.rs",
    entry = "Accrete",
    object = "accrete"
}
