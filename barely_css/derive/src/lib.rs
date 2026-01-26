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
    let resolved = path.span().local_file().map_or(<_>::default(), |root| {
        root.parent().unwrap().join(path.value())
    });
    let root = resolved.parent().unwrap();
    let file = resolved.file_name().unwrap();
    barely_css_impl::compile(root, file)
        .map(|css| quote::quote!(#css))
        .unwrap_or_else(|err| {
            Error::new(path.span(), format!("Failed to compile CSS: {err}")).into_compile_error()
        })
        .into()
}
