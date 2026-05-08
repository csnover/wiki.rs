//! Compile-time CSS compiler.

#![warn(
    clippy::pedantic,
    clippy::missing_docs_in_private_items,
    missing_docs,
    rust_2018_idioms
)]

use proc_macro::TokenStream;
use syn::{Error, LitStr, parse_macro_input};

/// Compiles a CSS file into a string literal.
#[proc_macro]
pub fn compile(input: TokenStream) -> TokenStream {
    let path = parse_macro_input!(input as LitStr);

    let resolved = path
        .span()
        .local_file()
        .and_then(|root| root.parent().map(|parent| parent.join(path.value())));

    let Some((root, file)) = resolved
        .as_deref()
        .and_then(|resolved| resolved.parent().zip(resolved.file_name()))
    else {
        return Error::new(
            path.span(),
            format!("Failed to compile CSS: {path:?} not found"),
        )
        .into_compile_error()
        .into();
    };

    barely_css_impl::compile(root, file)
        .map_or_else(
            |err| {
                Error::new(path.span(), format!("Failed to compile CSS: {err}"))
                    .into_compile_error()
            },
            |css| quote::quote!(#css),
        )
        .into()
}
