//! Renderer stack management types and functions.

use super::{
    Error, Result, State,
    expand_templates::{ExpandMode, ExpandTemplates},
    surrogate::Surrogate,
};
use crate::{
    lua::LuaFrame,
    title::Title,
    wikitext::{Argument, FileMap, Span, Spanned, Token},
};
use ::core::slice;
use core::fmt;
use piccolo::{Context, StashedString, Table};
use std::{
    borrow::Cow,
    cell::{Ref, RefCell},
    collections::HashMap,
    pin::{Pin, pin},
    rc::Rc,
};

/// A template transclusion or module call stack frame.
// TODO: There are two kinds of these, Wikitext and module. The source is always
// source code, but there could also be a value which is either the StashedClosure
// or the parser Output, which would make it clearer what things are associated
// with what?
pub(crate) struct StackFrame<'a> {
    /// The title of the article (template or module) rendered by this frame.
    pub name: Title,
    /// The source code of the frame.
    pub source: FileMap<'a>,
    /// The arguments passed in from the parent.
    pub arguments: KeyCacheKvs<'a, 'a>,
    /// The parent stack frame.
    pub parent: Option<Pin<&'a StackFrame<'a>>>,
    /// Named child frames, used by Scribunto modules to commit crimes.
    pub children: RefCell<HashMap<String, LuaFrame>>,
}

impl<'a> StackFrame<'a> {
    /// Creates a new stack frame for the given title.
    pub(super) fn new(name: Title, source: FileMap<'a>) -> Self {
        Self {
            name,
            source,
            arguments: <_>::default(),
            parent: None,
            children: <_>::default(),
        }
    }

    /// Convenience function for emitting a backtrace immediately for debugging
    /// purposes.
    pub fn backtrace(&self) {
        debug_backtrace(&self.name, self.parent.as_deref().unwrap_or(self));
    }

    /// Creates a new stack frame for the given title, with the given parent and
    /// arguments.
    pub fn chain(
        &'a self,
        name: Title,
        source: FileMap<'a>,
        arguments: &'a [Kv<'a>],
    ) -> Result<Self, Error> {
        check_recursion(self, &name)?;

        Ok(Self {
            name,
            source,
            arguments: KeyCacheKvs::new(arguments),
            parent: Some(pin!(self)),
            children: <_>::default(),
        })
    }

    /// Clones a stack frame to use with different source text.
    pub fn clone_with_source(&'a self, source: FileMap<'a>) -> StackFrame<'a> {
        let name =
            Title::from_parts(self.name.namespace(), self.name.key(), Some(&source), None).unwrap();
        Self {
            name,
            source,
            arguments: self.arguments.clone(),
            parent: self.parent,
            children: <_>::default(),
        }
    }

    /// Evaluates the given `expr` in the scope of this stack frame.
    pub fn eval(&'a self, state: &mut State<'_>, expr: &[Spanned<Token>]) -> Result<Cow<'a, str>> {
        // Clippy: The semicolon is required to make this a statement which
        // `#[rustfmt::skip]` can apply to until rust-lang/rust#15701 is fixed
        #[allow(clippy::unnecessary_semicolon)]
        #[rustfmt::skip]
        if let [Spanned { span, node: Token::Text }] = expr {
            return Ok(Cow::Borrowed(&self.source[span.into_range()]));
        };

        let mut evaluator = ExpandTemplates::new(if self.parent.is_some() {
            ExpandMode::Include
        } else {
            ExpandMode::Normal
        });
        evaluator.adopt_tokens(state, self, expr)?;
        Ok(evaluator.finish().into())
    }

    /// Evaluates the argument with the given key.
    pub fn expand(&self, state: &mut State<'_>, key: &str) -> Result<Option<Cow<'_, str>>> {
        Ok(
            if let Some(parent) = &self.parent
                && let Some((index, is_named)) = self.arguments.get_index(state, parent, key)?
            {
                self.arguments.value(state, parent, index)?.map(|value| {
                    if is_named {
                        match value {
                            Cow::Borrowed(b) => Cow::Borrowed(b.trim_ascii()),
                            Cow::Owned(o) => Cow::Owned(o.trim_ascii().to_string()),
                        }
                    } else {
                        value
                    }
                })
            } else {
                None
            },
        )
    }

    /// Returns all cached arguments for the given Lua context.
    ///
    /// This is a performance optimisation.
    pub fn expand_all_cached<'gc>(&self, ctx: Context<'gc>) -> Option<Table<'gc>> {
        let values = self.arguments.value_cache.borrow();
        (values.len() == self.arguments.raw.len()).then(|| {
            let table = Table::new(&ctx);
            let keys = self.arguments.key_map.borrow();
            for (key, index) in &keys.indices {
                let value = values.get(index).unwrap();
                let value = if keys.is_named(*index) {
                    value.trim_ascii()
                } else {
                    value
                };
                if let Ok(key) = key.parse::<i64>() {
                    table.set(ctx, key, ctx.intern(value.as_bytes())).unwrap();
                } else {
                    table
                        .set(
                            ctx,
                            ctx.intern(key.as_bytes()),
                            ctx.intern(value.as_bytes()),
                        )
                        .unwrap();
                }
            }
            table
        })
    }

    /// Returns the cached argument with the given key.
    #[inline]
    pub fn expand_cached(&self, key: &str) -> CachedValue<'_> {
        self.arguments.get_cached(key)
    }

    /// Returns an iterator over the keys of the frame’s arguments.
    pub fn keys(&self) -> KeyIter<'_> {
        KeyIter {
            arguments: &self.arguments,
            sp: self.parent.as_deref(),
            index: 0,
        }
    }

    /// Gets the root stack frame.
    // TODO: This should not exist, information from here should come from the
    // global state, there is only one root.
    pub fn root(&self) -> &StackFrame<'a> {
        let mut sp = self;
        while let Some(parent) = &sp.parent {
            sp = parent;
        }
        sp
    }
}

