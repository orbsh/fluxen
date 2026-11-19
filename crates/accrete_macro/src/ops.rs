use crate::utils::{get_ident_from_type, struct_has_field};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

pub fn impl_accrete_ops(ast: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &ast.ident;

    let id = if struct_has_field(ast, "id") {
        quote! { &self.id }
    } else {
        quote! { &None }
    };

    let mut sub_ref = quote! { None };
    let mut sub_mut = quote! { None };
    let mut set_children = quote! {};

    if struct_has_field(ast, "children") {
        sub_ref = quote! { self.children.as_ref() };
        sub_mut = quote! { self.children.as_mut() };
        set_children = quote! { self.children = Some(accrete); };
    };

    let mut attrs_ref = quote! { None };
    let mut attrs_mut = quote! { None };
    if struct_has_field(ast, "attrs") {
        attrs_ref = quote! { Some(&self.attrs) };
        attrs_mut = quote! { Some(&mut self.attrs) };
    };

    // Channels are per-struct: a field that is not there is a channel the node
    // does not accept, and the default trait method says so.
    let get_source = if struct_has_field(ast, "source") {
        quote! { self.source.as_ref() }
    } else {
        quote! { None }
    };
    let get_sink = if struct_has_field(ast, "sink") {
        quote! { self.sink.as_ref() }
    } else {
        quote! { None }
    };

    Ok(quote! {
        impl AccreteOps for #name {
            fn get_id(&self) -> &Option<String> {
                #id
            }
            fn get_type(&self) -> &str {
                stringify!(#name)
            }
            fn borrow_children(&self) -> Option<&Vec<Accrete>> {
                #sub_ref
            }
            fn borrow_children_mut(&mut self) -> Option<&mut Vec<Accrete>> {
                #sub_mut
            }
            fn set_children(&mut self, accrete: Vec<Accrete>) {
                #set_children
            }
            fn borrow_attrs(&self) -> Option<&dyn Classify> {
                #attrs_ref
            }
            fn borrow_attrs_mut(&mut self) -> Option<&mut dyn Classify> {
                #attrs_mut
            }
            fn get_source(&self) -> Option<&Bind> {
                #get_source
            }
            fn get_sink(&self) -> Option<&Bind> {
                #get_sink
            }
        }
    })
}

pub fn impl_accrete_ops_variant(ast: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &ast.ident;
    let mut r = Vec::new();
    if let syn::Data::Enum(d) = &ast.data {
        for i in &d.variants {
            r.push(&i.ident);
        }
    }
    Ok(quote! {
        impl AccreteOps for #name {
            fn get_id(&self) -> &Option<String> {
                match self {
                    #(#name::#r(c) => c.get_id()),*
                }
            }

            fn borrow_children(&self) -> Option<&Vec<Accrete>> {
                match self {
                    #(#name::#r(c) => c.borrow_children()),*
                }
            }

            fn borrow_children_mut(&mut self) -> Option<&mut Vec<Accrete>> {
                match self {
                    #(#name::#r(c) => c.borrow_children_mut()),*
                }
            }

            fn set_children(&mut self, accrete: Vec<Accrete>) {
                match self {
                    #(#name::#r(c) => { c.set_children(accrete) }),*
                }
            }

            fn get_source(&self) -> Option<&Bind> {
                match self {
                    #(#name::#r(c) => { c.get_source() }),*
                }
            }

            fn get_sink(&self) -> Option<&Bind> {
                match self {
                    #(#name::#r(c) => { c.get_sink() }),*
                }
            }

            fn get_type(&self) -> &str {
                match self {
                    #(#name::#r(c) => { stringify!(#name::#r) }),*
                }
            }

            fn borrow_attrs(&self) -> Option<&dyn Classify> {
                match self {
                    #(#name::#r(c) => { c.borrow_attrs() }),*
                }
            }

            fn borrow_attrs_mut(&mut self) -> Option<&mut dyn Classify> {
                match self {
                    #(#name::#r(c) => { c.borrow_attrs_mut() }),*
                }
            }
        }
    })
}

pub fn impl_accrete_wrap_variant(ast: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &ast.ident;
    let mut r = Vec::new();
    if let syn::Data::Enum(d) = &ast.data {
        for i in &d.variants {
            let v = &i.ident;
            let ty = match &i.fields {
                syn::Fields::Unnamed(f) => {
                    let x = f
                        .unnamed
                        .iter()
                        .map(|x| get_ident_from_type(&x.ty))
                        .filter(|x| x.is_some())
                        .flatten()
                        .collect::<Vec<_>>();
                    x.first().cloned()
                }
                _ => None,
            };
            if let Some(ty) = ty {
                r.push(quote! {
                    impl WrapOps for #ty {
                        type Target = #name;
                        fn wrap(self) -> Self::Target {
                            Self::Target::#v(self)
                        }
                    }
                })
            }
        }
    }
    Ok(quote! {
        #(#r)*
    })
}
