//! Composible Wikitext to HTML streaming transformers.

mod accumulator;
mod attribute_filter;
mod buffer;
mod debugger;
mod dom_tree;
mod element_marker;
mod graf_wrapper;
mod outline;
mod pretty_text;
mod replace_text;

use super::StripMarker;
pub(super) use accumulator::Accumulator;
pub(super) use attribute_filter::AttributeFilter;
use buffer::Buffer;
pub(super) use dom_tree::DomTree;
pub(super) use element_marker::{EmptyMarker, TemplateMarker};
pub(super) use graf_wrapper::GrafWrapper;
use libwikitext_parse::VOID_TAGS;
pub(super) use outline::OutlineGenerator;
pub(super) use pretty_text::PrettyText;
pub(super) use replace_text::ReplaceText;

/// An intermediate sink.
pub(super) trait Chain {
    /// The type of the next sink in the chain.
    type Next;

    /// Returns a reference to the next sink in the chain.
    fn next(&self) -> &Self::Next;

    /// Returns a mutable reference to the next sink in the chain.
    fn next_mut(&mut self) -> &mut Self::Next;
}

/// A streaming node sink.
pub(super) trait Sink {
    /// Ends a comment.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                                   ^^^^
    /// ```
    fn comment_end(&mut self);

    /// Starts a comment.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                              ^^^^^
    /// ```
    fn comment_start(&mut self);

    /// A character entity.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                       ^^^^^^^
    /// ```
    fn entity(&mut self, value: char, raw: &str);

    /// Finish processing input.
    fn finish(self) -> String;

    /// A source newline.
    ///
    /// This is used for source-line-sensitive rules.
    fn new_line(&mut self);

    /// Writes strip marker content.
    fn strip_marker(&mut self, marker: &StripMarker<'_>);

    /// End a tag attribute with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                 ^
    /// ```
    fn tag_attribute_end(&mut self, name: &str);

    /// Emits a whole tag attribute with the given `name` and `value`.
    fn tag_attribute_full(&mut self, name: &str, value: &str) {
        self.tag_attribute_start(name);
        self.text(value);
        self.tag_attribute_end(name);
    }

    /// Start a tag attribute with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///     ^^^^^^^
    /// ```
    fn tag_attribute_start(&mut self, name: &str);

    /// Ends a node with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                                              ^^^^^^
    /// ```
    fn tag_end(&mut self, name: &str);

    /// Start a tag with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    /// ^^^^
    /// ```
    fn tag_start(&mut self, name: &str);

    /// Ends a start tag with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                  ^
    /// ```
    fn tag_start_end(&mut self, name: &str);

    /// Starts a tag with the given `name` and no attributes.
    fn tag_start_full(&mut self, name: &str) {
        self.tag_start(name);
        self.tag_start_end(name);
    }

    /// Text content.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///            ^^^^^  ^^^^            ^^^^^^^
    /// ```
    fn text(&mut self, text: &str);
}

impl<L: Sink, R: Sink> Sink for either::Either<&mut L, &mut R> {
    #[inline]
    fn comment_end(&mut self) {
        either::for_both!(self, sink => sink.comment_end());
    }

    #[inline]
    fn comment_start(&mut self) {
        either::for_both!(self, sink => sink.comment_start());
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        either::for_both!(self, sink => sink.entity(value, raw));
    }

    #[inline]
    fn finish(self) -> String {
        panic!("should not call this")
    }

    #[inline]
    fn new_line(&mut self) {
        either::for_both!(self, sink => sink.new_line());
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        either::for_both!(self, sink => sink.strip_marker(marker));
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        either::for_both!(self, sink => sink.tag_attribute_end(name));
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        either::for_both!(self, sink => sink.tag_attribute_start(name));
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        either::for_both!(self, sink => sink.tag_end(name));
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        either::for_both!(self, sink => sink.tag_start(name));
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        either::for_both!(self, sink => sink.tag_start_end(name));
    }

    #[inline]
    fn text(&mut self, text: &str) {
        either::for_both!(self, sink => sink.text(text));
    }
}

/// Generates an implementation of [`Chain`] for a generic type with the given
/// ident.
macro_rules! chainable {
    ($ty:ident) => {
        chainable! { $ty<S> }
    };

    ($ty:ident<$($lt:lifetime,)* $s:ident $(, $gen:ident)* $(,)?>) => {
        impl<$($lt,)* $s $(, $gen)*> $crate::transform::Chain for $ty<$($lt,)* $s $(, $gen)*>
        where
            $s: $crate::transform::Sink,
        {
            type Next = $s;

            #[inline]
            fn next(&self) -> &Self::Next {
                &self.next
            }

            #[inline]
            fn next_mut(&mut self) -> &mut Self::Next {
                &mut self.next
            }
        }
    };
}

use chainable;

/// Flushes runs of text tokens separated by newlines in `ws` to `next`.
#[inline]
fn flush_ws<S: Sink + ?Sized>(next: &mut S, mut ws: &str) {
    while let Some((text, rest)) = ws.split_once('\n') {
        if !text.is_empty() {
            next.text(text);
        }
        next.new_line();
        ws = rest;
    }
    if !ws.is_empty() {
        next.text(ws);
    }
}

/// Tokenises the given `html` and sends it to `next`.
pub(super) fn tokenise<S: Sink + ?Sized>(next: &mut S, html: &str) {
    use html5gum::{
        Span, Tokenizer,
        emitters::callback::{CallbackEmitter, CallbackEvent},
    };

    let mut in_attr = None::<String>;
    let mut in_tag = None;
    let emitter = CallbackEmitter::new(|event: CallbackEvent<'_>, _: Span<()>| {
        match event {
            CallbackEvent::OpenStartTag { name } => {
                // SAFETY: This data comes from a `&str`.
                let name = unsafe { str::from_utf8_unchecked(name) };
                next.tag_start(name);
                in_tag = Some(name.to_owned());
            }
            CallbackEvent::AttributeName { name } => {
                if let Some(name) = &in_attr {
                    next.tag_attribute_end(name);
                }
                // SAFETY: This data comes from a `&str`.
                let name = unsafe { str::from_utf8_unchecked(name) };
                next.tag_attribute_start(name);
                in_attr = Some(name.to_owned());
            }
            CallbackEvent::AttributeValue { value } | CallbackEvent::String { value } => {
                // SAFETY: This data comes from a `&str`.
                let value = unsafe { str::from_utf8_unchecked(value) };
                flush_ws(next, value);
            }
            CallbackEvent::CloseStartTag { self_closing } => {
                if let Some(name) = in_attr.take() {
                    next.tag_attribute_end(&name);
                }
                let name = in_tag.take().unwrap();
                next.tag_start_end(&name);
                if self_closing && !VOID_TAGS.contains(&name) {
                    next.tag_end(&name);
                }
            }
            CallbackEvent::EndTag { name } => {
                // SAFETY: This data comes from a `&str`.
                let name = unsafe { str::from_utf8_unchecked(name) };
                next.tag_end(name);
            }
            CallbackEvent::Comment { value } => {
                // SAFETY: This data comes from a `&str`.
                let value = unsafe { str::from_utf8_unchecked(value) };
                next.comment_start();
                next.text(value);
                next.comment_end();
            }
            CallbackEvent::Doctype { .. } => {}
            CallbackEvent::Error(error) => {
                log::warn!("Tokenizer error: {error}");
            }
        }

        None::<core::convert::Infallible>
    });

    Tokenizer::new_with_emitter(html, emitter).finish();
}