/// A cached stack frame parent value.
///
/// This is an optimisation to avoid expensive yields in the VM when the shared
/// state is not needed to actually get the value because nothing needs to be
/// rendered.
pub(crate) enum CachedValue<'a> {
    /// There is no value matching the key.
    Nil,
    /// There may be a value matching the key.
    Unknown,
    /// There is a cached value, and here it is!
    Cached(Ref<'a, str>),
}

/// Iterator over the argument keys in a [`KeyCacheKvs`].
pub(crate) struct KeyIter<'a> {
    /// The arguments to iterate.
    arguments: &'a KeyCacheKvs<'a, 'a>,
    /// The parent stack frame.
    ///
    /// This is required to expand any templates in key names.
    sp: Option<&'a StackFrame<'a>>,
    /// The next argument index.
    index: usize,
}

impl KeyIter<'_> {
    /// Advances the iterator and returns the next value.
    pub fn next(&mut self, state: &mut State<'_>) -> Result<Option<String>> {
        if let Some(sp) = self.sp
            && self.index != self.arguments.len()
        {
            let key = self.arguments.key(state, sp, self.index);
            self.index += 1;
            key
        } else {
            Ok(None)
        }
    }
}

impl fmt::Debug for StackFrame<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StackFrame")
            .field("name", &self.name)
            .field("source", &self.source)
            .field("arguments", &self.arguments)
            .field(
                "parent",
                &if let Some(parent) = &self.parent {
                    parent.name.key()
                } else {
                    <_>::default()
                },
            )
            .field("children", &self.children)
            .finish()
    }
}

/// Cached key-to-index map.
#[derive(Clone, Debug, Default)]
struct KeyMap {
    /// A map from a trimmed key to its index in the raw argument list.
    indices: HashMap<String, usize>,
    /// The last used unnamed key.
    last_unnamed_key: usize,
    /// A bit map of named keys.
    named_keys: [u8; 8],
    /// Spill of named keys for things with way too many arguments.
    named_keys_heap: Vec<u8>,
}

impl KeyMap {
    /// Gets the index and named flag for the given key.
    #[inline]
    fn get_cached(&self, key: &str) -> Option<(usize, bool)> {
        self.indices
            .get(key)
            .map(|&index| (index, self.is_named(index)))
    }

