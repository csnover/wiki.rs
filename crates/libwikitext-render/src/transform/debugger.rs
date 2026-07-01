//! A [`Sink`] that emits calls to a [formatter](core::fmt::Write) before
//! forwarding them to the next sink.

use super::{Chain, Sink};
use crate::StripMarker;

/// An emitter debugger.
#[derive(Debug)]
pub(super) struct Debugger<W, S> {
    /// The debug output.
    fmt: W,
    /// The output.
    next: S,
}

impl<W, S> Debugger<W, S> {
    /// Creates a new `Debugger` which emits to `next`.
    pub fn new(fmt: W, next: S) -> Self {
        Self { fmt, next }
    }
}

impl<W, S> core::ops::Deref for Debugger<W, S> {
    type Target = S;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.next
    }
}

impl<W, S> core::ops::DerefMut for Debugger<W, S> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.next
    }
}

impl<W, S: Chain> Chain for Debugger<W, S> {
    type Next = S::Next;

    #[inline]
    fn next(&self) -> &Self::Next {
        self.next.next()
    }

    #[inline]
    fn next_mut(&mut self) -> &mut Self::Next {
        self.next.next_mut()
    }
}

impl<W: Fmt, S: Sink> Sink for Debugger<W, S> {
    fn comment_end(&mut self) {
        write!(self.fmt, "-->");
        self.next.comment_end();
    }

    fn comment_start(&mut self) {
        write!(self.fmt, "<!--");
        self.next.comment_start();
    }

    fn entity(&mut self, value: char, raw: &str) {
        write!(self.fmt, "{raw}");
        self.next.entity(value, raw);
    }

    fn finish(self) -> String {
        self.next.finish()
    }

    fn new_line(&mut self) {
        writeln!(self.fmt);
        self.next.new_line();
    }

    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        write!(self.fmt, "{marker:?}");
        self.next.strip_marker(marker);
    }

    fn tag_attribute_end(&mut self, name: &str) {
        write!(self.fmt, "\"");
        self.next.tag_attribute_end(name);
    }

    fn tag_attribute_start(&mut self, name: &str) {
        write!(self.fmt, " {name}=\"");
        self.next.tag_attribute_start(name);
    }

    fn tag_end(&mut self, name: &str) {
        write!(self.fmt, "</{name}>");
        self.next.tag_end(name);
    }

    fn tag_start(&mut self, name: &str) {
        write!(self.fmt, "<{name}");
        self.next.tag_start(name);
    }

    fn tag_start_end(&mut self, name: &str) {
        write!(self.fmt, ">");
        self.next.tag_start_end(name);
    }

    fn text(&mut self, text: &str) {
        write!(self.fmt, "{text}");
        self.next.text(text);
    }
}

/// A debugger formatting trait.
///
/// This is a dumb workaround for how lack of specialisation makes it impossible
/// to do this generically.
trait Fmt {
    /// The `write_fmt` method compatible with [`macro@write`].
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>);
}

impl Fmt for core::fmt::Formatter<'_> {
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) {
        let _ = core::fmt::Formatter::write_fmt(self, args);
    }
}

impl Fmt for std::io::Stderr {
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) {
        let _ = std::io::Write::write_fmt(self, args);
    }
}

impl<T: Fmt> Fmt for &mut T {
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) {
        (*self).write_fmt(args);
    }
}
