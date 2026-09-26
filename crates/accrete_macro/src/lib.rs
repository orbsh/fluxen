mod classify;
mod ops;
mod utils;

use classify::{impl_classify_accrete, impl_classify_attrs, impl_classify_variant};
use ops::{impl_accrete_ops, impl_accrete_ops_variant, impl_accrete_wrap_variant};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::{Data, DeriveInput, parse_macro_input};

fn into_ts(result: syn::Result<TokenStream2>) -> TokenStream {
    match result {
        Ok(output_stream2) => output_stream2.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

#[proc_macro_derive(Wrap)]
pub fn accrete_wrap(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    match ast.data {
        Data::Enum(_) => into_ts(impl_accrete_wrap_variant(&ast)),
        _ => syn::Error::new(ast.ident.span(), "Wrap only supports enums")
            .to_compile_error()
            .into(),
    }
}

#[proc_macro_derive(AccreteOps, attributes(ui_acrete))]
pub fn accrete_props(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    match ast.data {
        Data::Struct(_) => into_ts(impl_accrete_ops(&ast)),
        Data::Enum(_) => into_ts(impl_accrete_ops_variant(&ast)),
        _ => syn::Error::new(
            ast.ident.span(),
            "AccreteOps only supports structs and enums",
        )
        .to_compile_error()
        .into(),
    }
}

#[proc_macro_derive(ClassifyAttrs)]
pub fn classify_attrs(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    match impl_classify_attrs(&ast) {
        Ok(output_stream2) => output_stream2.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

#[proc_macro_derive(ClassifyAccrete)]
pub fn classify_accrete(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    match impl_classify_accrete(&ast) {
        Ok(output_stream2) => output_stream2.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

#[proc_macro_derive(ClassifyVariant)]
pub fn classify_variant(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    match impl_classify_variant(&ast) {
        Ok(output_stream2) => output_stream2.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn info(_args: TokenStream, input: TokenStream) -> TokenStream {
    // let item = parse_macro_input!(input as Item);
    // quote! {#item}.into()
    input
}

#[cfg(test)]
#[path = "test.rs"]
mod test_macro;