    /// Returns true if the given index is a named key.
    // Clippy: Value can only be 0..=7.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    fn is_named(&self, index: usize) -> bool {
        let slot = index / 8;
        let bits = if slot >= self.named_keys.len() {
            let slot = slot - self.named_keys.len();
            if slot >= self.named_keys_heap.len() {
                return false;
            }
            self.named_keys_heap[slot]
        } else {
            self.named_keys[slot]
        };

        (bits & (1 << (index as u8 & 7))) != 0
    }

    /// Marks the key at the given index as a named key.
    // Clippy: Value can only be 0..=7.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    fn set_named(&mut self, index: usize) {
        let slot = index / 8;
        let bits = if slot >= self.named_keys.len() {
            let slot = slot - self.named_keys.len();
            if slot >= self.named_keys_heap.len() {
                self.named_keys_heap.resize(slot + 1, 0);
            }
            &mut self.named_keys_heap[slot]
        } else {
            &mut self.named_keys[slot]
        };

        *bits |= 1 << (index as u8 & 7);
    }
}

/// A list of k-v pairs with a cached key map.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeyCacheKvs<'call, 'args> {
    /// A lazily populated key-index map into [`Self::raw`].
    key_map: Rc<RefCell<KeyMap>>,
    /// The raw arguments passed to the function.
    raw: &'call [Kv<'args>],
    /// Cached values.
    value_cache: Rc<RefCell<HashMap<usize, String>>>,
}

impl<'call, 'args> KeyCacheKvs<'call, 'args> {
    /// Creates a new [`KeyCacheKvs`] using the given arguments.
    pub fn new(raw: &'call [Kv<'args>]) -> Self {
        Self {
            key_map: <_>::default(),
            raw,
            value_cache: <_>::default(),
        }
    }

    /// Evaluates an entire k-v pair at the given index as a single value.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    pub fn eval(
        &self,
        state: &mut State<'_>,
        sp: &'call StackFrame<'_>,
        index: usize,
    ) -> Result<Option<Cow<'call, str>>> {
        self.raw
            .get(index)
            .map(|arg| arg.eval(state, sp))
            .transpose()
    }

    /// Evaluates the value part of a k-v pair with the given key.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    pub fn get(
        &self,
        state: &mut State<'_>,
        sp: &'call StackFrame<'_>,
        key: &str,
    ) -> Result<Option<Cow<'call, str>>> {
        if let Some((index, _)) = self.get_index(state, sp, key)? {
            self.value(state, sp, index)
        } else {
            Ok(None)
        }
    }

    /// Returns the cached value part of a k-v pair with the given key.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    fn get_cached(&self, key: &str) -> CachedValue<'_> {
        let key_map = self.key_map.borrow();
        let index = key_map.indices.get(key).copied();
        if let Some(index) = index {
            Ref::filter_map(self.value_cache.borrow(), |cache| {
                cache.get(&index).map(|value| {
                    if key_map.is_named(index) {
                        value.trim_ascii()
                    } else {
                        value
                    }
                })
            })
            .map_or(CachedValue::Unknown, CachedValue::Cached)
        } else if key_map.indices.len() == self.raw.len() {
            CachedValue::Nil
        } else {
            CachedValue::Unknown
        }
    }

    /// Returns the index of an argument by key.
    pub fn get_index(
        &self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        key: &str,
    ) -> Result<Option<(usize, bool)>> {
        if let value @ Some(..) = self.key_map.borrow().get_cached(key) {
            return Ok(value);
        } else if self.key_map.borrow().indices.len() != self.len() {
            let mut key_map = self.key_map.borrow_mut();
            for index in key_map.indices.len()..self.len() {
                let (name, is_named) = if let Some(name) = self.name(state, sp, index)? {
                    key_map.set_named(index);
                    (name.trim_ascii().to_string(), true)
                } else {
                    key_map.last_unnamed_key += 1;
                    (key_map.last_unnamed_key.to_string(), false)
                };
                let is_match = name == key;
                key_map.indices.insert(name, index);
                if is_match {
                    return Ok(Some((index, is_named)));
                }
            }
        }
        Ok(None)
    }

    /// Returns the trimmed key for the given index.
    pub fn key(
        &self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        index: usize,
    ) -> Result<Option<String>> {
        if index >= self.raw.len() {
            return Ok(None);
        }

        if self.key_map.borrow().indices.len() <= index {
            let mut key_map = self.key_map.borrow_mut();
            for index in key_map.indices.len()..=index {
                let name = if let Some(name) = self.name(state, sp, index)? {
                    key_map.set_named(index);
                    name.trim_ascii().to_string()
                } else {
                    key_map.last_unnamed_key += 1;
                    key_map.last_unnamed_key.to_string()
                };
                key_map.indices.insert(name, index);
            }
        }

        Ok(self
            .key_map
            .borrow()
            .indices
            .iter()
            .find_map(|(k, i)| (*i == index).then(|| k.clone())))
    }

    /// Returns true if there are no arguments.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Returns an iterator over the raw arguments.
    pub fn iter(&self) -> impl Iterator<Item = &Kv<'_>> {
        self.raw.iter()
    }

    /// Returns the number of arguments.
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Evaluates the name part of a k-v pair at the given index.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    pub fn name(
        &self,
        state: &mut State<'_>,
        sp: &'call StackFrame<'_>,
        index: usize,
    ) -> Result<Option<Cow<'call, str>>> {
        self.raw
            .get(index)
            .and_then(|arg| arg.name(state, sp).transpose())
            .transpose()
    }

    /// Evaluates the value part of a k-v pair at the given index.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    pub fn value(
        &self,
        state: &mut State<'_>,
        sp: &'call StackFrame<'_>,
        index: usize,
    ) -> Result<Option<Cow<'call, str>>> {
        if !self.value_cache.borrow().contains_key(&index) {
            let Some(value) = self
                .raw
                .get(index)
                .map(|arg| arg.value(state, sp))
                .transpose()?
            else {
                return Ok(None);
            };

            self.value_cache
                .borrow_mut()
                .insert(index, value.to_string());
        }

        // TODO: Do something better that does not require cloning
        Ok(self
            .value_cache
            .borrow()
            .get(&index)
            .map(|value| Cow::Owned(value.clone())))
    }
}

