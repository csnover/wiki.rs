//! A [`Sink`] that emits calls to [`stderr`](std::io::stderr) before forwarding
//! them to the next sink.

#![allow(
    clippy::allow_attributes,
    clippy::print_stderr,
    dead_code,
    reason = "this is debugging infrastructure"
)]

use super::{Sink, chainable};
use crate::StripMarker;

/// An emitter debugger.
#[derive(Debug)]
pub(super) struct Debugger<S: Sink> {
    /// The output.
    next: S,
}

impl<S: Sink> Debugger<S> {
    /// Creates a new `Debugger` which emits to `next`.
    pub fn new(next: S) -> Self {
        Self { next }
    }
}

chainable!(Debugger);

impl<S: Sink> Sink for Debugger<S> {
    fn comment_end(&mut self) {
        eprint!("-->");
        self.next.comment_end();
    }

    fn comment_start(&mut self) {
        eprint!("<!--");
        self.next.comment_start();
    }

    fn entity(&mut self, value: char, raw: &str) {
        eprint!("{raw:?}");
        self.next.entity(value, raw);
    }

    fn finish(self) -> String {
        self.next.finish()
    }

    fn new_line(&mut self) {
        eprintln!();
        self.next.new_line();
    }

    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        eprint!("{marker:?}");
        self.next.strip_marker(marker);
    }

    fn tag_attribute_end(&mut self, name: &str) {
        eprint!("\"");
        self.next.tag_attribute_end(name);
    }

    fn tag_attribute_start(&mut self, name: &str) {
        eprint!(" {name}=\"");
        self.next.tag_attribute_start(name);
    }

    fn tag_end(&mut self, name: &str) {
        eprint!("</{name}>");
        self.next.tag_end(name);
    }

    fn tag_start(&mut self, name: &str) {
        eprint!("<{name}");
        self.next.tag_start(name);
    }

    fn tag_start_end(&mut self, name: &str) {
        eprint!(">");
        self.next.tag_start_end(name);
    }

    fn text(&mut self, text: &str) {
        eprint!("{text:?}");
        self.next.text(text);
    }
}
