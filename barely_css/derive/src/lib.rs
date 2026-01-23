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
    barely_css_impl::compile("", &path.value())
        .map(|css| quote::quote!(#css))
        .unwrap_or_else(|err| {
            Error::new(path.span(), format!("Failed to compile CSS: {err}")).into_compile_error()
        })
        .into()
}