impl<'args, I> core::ops::Index<I> for KeyCacheKvs<'_, 'args>
where
    I: core::slice::SliceIndex<[Kv<'args>]>,
{
    type Output = <I as core::slice::SliceIndex<[Kv<'args>]>>::Output;

    fn index(&self, index: I) -> &Self::Output {
        self.raw.index(index)
    }
}

/// A helper for handling calls to function-like items (parser functions and
/// extension tags).
pub(crate) struct IndexedArgs<'args, 'call, 'sp> {
    /// The raw arguments passed to the function.
    pub arguments: KeyCacheKvs<'call, 'args>,
    /// The name of the callee.
    pub callee: &'call str,
    /// The current stack frame.
    pub sp: &'call StackFrame<'sp>,
    /// The span of the callee in the source text. This is `None` for calls
    /// from Lua scripts.
    pub span: Option<Span>,
}

impl IndexedArgs<'_, '_, '_> {
    /// Evaluates an entire k-v pair at the given index as a single value.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    pub fn eval(&self, state: &mut State<'_>, index: usize) -> Result<Option<Cow<'_, str>>> {
        self.arguments.eval(state, self.sp, index)
    }

    /// Evaluates the value part of a k-v pair with the given key.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    pub fn get(&self, state: &mut State<'_>, key: &str) -> Result<Option<Cow<'_, str>>> {
        self.arguments.get(state, self.sp, key)
    }

    /// Returns true if there are no arguments.
    pub fn is_empty(&self) -> bool {
        self.arguments.is_empty()
    }

    /// Returns an iterator over the raw arguments.
    pub fn iter(&self) -> impl Iterator<Item = &Kv<'_>> {
        self.arguments.iter()
    }

    /// Returns the number of arguments.
    pub fn len(&self) -> usize {
        self.arguments.len()
    }
}

impl<'args, 'call> IntoIterator for &IndexedArgs<'args, 'call, '_> {
    type Item = <&'call [Kv<'args>] as IntoIterator>::Item;

    type IntoIter = <&'call [Kv<'args>] as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.arguments.raw.iter()
    }
}

