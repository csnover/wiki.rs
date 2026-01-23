//! A compiler and parser that barely supports a subset of CSS.

#![warn(
    clippy::pedantic,
    clippy::missing_docs_in_private_items,
    missing_docs,
    rust_2018_idioms
)]

pub use barely_css_derive::compile;
pub use barely_css_impl::*;