/// A collapsible key-value pair.
///
/// In Wikitext, k-v pairs are used in template arguments, tag attributes, and
/// wikilink image arguments. In the case of template arguments, these values
/// are only treated as k-vs in template expansions; for parser function calls,
/// they are usually treated as scalar values.
#[derive(Clone, Debug)]
pub(crate) enum Kv<'a> {
    /// A regular template argument.
    ///
    /// ```wikitext
    /// {{#fn:a{{b}}|extra|{{argument}}s}}
    ///              ^^^^^ ^^^^^^^^^^^^^
    /// ```
    Argument(&'a Spanned<Argument>),
    /// A list of borrowed tokens from a template target.
    ///
    /// ```wikitext
    /// {{#fn:a{{b}}|extra|{{argument}}s}}
    ///       ^^^^^^
    /// ```
    Partial(Vec<&'a Spanned<Token>>),
    /// A k-v argument provided by a Lua script rather than a Wikitext document.
    // TODO: Make sure that Lua strings always end up going through the parser
    // since otherwise they will be emitted with improper entity encoding.
    String(StashedString, StashedString),
}

impl Kv<'_> {
    /// Evaluates the whole argument as a single string.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    pub fn eval<'a>(
        &'a self,
        state: &mut State<'_>,
        sp: &'a StackFrame<'_>,
    ) -> Result<Cow<'a, str>> {
        match self {
            Kv::Argument(argument) => sp.eval(state, &argument.content),
            Kv::Partial(nodes) => {
                if nodes.len() == 1 {
                    sp.eval(state, slice::from_ref(nodes[0]))
                } else {
                    let mut out = String::new();
                    for node in nodes {
                        out += &sp.eval(state, slice::from_ref(node))?;
                    }
                    Ok(Cow::Owned(out))
                }
            }
            Kv::String(_, value) => state
                .statics
                .vm
                .try_enter(|ctx| {
                    let value = ctx.fetch(value).to_str()?;
                    Ok(Cow::Owned(value.to_string()))
                })
                .map_err(Into::into),
        }
    }

    /// Evaluates the name-part of a k-v argument.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    pub fn name<'a>(
        &'a self,
        state: &mut State<'_>,
        sp: &'a StackFrame<'_>,
    ) -> Result<Option<Cow<'a, str>>> {
        match self {
            Kv::Partial(_) => {
                log::warn!("The thing that should never happen, happened");
                Ok(None)
            }
            Kv::String(key, _) => state
                .statics
                .vm
                .try_enter(|ctx| {
                    let key = ctx.fetch(key).to_str()?;
                    Ok(Some(Cow::Owned(key.to_string())))
                })
                .map_err(Into::into),
            Kv::Argument(argument) => argument.name().map(|name| sp.eval(state, name)).transpose(),
        }
    }

    /// Evaluates the value-part of k-v argument.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    pub fn value<'a>(
        &'a self,
        state: &mut State<'_>,
        sp: &'a StackFrame<'_>,
    ) -> Result<Cow<'a, str>> {
        match self {
            Kv::Argument(argument) => sp.eval(state, argument.value()),
            _ => self.eval(state, sp),
        }
    }
}

/// Enforce a maximum stack depth and ensure that no templates are being called
/// recursively.
fn check_recursion(mut sp: &StackFrame<'_>, title: &Title) -> Result<(), Error> {
    let mut count = 0;
    let root_sp = sp;
    while let Some(frame) = &sp.parent {
        // MediaWiki documentation says this is the stack limit
        if count == 40 {
            debug_backtrace(title, root_sp);
            return Err(Error::StackOverflow(title.to_string()));
        // In MW, only template calls participate in loop checking so it is
        // OK to loop back to the root frame, which happens with e.g.
        // Template:Issubst -> Template:Issubst/doc -> Template:Issubst
        } else if frame.parent.is_some() && frame.name == *title {
            debug_backtrace(title, sp);
            return Err(Error::TemplateRecursion(title.to_string()));
        }
        sp = frame;
        count += 1;
    }
    Ok(())
}

/// Emits a backtrace to the error log.
fn debug_backtrace(title: &Title, mut sp: &StackFrame<'_>) {
    let mut index = 0;
    log::error!("{index:>2}. {title}");
    loop {
        index += 1;
        log::error!("{index:>2}. {}", sp.name);
        if let Some(p) = &sp.parent {
            sp = p;
        } else {
            break;
        }
    }
}
