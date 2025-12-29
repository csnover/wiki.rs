//! A parsing expression grammar for Wikitext documents.
//!
//! This grammar converts the Wikitext document into a pretty flat token tree
//! which requires additional context-aware processing later.

// Clippy: Most of the arguments are hidden. It is not possible to apply
// this annotation directly to the parser because rust-peg does not understand
// it.
#![allow(clippy::too_many_arguments)]

// This code is heavily adapted from the Parsoid grammar at
// <https://github.com/wikimedia/mediawiki-services-parsoid>
// based on
// Git-Commit-ID: 9cc7fc706b727c392b53fe7fe571747901424065
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::{
    AnnoAttribute, Argument, Globals, HeadingLevel, InclusionMode, LangFlags, LangVariant,
    MARKER_PREFIX, MARKER_SUFFIX, Parser, TextStyle, TextStylePosition, Token, VOID_TAGS,
    codemap::{Span, Spanned},
    config::HTML5_TAGS,
};
use core::iter;
use peg::RuleResult;
use std::{cell::Cell, collections::HashSet};

peg::parser! { pub(super) grammar wikitext(state: &Parser<'_>, globals: &Globals) for str {
    /// The top-level start rule.
    pub rule start() -> Vec<Spanned<Token>>
    = ctx:({ Context::default() })
      r:redirect_block(&ctx)?
      t:tlb(&ctx)*
      n:newline_token()*
    { reduce_tree(r.into_iter().flatten().chain(t.into_iter().flatten()).chain(n)) }

    /// The top-level start rule for processing text returned by a Lua module.
    pub rule start_no_expansion() -> Vec<Spanned<Token>>
    = ctx:({ Context::default().with_after_expansion() })
      t:tlb(&ctx)*
      n:newline_token()*
    { reduce_tree(t.into_iter().flatten().chain(n)) }

    ////////////////
    // Block flow //
    ////////////////

    /// A document is a sequence of top-level blocks. Tokens are emitted in
    /// chunks every top-level block to avoid buffering the full document.
    rule tlb(ctx: &Context) -> Vec<Spanned<Token>>
    = !eof()
      t:block(ctx)
    { t }

    /// The actual content of a top-level block.
    rule block(ctx: &Context) -> Vec<Spanned<Token>>
    = t:(
      sol_block_line(ctx)
      / inlineline(ctx)
      / empty_block(ctx)
    )
    // TODO: Is this ever *not* balancing?
    should_balance_quotes:(&eolf() { true } / { false })
    { if should_balance_quotes { balance_quotes(t) } else { t } }

    /// Whitespace followed by a block item that is anchored to the start of a
    /// line (headings, list items, horizontal rules, tables).
    rule sol_block_line(ctx: &Context) -> Vec<Spanned<Token>>
    = s:sol(ctx)
      // eat an empty line before the block
      s2:(space()* t:sol(ctx) { t })?
      bl:block_line(ctx)
    {
        reduce_tree(
            s.into_iter().chain(s2.into_iter().flatten()).chain(bl)
        )
    }

    /// A block containing only elements which do not participate in breaking
    /// another block (whitespace, comments, includes, annotations, and behavior
    /// switches).
    rule empty_block(ctx: &Context) -> Vec<Spanned<Token>>
      // Don't match `sol` on its own when in a table data-block. This ensures
      // we don't match a stray `||` on the line as a `<td>` when the newline
      // itself may not have started with a `<td>`. The `sol_block_line` rule
      // does allow `sol` to be matched when required for lists, headings, hrs,
      // and fresh table lines.
    = &assert(
        ctx.table_caption || ctx.full_table || !ctx.table_data_block,
        "in table caption, in full table, outside table data block")
      t:sol(ctx)
      !sof() /* this ensures `sol` advanced beyond the start of the file */
      !inline_breaks(ctx)
      not_empty()
    { t }

    /// A block nested in another item (except tables).
    rule nested_block(ctx: &Context) -> Vec<Spanned<Token>>
    = !inline_breaks(ctx) /* avoid consuming end delimiters meant for other items */
      t:block(ctx)
    { t }

    /// A block nested in a table.
    ///
    /// This rule is the same as `nested_block`, but avoids matching table
    /// tokens intended for the outer table.
    rule nested_block_in_table(ctx: &Context) -> Vec<Spanned<Token>>
    = // nested_block starts with !inline_breaks so test it early here as
      // an optimization
      !inline_breaks(&ctx.with_table_data_block())
      // TODO: don't rely on a lame look-ahead like this; use syntax stops
      // instead, so that multi-line th content followed by a line prefixed with
      // a comment is also handled. Alternatively, implement a sol look-behind
      // assertion accepting spaces and comments.
      !(sol(ctx) (space()* sol(ctx))? space()* (pipe() / "!"))
      // Setting `table_data_block` avoids recursion between `table_data_tag`
      // and `nested_block_in_table` <https://phabricator.wikimedia.org/T59670>
      t:nested_block(&ctx.with_table_data_block())
    { t }

    /// A block item that is anchored to the start of a line (headings, list
    /// items, horizontal rules, tables).
    rule block_line(ctx: &Context) -> Vec<Spanned<Token>>
    = heading(ctx)
    / list_item(ctx)
    / t:hr(ctx) { vec![t] }
    / &[' '|'\t'|'<'|'{'|'}'|'|'|'!']
      t:table_line(ctx) { t }

    /// Matches the start of a line and produces items which are transparent to
    /// productions which only match immediately after a newline.
    ///
    /// ```wikitext
    /// <!-- extra stuff -->* List item
    /// ^^^^^^^^^^^^^^^^^^^^
    /// ```
    // TODO: It sucks to use this in rules where the return value is wasted
    // because it is still allocating and reducing the tree.
    rule sol(ctx: &Context) -> Vec<Spanned<Token>>
    = // optimization: fail fast before putting entries in the packrat cache
      &#{|input, pos| if pos == 0 || matches!(input.as_bytes().get(pos), Some(b'\r' | b'\n')) {
            RuleResult::Matched(pos, ())
        } else {
            RuleResult::Failed
        }
      }
      p:sol_prefix()
      elc:empty_lines_with_comments()?
      st:sol_transparent(ctx)*
    {
        let p = match (p, elc) {
            (Some(mut p), Some(elc)) => {
                p.span.end = elc.span.end;
                Some(p)
            }
            (Some(p), None) => Some(p),
            (None, Some(elc)) => Some(elc),
            (None, None) => None,
        };

        reduce_tree(p.into_iter().chain(st.into_iter().flatten()))
    }

    /// Start of input or start of line. Consumes and returns a newline
    /// token for context-sensitive applications (e.g. paragraph wrappers).
    rule sol_prefix() -> Option<Spanned<Token>>
    = t:newline_token()
      { Some(t) }
    / pos:position!()
      {? if pos == 0 { Ok(None) } else { Err("start of file") } }

    /// A context-sensitive run of empty lines with at least one comment.
    rule empty_lines_with_comments() -> Spanned<Token>
    = spanned(<(space()* comment() space_or_comment()* newline())+ {
        Token::NewLine
    }>)

    /// An item which may exist at the start of a line without stopping the
    /// production of block flow items (e.g. headers, horizontal rules) which
    /// would normally only match immediately after a newline.
    rule sol_transparent(ctx: &Context) -> Vec<Spanned<Token>>
    = t:strip_marker() { vec![t] }
    / t:comment() { vec![t] }
    / include_limits(ctx)
    / t:annotation_tag(ctx) { vec![t] }
    / t:behavior_switch() { vec![t] }

    ///////////////////////
    // Block-level items //
    ///////////////////////

    /// An article redirect block with optional trailing content and a single
    /// extra block line.
    ///
    /// ```wikitext
    /// #REDIRECT: [[Target]] <!-- extra stuff -->
    ///
    /// * Extra block line
    /// ```
    ///
    /// The only valid position for this rule is as the first block in a
    /// document.
    rule redirect_block(ctx: &Context) -> Vec<Spanned<Token>>
    = &sof()
      // Redirect has to be the first alternative or it will be parsed as
      // an ordered list item
      r:redirect(ctx)
      cil:sol_transparent(ctx)*
      bl:block_line(ctx)?
    {
        reduce_tree(
            iter::once(r)
                .chain(cil.into_iter().flatten())
                .chain(bl.into_iter().flatten())
        )
    }

    /// An article redirect block.
    ///
    /// ```wikitext
    /// #REDIRECT: [[Target]] <!-- extra stuff -->
    /// ^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule redirect(ctx: &Context) -> Spanned<Token>
    = spanned(<
        redirect_magic()
        space_or_newline()*
        (":" space_or_newline()*)?
        link:wikilink(ctx)
        {?
            if matches!(link.as_slice(), [Spanned { node: Token::Link { .. }, .. }]) {
                Ok(Token::Redirect {
                    link: link.into_iter().next().map(Box::new).unwrap(),
                })
            } else {
                Err("wikilink")
            }
        }
    >)

    /// A redirect magic word.
    ///
    /// ```wikitext
    /// #REDIRECT: [[Target]]
    /// ^^^^^^^^^
    /// ```
    rule redirect_magic()
    = [' '|'\t'|'\n'|'\r'|'\0'|'\x0b']*
      magic:$([^' '|'\t'|'\n'|'\r'|'\x0c'|':'|'[']+)
    {?
        contains_ignore_case(&state.config.redirect_magic_words, magic)
            .then_some(())
            .ok_or("redirect magic word")
    }

    /// A heading, optionally trailed by any items which are SOL transparent.
    ///
    /// ```wikitext
    /// =h1=<!-- -->
    /// ==h2==
    /// ===h3===
    /// ```
    ///
    /// etc.
    rule heading(ctx: &Context) -> Vec<Spanned<Token>>
    = t:spanned(<
      &"=" // guard, to make sure '='+ will match.
      // XXX: Also check to end to avoid inline parsing?
      s:spanned(<$("="+)>)
      c:inlineline(&ctx.with_h())?
      // If `inlineline` matches, this needs to see at least one `=` since this
      // could also be some template argument on a new line that starts with a
      // `=`, not a heading
      e:spanned(<$("="+)>)
      &assert(c.is_some() || s.len() > 2, "heading")
      {
        let level = if e.is_empty() {
            (s.len() - 1) / 2
        } else {
            s.len().min(e.len())
        }.min(6);

        let extra_left = (s.len() > level).then(|| {
            let delta = s.len() - level;
            Spanned::new(Token::Text, s.span.end - delta, s.span.end)
        });

        let extra_right = (e.len() > level).then(|| {
            let delta = e.len() - level;
            Spanned::new(Token::Text, s.span.start, s.span.start + delta)
        });

        let content = extra_left.into_iter()
            .chain(c.into_iter().flatten())
            .chain(extra_right.into_iter())
            .collect();

        if let Ok(Ok(level)) = u8::try_from(level).map(HeadingLevel::try_from) {
            Token::Heading { level, content }
        } else {
            panic!("calculated level {level} from s = {s:?}, e = {e:?}");
        }
      }
      >)
      spc:(t:spanned(<space() { Token::Text }>) { vec![t] } / sol_transparent(ctx))*
      &eolf()
    { reduce_tree(iter::once(t).chain(spc.into_iter().flatten())) }

    /// A horizontal rule.
    rule hr(ctx: &Context) -> Spanned<Token>
    = spanned(<
      "----" "-"*
      line_content:(&sol(ctx) { false } / { true })
      { Token::HorizontalRule { line_content } }
    >)

    ///////////
    // Lists //
    ///////////

    /// An unordered, ordered, or definition list item.
    ///
    /// ```wikitext
    /// * Unordered
    /// # Ordered
    /// ; Term : Detail
    /// : Detail
    /// ```
    rule list_item(ctx: &Context) -> Vec<Spanned<Token>>
    = dtdd(ctx)
    / t:hacky_dl_uses(ctx) { vec![t] }
    / t:li(ctx) { vec![t] }

    /// An unordered or ordered list item.
    ///
    /// ```wikitext
    /// * Unordered
    /// # Ordered
    /// ```
    rule li(ctx: &Context) -> Spanned<Token>
    = spanned(<
      bullets:spanned(<list_char()+ {}>)
      content:inlineline(ctx)?
      // `inline_breaks` matches template terminator
      &(eolf() / inline_breaks(ctx))
      { Token::ListItem { bullets: bullets.span, content: content.unwrap_or(vec![]) } }
    >)

    /// An indented table.
    ///
    /// The documentation for `{|` says it is only valid at the start of a line,
    /// but in reality it is also allowed in this form as of commit
    /// a0746946312b0f1eda30a2c793f5f7052e8e5f3a.
    ///
    /// ```wikitext
    /// ::{| ... |}
    /// ```
    rule hacky_dl_uses(ctx: &Context) -> Spanned<Token>
    = t:spanned(<
      bullets:spanned(<":"+ {}>)
      space_or_comment()*
      !inline_breaks(ctx)
      content:table_start_tag(ctx)
      { Token::ListItem {
          bullets: bullets.span,
          content,
      } }
    >)
    { t }

    /// A list of definition list items.
    ///
    /// ```wikitext
    /// ; Term : Detail
    /// : Detail
    /// ```
    rule dtdd(ctx: &Context) -> Vec<Spanned<Token>>
    = bullets:spanned(<(!(";" !list_char()) list_char())* ";" {}>)
      details:(
          prev_content:spanned(<t:inlineline(&ctx.with_colon())? { t.unwrap_or(vec![]) }>)
          next_bullet:spanned(<":" {}>)
          { (prev_content, next_bullet) }
      )*
      last_content:spanned(<t:inlineline(ctx)? { t.unwrap_or(vec![]) }>)
      &eolf()
    {
        // TODO: Surely there is a less awful way to do this?
        let bullets = Cell::new(bullets);
        details
            .into_iter()
            .map(|(v, next_k)| {
                (bullets.replace(next_k), v)
            })
            .chain(iter::once_with(|| (bullets.get(), last_content)))
            .map(|(bullets, content)| {
                let start = bullets.span.start;
                let end = content.span.end;
                Spanned::new(Token::ListItem {
                    bullets: bullets.span, content: content.node
                }, start, end)
            })
            .collect()
    }

    /// Characters that are used by list items.
    rule list_char()
    = ['*'|'#'|':'|';']

    ////////////
    // Tables //
    ////////////

    // Because it is common for tables to be built by template fragments, all
    // table rules only match fragments. The one exception is
    // `full_table_in_link_caption`, where table start and end delimiters
    // *must not* come from templates (see that rule’s documentation for
    // details).
    //
    // This will continue to be the case unless/until the parser is updated to
    // so that it evaluates templates immediately and replaces the content in
    // the input stream with the template content while the parser is still
    // parsing, or at least until it evaluates templates in ambiguous positions
    // like this to decide what the grammar is supposed to be. Either of these
    // is technically feasible, but both create separate nightmares with various
    // performance implications and added complexity. Still, if Parsoid is the
    // future for official Wikitext parsing (after a decade, any day now?), then
    // it is fair to assume any edge cases which it does not handle also do not
    // need to be handled here.

    /// A table inside an image caption.
    ///
    /// ```wikitext
    /// [[Image:Foo.jpg|{| ... |}]]
    ///                 ^^^^^^^^^
    /// ```
    ///
    /// Due to the ambiguous grammar, table start and end delimiters cannot be
    /// produced by templates in these positions:
    ///
    /// ```wikitext
    /// [[Image:Foo.jpg|left|30px|Example 1
    /// {{Start tag?}}
    /// |foo
    /// {{End tag?}}
    /// ]]
    /// ```
    ///
    /// In this example, if these templates were to produce table tags, `|foo`
    /// would be a table cell; otherwise, it would be a new wikilink argument.
    /// To resolve the ambiguity, this rule assumes it is a wikilink argument.
    rule full_table_in_link_caption(ctx: &Context) -> Vec<Spanned<Token>>
    = !inline_breaks(ctx)
      // "linkdesc" is suppressed to provide a nested parsing context in which
      // to parse the table. Otherwise, we may break on pipes in the
      // `table_start_tag` and `table_row_tag` attributes. This is more
      // permissive than the old PHP parser but likelier to match the user's
      // intent.
      //
      // Recursion protection from `table_data_block` is suppressed since we're
      // trying to parse a full table and if a link is itself nested in a table
      // this would always stop.
      ctx:({ ctx.without_linkdesc().with_table().with_full_table().without_table_data_block() })
      a:space_or_comment()*
      b:table_start_tag(&ctx)
      // Accept multiple end tags since a nested table may have been
      // opened in the table content line.
      c:embedded_table_body(&ctx)+
    { reduce_tree(a.into_iter().chain(b).chain(c.into_iter().flatten())) }

    /// The body and terminator of a table inside an image caption.
    ///
    /// ```wikitext
    /// [[Image:Foo.jpg|{| ... |}]]
    ///                    ^^^^^^
    /// ```
    rule embedded_table_body(ctx: &Context) -> Vec<Spanned<Token>>
    = a:embedded_table_line(ctx)*
      b:embedded_full_table_line_prefix(&ctx)+
      c:table_end_tag()
    { reduce_tree(a.into_iter().flatten().chain(b.into_iter().flatten()).chain(iter::once(c))) }

    /// The body of a table inside an image caption.
    ///
    /// ```wikitext
    /// [[Image:Foo.jpg|{| ... |}]]
    ///                    ^^^
    /// ```
    rule embedded_table_line(ctx: &Context) -> Vec<Spanned<Token>>
    = a:embedded_full_table_line_prefix(&ctx)+
      b:(table_content_line(&ctx) / template_param_or_template(&ctx))
    { reduce_tree(a.into_iter().flatten().chain(b)) }

    /// Matches the start of a line and produces items which are transparent to
    /// productions which only match immediately after a newline, which are
    /// discarded, plus any additional spaces or comments, which are returned.
    rule embedded_full_table_line_prefix(ctx: &Context) -> Vec<Spanned<Token>>
    = s:sol(ctx)
      t:space_or_comment()*
      not_empty()
    { reduce_tree(s.into_iter().chain(t)) }

    /// Any table start, caption, row, heading, data, or end item.
    ///
    /// ```wikitext
    /// {| k="v" |+ c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    /// ```
    ///
    /// This rule *assumes* start-of-line position and is faster than using
    /// `sol_block_line` in table contexts.
    rule table_line(ctx: &Context) -> Vec<Spanned<Token>>
    = space_or_comment()*
      !inline_breaks(ctx)
      t:(
        table_start_tag(ctx)
        / table_content_line(&ctx.with_table())
        / t:table_end_tag() { vec![t] }
      )
    { t }

    /// A table start tag.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    /// ^^^^^^^^^
    /// ```
    rule table_start_tag(ctx: &Context) -> Vec<Spanned<Token>>
    = t:spanned(<
      "{" pipe()
      // ok to normalize away stray |} on rt (see T59360)
      attributes:table_attributes(&ctx.without_table())
      space()*
      { attributes }
    >)
    {
        let mut t = t;
        let comment = t.node.last_mut().and_then(|attribute| {
            attribute.node.content.pop_if(|token| {
                matches!(
                    token,
                    Spanned { node: Token::Comment { .. }, .. }
                )
            }).map(|comment| {
                (comment, attribute.node.content.is_empty())
            })
        });

        let comment = comment.map(|(comment, is_empty)| {
            // If the comment was the only thing in the attribute, it was not
            // actually an attribute. 'Template:Markup/row' triggers this in
            // non-include mode
            if is_empty {
                t.node.pop();
            }
            comment
        });

        let t = t.map_node(|attributes| Token::TableStart { attributes });
        if let Some(comment) = comment {
            vec![t, comment]
        } else {
            vec![t]
        }
    }

    /// Any table caption, row, heading, or data item.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    ///          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule table_content_line(ctx: &Context) -> Vec<Spanned<Token>>
    = table_heading_tags(&ctx.with_table_head())
    / t:table_row_tag(ctx) { vec![t] }
    / table_data_tags(ctx)
    / table_caption_tag(ctx)

    /// A table caption.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    ///          ^^^^^^^^^^^^^^^
    /// ```
    // FIXME: Not sure if we want to support it, but this should allow columns.
    rule table_caption_tag(ctx: &Context) -> Vec<Spanned<Token>>
    = &assert(!ctx.table_data_block, "not in table data block")
      caption:spanned(<
        pipe() "+"
        attributes:(t:row_syntax_table_attrs(ctx)? { t.unwrap_or(vec![]) })
        { Token::TableCaption { attributes } }
      >)
      // It is not reasonably possible to encapsulate the table caption content
      // inside the caption token itself because wikitables can be nested, and
      // this grammar only parses table *parts*, so it does not have enough
      // state information to know whether a new table part encountered in
      // `nested_block_in_table` is a continuation of a child table or a
      // terminator for the current table part.
      // avoid recursion via nested_block_in_table
      content:nested_block_in_table(&ctx.with_table_caption())*
    { reduce_tree(iter::once(caption).chain(content.into_iter().flatten())) }

    /// A table row tag.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    ///                         ^^^^^^^^^^^
    /// ```
    rule table_row_tag(ctx: &Context) -> Spanned<Token>
    = // avoid recursion via nested_block_in_table
      &assert(!ctx.table_data_block, "not in table data block")
      t:spanned(<
        pipe() "-"+
        attributes:(table_attributes(&ctx.without_table()))
        space()*
        { Token::TableRow { attributes } }
      >)
    { t }

    /// Table heading tags.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    ///                                    ^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule table_heading_tags(ctx: &Context) -> Vec<Spanned<Token>>
    = first:table_heading_tag(ctx, <"!">)
      rest:table_heading_tag(ctx, <("!!" / pipe_pipe()) {}>)*
    { reduce_tree(first.into_iter().chain(rest.into_iter().flatten())) }

    /// A single table heading.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    ///                                      ^^^^^^^^^^^    ^^
    /// ```
    rule table_heading_tag(ctx: &Context, delimiter: rule<()>) -> Vec<Spanned<Token>>
    = heading:spanned(<
        delimiter()
        attributes:(t:row_syntax_table_attrs(ctx)? { t.unwrap_or(vec![]) })
        { Token::TableHeading { attributes } }
      >)
      // It is not reasonably possible to encapsulate the table heading content
      // inside the heading token itself because wikitables can be nested, and
      // this grammar only parses table *parts*, so it does not have enough
      // state information to know whether a new table part encountered in
      // `nested_block_in_table` is a continuation of a child table or a
      // terminator for the current table part.
      content:(
          start:position!()
          t:nested_block_in_table(ctx)
          #{|input, pos| {
              // Parsoid did an even dirtier check to avoid matching on newlines
              // inside of a template, but since those are expanded on-demand in
              // this parser, any newline is a true newline.
              // TODO: since there has to be a rule to match the newline, it
              // seems like this flag clear could just be done in whatever rule
              // matches the newline instead of doing this dumb scan??
              if ctx.table_head.get() && input[start..pos].contains('\n') {
                  // There's been a newline. Remove the break and continue
                  // tokenizing nested_block_in_tables.
                  ctx.table_head.set(false);
              }
              RuleResult::Matched(pos, ())
          }}
          { t }
      )*
    { reduce_tree(iter::once(heading).chain(content.into_iter().flatten())) }

    /// Table data tags.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    ///                                                        ^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule table_data_tags(ctx: &Context) -> Vec<Spanned<Token>>
    = // avoid recursion via nested_block_in_table
      &assert(!ctx.table_data_block, "not in table data block")
      first:table_data_tag(ctx, <pipe() !['+'|'-']>)
      rest:table_data_tag(ctx, <pipe_pipe() {}>)*
    { reduce_tree(first.into_iter().chain(rest.into_iter().flatten())) }

    /// A single table data tag.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    ///                                                          ^^^^^^^^^^^    ^^
    /// ```
    rule table_data_tag(ctx: &Context, delimiter: rule<()>) -> Vec<Spanned<Token>>
    = data:spanned(<
        delimiter()
        !"}"
        attributes:(t:row_syntax_table_attrs(ctx)? { t.unwrap_or(vec![]) })
        { Token::TableData { attributes } }
      >)
      // It is not reasonably possible to encapsulate the table data content
      // inside the data token itself because wikitables can be nested, and
      // this grammar only parses table *parts*, so it does not have enough
      // state information to know whether a new table part encountered in
      // `nested_block_in_table` is a continuation of a child table or a
      // terminator for the current table part.
      // use `inline_breaks` to break on `tr`, etc.
      content:nested_block_in_table(ctx)*
    { reduce_tree(iter::once(data).chain(content.into_iter().flatten())) }

    /// Table end tag.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    ///                                                                            ^^
    /// ```
    rule table_end_tag() -> Spanned<Token>
    = spanned(<pipe() "}" { Token::TableEnd }>)

    /// Inline table attributes terminated by a single pipe.
    ///
    /// ```wikitext
    /// {| k="v" |+ c-k="v" | c |- r-k="v" ! h-k="v" | h !! h2 | d-k="v" | d || d2 |}
    ///             ^^^^^^^^^                ^^^^^^^^^           ^^^^^^^^^
    /// ```
    rule row_syntax_table_attrs(ctx: &Context) -> Vec<Spanned<Argument>>
    = attributes:table_attributes(&ctx.with_table_cell_attrs())
      space()*
      pipe() !pipe()
    { attributes }

    /// Pipe characters.
    rule pipe() = "|" / "{{!}}"

    /// Double-pipe characters.
    // SSS FIXME: what about |{{!}} and {{!}}|
    rule pipe_pipe() = "||" / "{{!}}{{!}}"

    /////////////////
    // Inline flow //
    /////////////////

    // Legend of symbols valid in inline contexts
    // '      quotes (italic/bold)
    // <      start of xmlish_tag
    // [ ]    links
    // { }    start of parser functions, transclusion and template params
    // \r \n  all sort of block-level markup at start of line
    // A-Za-z autolinks (http(s), nttp(s), mailto, ISBN, PMID, RFC)
    // _      behavior switches (e.g., '__NOTOC__')
    // ! |    table cell delimiters
    // =      headings
    // -      start of lang_variant -{ ... }-
    // :      separate definition in ; term : definition
    // ;      separator in lang_variant

    /// An inline item (tag, template, annotation, link, quote), plain text, or
    /// comment. These should be wrapped into paragraphs in block contexts.
    rule inlineline(ctx: &Context) -> Vec<Spanned<Token>>
    = t:(
        urltext(ctx)
      / !inline_breaks(ctx)
        t:inlineline_element(ctx)
        { t }
    )+
    { reduce_tree(t.into_iter().flatten()) }

    /// An inline element or plain text within an inline item.
    rule inlineline_element(ctx: &Context) -> Vec<Spanned<Token>>
    = inline_element(ctx)
    / !newline()
      t:spanned(<[_] { Token::Text }>)
      { vec![t] }

    /// A tag, template, language variant, link, or text style.
    rule inline_element(ctx: &Context) -> Vec<Spanned<Token>>
    = t:strip_marker() { vec![t] }
    / &"<" t:angle_bracket_markup(ctx) { t }
    / &"{" t:template_param_or_template(ctx) { t }
    / &"-{" t:lang_variant_or_tpl(ctx) { t }
    / t:spanned(<$("[[" &"[")+ { Token::Text }>) { vec![t] }
    / &"[" t:(wikilink(ctx) / t:extlink(ctx) { vec![t] }) { t }
    / &"'" t:quote() { t }

    /// A lookahead that matches if the input is at a terminator for whatever
    /// inline item is currently being parsed, according to `ctx`.
    rule inline_breaks(ctx: &Context)
    = // TODO: This precondition is just STOP_CHAR minus ' and <
      &['='|'|'|'!'|'{'|'}'|':'|';'|'\r'|'\n'|'['|']'|'-']
      #{|input, pos| inline_breaks(state, input, pos, ctx)}

    ///////////////////////////
    // Generic XML-like tags //
    ///////////////////////////

    /// A supported HTML5 start or end tag.
    rule html_tag(ctx: &Context) -> Spanned<Token>
    = xmlish_tag(&ctx.with_tag_kind(Some(TagKind::Html)))

    /// Any XML-tag-like item.
    ///
    /// `wellformed_extension_tag` is not used because `html_tag` is permissible
    /// and those can be imbalanced.
    rule angle_bracket_markup(ctx: &Context) -> Vec<Spanned<Token>>
    = t:annotation_tag(ctx) { vec![t] }
    / t:maybe_extension_tag(ctx) { vec![t] }
    / include_limits(ctx)
    / t:html_tag(ctx) { vec![t] }
    / t:comment() { vec![t] }

    /// An XML tag.
    ///
    /// ```wikitext
    /// <tag-name attr="value">content</tag-name>
    /// ^^^^^^^^^^^^^^^^^^^^^^^       ^^^^^^^^^^^
    /// ```
    #[cache]
    rule xmlish_tag(ctx: &Context) -> Spanned<Token>
    = spanned(<
      start:xmlish_start()
      &assert({
          let (name, _) = start;
          ctx.tag_kind != Some(TagKind::Html) || contains_ignore_case(&HTML5_TAGS, &name)
      }, "xml tag")

      // By the time we get to `doTableStuff` in the old parser, we've already
      // safely encoded element attributes. See 55313f4e in core.
      // `without_equal` is required to correctly parse tags in the
      // name-position of a template argument. Otherwise the tags get split at
      // the first attribute delimiter and turned into k-vs.
      attributes:generic_newline_attribute(&ctx
          .without_equal()
          .without_table()
          .without_table_cell_attrs()
      )*
      // No need to preserve this -- canonicalize on RT via dirty diff
      space_or_newline_or_solidus()*
      selfclose:"/"?
      // not preserved - canonicalized on RT via dirty diff
      space()*
      ">"
      {
          let (name, mut is_close) = start;

          let is_void = ctx.tag_kind == Some(TagKind::Html) && contains_ignore_case(&VOID_TAGS, &name);

          // Support </br>
          if name.eq_ignore_ascii_case("br") && is_close {
              is_close = false;
          }

          if is_close {
              Token::EndTag {
                  name: name.span,
              }
          } else {
              Token::StartTag {
                  name: name.span,
                  attributes,
                  self_closing: selfclose.is_some()
              }
          }
      }
    >)

    /// The first part of an XML-like tag.
    ///
    /// ```wikitext
    /// <tag-name attr="value">content</tag-name>
    /// ^^^^^^^^^                     ^^^^^^^^^^
    /// ```
    rule xmlish_start() -> (Spanned<&'input str>, bool)
    = "<" c:"/"? n:spanned(<$(tag_name())>) { (n, c.is_some()) }

    /// The tag name part of an XML-like tag.
    ///
    /// ```wikitext
    /// <tag-name attr="value">content</tag-name>
    ///  ^^^^^^^^                       ^^^^^^^^
    /// ```
    // See http://www.w3.org/TR/html5/syntax.html#tag-open-state and the
    // following paragraphs. We don't enforce ascii alpha for the first
    // character because extension tags are more permissive.
    rule tag_name() = [^'\t'|'\n'|'\x0b'|' '|'/'|'>'|'\0']+

    /// An XML tag attribute that can span multiple lines.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2=value2>content</tag-name>
    ///           ^^^^^^^^^^^^ ^^^^^^^^^^^^
    /// ```
    rule generic_newline_attribute(ctx: &Context) -> Spanned<Argument>
      // Keeping the space outside of the span makes it possible to serialise
      // essential whitespace more uniformly by adopting all content between
      // attributes, instead of having a range in the attribute span that is not
      // actually associated with anything in the attribute.
    = space_or_newline_or_solidus()*
      t:spanned(<
        name:attribute_name(ctx, <generic_attribute_name_piece(ctx)>)
        // For the same reason of serialisation, space should only be included
        // if there is actually a value
        value:(
          space_or_newline()*
          !inline_breaks(ctx)
          t:generic_attribute_value(ctx)
          { t }
        )?
        { make_attribute(name, value) }
    >)
    { t }

    /// Any tag attribute name which may contain directives.
    ///
    /// ```wikitext
    /// <tag-name attr-{{{kind}}}="value">content</tag-name>
    ///           ^^^^^^^^^^^^^^^
    /// {| attr="value" attr2=value2 ... |}
    ///    ^^^^         ^^^^^
    /// ```
    rule attribute_name(ctx: &Context, piece: rule<Vec<Spanned<Token>>>) -> Vec<Spanned<Token>>
    = first:(
        // The arrangement of chars is to emphasize the split between what's
        // disallowed by HTML5 and what's necessary to give `directive` a chance.
        // See: http://www.w3.org/TR/html5/syntax.html#attributes-0
        // From #before-attribute-name-state, < is omitted for directive
        t:spanned(<['"'|'\''|'='] { Token::Text }>) { vec![t] }
        / piece()
      )
      rest:piece()*
    { reduce_tree(iter::once(first).chain(rest.into_iter()).flatten()) }

    /// A piece of an attribute name inside an XML tag.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2=value2>content</tag-name>
    ///           ^^^^         ^^^^^
    /// ```
    rule generic_attribute_name_piece(ctx: &Context) -> Vec<Spanned<Token>>
    = t:spanned(<attribute_text()+ { Token::Text }>) { vec![t] }
    / !inline_breaks(ctx)
      t:(
        directive(ctx)
        / t:spanned(<less_than(ctx) { Token::Text }>) { vec![t] }
            // "\0/=>" is the html5 attribute name set we do not want.
            / t:spanned(<!(space_or_newline() / ['\0'|'/'|'='|'>'|'<']) [_] { Token::Text }>) { vec![t] }
      )
      { reduce_tree(t) }

    /// Table attributes.
    ///
    /// ```wikitext
    /// {| attr="value" attr2=value2 ... |}
    ///    ^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule table_attributes(ctx: &Context) -> Vec<Spanned<Argument>>
    = t:(
          table_attribute(ctx)
        / space()*
          t:spanned(<
            broken_table_attribute_name_char()
            { Argument { content: vec![], delimiter: None, terminator: None } }
          >)
          { t }
    )*

    /// An XML attribute which may not span multiple lines.
    ///
    /// ```wikitext
    /// {| attr="value" attr2=value2 ... |}
    ///    ^^^^^^^^^^^^ ^^^^^^^^^^^^
    /// ```
    rule table_attribute(ctx: &Context) -> Spanned<Argument>
      // Keeping the space outside of the span makes it possible to serialise
      // essential whitespace more uniformly by adopting all content between
      // attributes, instead of having a range in the attribute span that is not
      // actually associated with anything in the attribute.
    = space()*
      t:spanned(<
        name:attribute_name(ctx, <table_attribute_name_piece(ctx)>)
        // For the same reason of serialisation, space should only be included
        // if there is actually a value
        value:(space()* t:table_attribute_value(ctx) { t })?
      { make_attribute(name, value) }
    >)
    { t }

    // The old parser's Sanitizer::removeHTMLtags explodes on < so that it can't
    // be found anywhere in xmlish tags.  This is a divergence from html5 tokenizing
    // which happily permits it in attribute positions.  Extension tags being the
    // exception, since they're stripped beforehand.
    rule less_than(ctx: &Context)
    = !html_or_empty(ctx) "<"

    /// Characters valid in the unquoted plain text of a Wikitext table or table
    /// row attribute.
    rule broken_table_attribute_name_char()
    = ['\0'|'/'|'='|'>']

    /// A piece of an attribute name inside a Wikitext table.
    ///
    /// ```wikitext
    /// {| attr="value" attr2="value2" ... |}
    ///    ^^^^         ^^^^^
    /// ```
    ///
    /// This is the same as `generic_attribute_name_piece`, except tags and
    /// wikilinks are accepted. (That doesn't make sense (i.e. match the old
    /// parser) in the generic case.) We also give a chance to break on \[
    /// (see T2553).
    rule table_attribute_name_piece(ctx: &Context) -> Vec<Spanned<Token>>
    = t:spanned(<(!"[" attribute_text())+ { Token::Text }>) { vec![t] }
    / !inline_breaks(ctx)
      // \0/=> is the html5 attribute name set we do not want.
      t:(
          t:spanned(<wikilink(ctx) { Token::Text }>) { vec![t] }
        / directive(ctx)
          // Wikitext is trash and obliges us to accept HTML tags in the table
          // attribute list. Parsoid takes an entire XML-ish tag as an attribute
          // name and then does postprocessing to extract the attributes inside
          // the tag, which seems excessive when the whole point is just that a
          // braindead parser once treated an HTML tag like an attribute name,
          // and we can fix it by just doing the exact same thing and then
          // emitting a nothing-token.
        / x:spanned(<html_tag_name() { Token::Generated(String::new()) }>) { vec![x] }
        / t:spanned(<!(space_or_newline() / ['\0'|'/'|'='|'>']) [_] { Token::Text }>)
          { vec![t] }
      )
      { t }

    /// An HTML start tag squatting illegally inside a Wikitext attribute part.
    ///
    /// ```wikitext
    /// {| attr="value" <span attr2="value2" ... |}
    ///                 ^^^^^
    /// ```
    rule html_tag_name()
    = start:xmlish_start()
      &assert({ contains_ignore_case(&HTML5_TAGS, &start.0) }, "html tag")

    /// An attribute value inside an XML tag.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2=value>content</tag-name>
    ///               ^^^^^^^       ^^^^^^
    /// ```
    ///
    /// Quoted values can span multiple lines. Quoted values without a closing
    /// quote will also terminate when the tag closes:
    ///
    /// ```wikitext
    /// <tag-name attr="value>content</tag-name>
    ///               ^^^^^^^
    /// <tag-name attr="value/>content</tag-name>
    ///               ^^^^^^^
    /// ```
    rule generic_attribute_value(ctx: &Context) -> AttributeValue
    = start:spanned(<
        "="
        space_or_newline()*
        t:['\''|'"'] { t }
      >)
      value:generic_attribute_value_text(ctx, <![c if c == start.node]>)?
      end:(
          t:spanned(<[c if c == start.node] { Token::Text }>)
          { Some(t) }
        / &("/"? ">")
          { None }
      )
      { (start.map_node(|_| Token::Text), value.unwrap_or(vec![]), end) }
    / start:spanned(<
        "="
        space_or_newline()*
        { Token::Text }
      >)
      value:generic_attribute_value_text(ctx, <![' '|'\t'|'\n'|'\r'|'\x0c']>)
      &(space_or_newline() / eof() / "/"? ">")
      { (start, value, None) }

    /// An attribute value inside a Wikitext table.
    ///
    /// ```wikitext
    /// {| attr="value" attr2=value2 ... |}
    ///         ^^^^^^^       ^^^^^^
    /// ```
    ///
    /// All values are restricted to a single line. Quoted values without a
    /// closing quote will terminate at `|`, `!!`, or a newline:
    ///
    /// ```wikitext
    /// {| attr="value| ... |}
    ///          ^^^^^
    /// {| attr="value!! ... |}
    ///          ^^^^^
    /// ```
    rule table_attribute_value(ctx: &Context) -> AttributeValue
    = start:spanned(<
        "="
        space()*
        t:['\''|'"'] { t }
      >)
      value:table_attribute_value_text(ctx, <![c if c == start.node]>)?
      end:(
          t:spanned(<[c if c == start.node] { Token::Text }>)
          { Some(t) }
        / &("!!" / ['|'|'\r'|'\n'])
          { None }
      )
      { (start.map_node(|_| Token::Text), value.unwrap_or(vec![]), end) }
    / start:spanned(<
        "="
        space()*
        { Token::Text }
      >)
      value:table_attribute_value_text(ctx, <![' '|'\t'|'\x0c']>)
      &(space_or_newline() / eof() / "!!" / "|")
      { (start, value, None) }

    /// Attribute value content inside an XML tag.
    ///
    /// `/` is a permissible char. We only break on `/>`, enforced by the
    /// negated expression, so it isn't included in the stop set.
    ///
    /// (In parsoid: attribute_preprocessor_text{,_single,_double})
    rule generic_attribute_value_text(ctx: &Context, stop: rule<()>) -> Vec<Spanned<Token>>
    = t:(
        t:spanned(<(stop() [^'\x7f'|'{'|'}'|'&'|'<'|'-'|'|'|'/'|'>'])+ { Token::Text }>) { vec![t] }
        /
        !inline_breaks(ctx)
        !"/>"
        t:(
            directive(ctx)
            / t:spanned(<(less_than(ctx) / ['{'|'}'|'&'|'-'|'|'|'/']) { Token::Text }>) { vec![t] }
        )
        { t }
    )*
    { reduce_tree(t.into_iter().flatten()) }

    /// Attribute value content inside a Wikitext table.
    ///
    /// `!` is a permissible char. We only break on `!!` in `th`, enforced by
    /// the `inline_break`, so it isn't included in the stop set.
    /// `[` is also permissible but we give a chance to break for the `[[`
    /// special case in the old parser (See T2553).
    ///
    /// (In parsoid: table_attribute_preprocessor_text{,_single,_double})
    rule table_attribute_value_text<T>(ctx: &Context, stop: rule<T>) -> Vec<Spanned<Token>>
    = t:(
        t:spanned(<(stop() [^'\x7f'|'{'|'}'|'&'|'<'|'-'|'!'|'['|'\r'|'\n'|'|'])+ { Token::Text }>) { vec![t] }
        /
        !inline_breaks(ctx)
        t:(
            directive(ctx)
            / t:spanned(<['{'|'}'|'&'|'<'|'-'|'!'|'['] { Token::Text }>) { vec![t] }
        ) { t }
    )*
    { reduce_tree(t.into_iter().flatten()) }

    /// Characters valid in the unquoted plain text of an XML attribute.
    rule attribute_text()
    = [^'\x7f'|' '|'\t'|'\r'|'\n'|'\0'|'/'|'='|'>'|'<'|'&'|'{'|'}'|'-'|'!'|'|']

    /// This rule is used in carefully crafted places of xmlish tag tokenizing
    /// with the inclusion of solidus to match where the spec would ignore those
    /// characters. In particular, it does not belong in between attribute name
    /// and value.
    rule space_or_newline_or_solidus()
    = space_or_newline() / ("/" !">")

    ///////////////////////
    // Inclusion control //
    ///////////////////////

    /// A lookahead that matches if the parser is in a valid state for parsing
    /// an inclusion control tag.
    rule include_check(ctx: &Context)
    = &html_or_empty(ctx)
      start:xmlish_start()
      &assert(
        contains_ignore_case(&INCLUDE_TAGS, &start.0),
        "inclusion control tag"
      )

    /// A `noinclude`, `includeonly`, or `onlyinclude` tag.
    ///
    /// These are normally handled by the `xmlish_tag` rule, except where
    /// generic tags are not allowed (e.g. inside a `directive`). For example:
    ///
    /// ```wikitext
    /// {|
    /// |-<includeonly>
    /// foo
    /// </includeonly>
    /// |Hello
    /// |}
    /// ```
    ///
    /// The behaviour of `<noinclude>` and `<includeonly>` is self-explanatory.
    /// Use the inner contents only when not transcluded, and vice-versa.
    ///
    /// `<onlyinclude>` is an insane thing. Content inside `<onlyinclude>`
    /// appears regardless of whether the page is transcluded or not, and when
    /// the page *is* transcluded, anything outside of `<onlyinclude>` is
    /// treated as if it were wrapped by `<noinclude>` (including any
    /// `<includeonly>` tags).
    ///
    /// It is impossible to parse a Wikitext document correctly without cleaving
    /// out the includes immediately because the position of these tokens is
    /// unrestricted in the Wikitext source and so it is legal to, for example,
    /// define different numbers of arguments for a template depending on
    /// whether or not the document is being transcluded.
    rule include_limits(ctx: &Context) -> Vec<Spanned<Token>>
    = &include_check(ctx)
      t:xmlish_tag(&ctx.with_tag_kind(Some(TagKind::Inclusion)))
      t:#{|input, pos| {
        let (name, is_end) = match t.node {
            Token::StartTag { name, self_closing, .. } => (name, self_closing),
            Token::EndTag { name } => (name, true),
            _=> unreachable!("impossible include tag")
        };
        let name = &input[name.into_range()];
        let content = if is_end {
            None
        } else {
            find_end_tag(&input[t.span.end..], name)
        };

        let mode = if name.eq_ignore_ascii_case("includeonly") {
            InclusionMode::IncludeOnly
        } else if name.eq_ignore_ascii_case("noinclude") {
            InclusionMode::NoInclude
        } else if name.eq_ignore_ascii_case("onlyinclude") {
            globals.has_onlyinclude.set(true);
            InclusionMode::OnlyInclude
        } else {
            unreachable!()
        };

        if globals.including {
            match mode {
                InclusionMode::IncludeOnly => {
                    // T353697: `<pre<includeonly></includeonly>` in a template
                    // should be equivalent to emitting an HTML `<pre>` at the
                    // output stage.
                    let pre_hack = content.is_some_and(|_| {
                        input[..t.span.start].ends_with("<pre")
                    });

                    // Discard the tag, parse the content
                    RuleResult::Matched(pos, if pre_hack {
                        // …unless it’s the pre-hack
                        vec![t.map_node(|_| {
                            Token::Generated(" format=\"wikitext\"".into())
                        })]
                    } else {
                        vec![]
                    })
                }
                InclusionMode::NoInclude => {
                    // Discard the tag, skip the content.
                    RuleResult::Matched(
                        pos + content.map_or_else(
                            || if is_end { 0 } else { input.len() - pos },
                            |(_, e)| e
                        ),
                        vec![]
                    )
                }
                InclusionMode::OnlyInclude => {
                    let tag = t.map_node(|_| {
                        if is_end {
                            Token::EndInclude(mode)
                        } else {
                            Token::StartInclude(mode)
                        }
                    });

                    // The content inside `<onlyinclude>` needs to be parsed
                    // separately because it should be as if none of the earlier
                    // content ever existed at all, but the parser is already in
                    // the middle of some context that belongs to that other
                    // content.
                    if let Some((content_len, end_pos)) = content
                        // To make sure that the span positions are correct,
                        // reparsing happens with a special rule which just
                        // skips earlier content and then continues normally.
                        && let Ok(inner) = only_include(
                            &input[..pos + content_len], state, globals, pos
                        )
                    {
                        RuleResult::Matched(
                            pos + content_len,
                            iter::once(tag).chain(inner).collect()
                        )
                    } else {
                        RuleResult::Matched(pos, vec![tag])
                    }
                }
            }
        } else {
            match mode {
                InclusionMode::IncludeOnly => {
                    if is_end && mode == InclusionMode::IncludeOnly {
                        // Compatibility with the legacy parser
                        RuleResult::Matched(pos, vec![t.map_node(|_| {
                            Token::Text
                        })])
                    } else {
                        // Discard the tag, skip the content
                        RuleResult::Matched(
                            pos + content.map_or_else(
                                ||if is_end { 0 } else { input.len() - pos },
                                |(_, e)| e
                            ),
                            vec![]
                        )
                    }
                }
                InclusionMode::OnlyInclude | InclusionMode::NoInclude => {
                    // Discard the tag, parse the content
                    RuleResult::Matched(pos, vec![])
                }
            }
        }
    }}
    { t }

    /// The content of an `<onlyinclude>` tag.
    ///
    /// ```wikitext
    /// <onlyinclude>inner {{content}}</onlyinclude>
    ///              ^^^^^^^^^^^^^^^^^
    /// ```
    pub rule only_include(start_at: usize) -> Vec<Spanned<Token>>
    = #{|input, pos| RuleResult::Matched(start_at, ()) }
      t:start()
    { t }

    ////////////////////
    // Extension tags //
    ////////////////////

    /// A lookahead that matches if the parser is in a valid state for parsing
    /// an HTML or extension tag.
    rule html_or_empty(ctx: &Context)
    = assert(matches!(ctx.tag_kind, Some(TagKind::Html) | None), "html or empty")

    /// A lookahead that matches if the parser is in a valid state for parsing
    /// an extension tag.
    rule extension_check(ctx: &Context)
    = &html_or_empty(ctx)
      start:xmlish_start()
      &assert({
        let (name, _) = start;
        // MW used Unicode case folding here, but why? All these are ASCII.
        name.node == "wiki-rs" || (
            contains_ignore_case(&state.config.extension_tags, &name)
            && !contains_ignore_case(&state.config.annotation_tags, &name)
        )
      }, "extension tag")

    /// An extension tag. The entire tag and its contents are consumed at once.
    ///
    /// ```wikitext
    /// <extension-tag>Value</extension-tag>
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    ///
    /// If the extension tag is not syntactically valid, text will be produced
    /// instead:
    ///
    /// ```wikitext
    /// <missing-end>No end tag
    /// ^^^^^^^^^^^^^
    /// No start tag</missing-start>
    ///             ^^^^^^^^^^^^^^^^
    ///
    /// Extension tags should be parsed with higher priority than anything else.
    /// Their content is not immediately parsed. This ensures that any malformed
    /// Wikitext inside the tag is isolated, and that things which are not
    /// Wikitext are never misinterpreted as Wikitext (e.g.
    /// `<nowiki>{{foo}}</nowiki>`, `<math>\frac\{foo\frac{bar}}</math>`).
    /// ```
    rule maybe_extension_tag(ctx: &Context) -> Spanned<Token>
    = &extension_check(ctx)
      t:xmlish_tag(&ctx.with_tag_kind(Some(TagKind::Extension)))
      t:#{|input, pos| {
        match t.node {
            Token::StartTag { name, attributes, self_closing } if self_closing => {
                RuleResult::Matched(pos, Spanned {
                    node: Token::Extension { name, attributes, content: None },
                    span: t.span,
                })
            },
            Token::StartTag { name, attributes, self_closing } => {
                let Some((content_len, end_pos)) = find_end_tag(
                    &input[pos..], &input[name.into_range()]
                ) else {
                    // This is undefined behaviour. The old parser returns text
                    // here (see core commit 674e8388cba).
                    return RuleResult::Matched(pos, Spanned {
                        node: Token::Text,
                        span: t.span,
                    });
                };

                let after_end = pos + end_pos;

                // TODO:
                // if state.in_template {
                    // Support nesting in extensions tags while tokenizing in templates
                    // to support the #tag parser function.
                    //
                    // It's necessary to permit this broadly in templates because
                    // there's no way to distinguish whether the nesting happened
                    // while expanding the #tag parser function, or just a general
                    // syntax errors.  In other words,
                    //
                    //   hi<ref>ho<ref>hi</ref>ho</ref>
                    //
                    // and
                    //
                    //   hi{{#tag:ref|ho<ref>hi</ref>ho}}
                    //
                    // found in template are returned indistinguishably after a
                    // preprocessing request, though the old parser renders them
                    // differently.  #tag in template is probably a common enough
                    // use case that we want to accept these false positives,
                    // though another approach could be to drop this code here, and
                    // invoke a native #tag handler and forgo those in templates.
                    //
                    // Expand `extSrc` as long as there is a <tagName> found in the
                    // extension source body.
                    // $startTagRE = '~<' . preg_quote( $tagName, '~' ) . '(?:[^/>]|/(?!>))*>~i';
                    // $s = substr( $extSrc, $dp->tsr->end - $dp->tsr->start );
                    // $openTags = 0;
                    // while ( true ) {
                    //     if ( preg_match_all( $startTagRE, $s, $matches ) ) {
                    //         $openTags += count( $matches[0] );
                    //     }
                    //     if ( !$openTags ) {
                    //         break;
                    //     }
                    //     if ( !preg_match( $endTagRE, $this->input, $tagContent, 0, $extEndOffset ) ) {
                    //         break;
                    //     }
                    //     $openTags -= 1;
                    //     $s = $tagContent[0];
                    //     $extEndOffset += strlen( $s );
                    //     $extEndTagWidth = strlen( $tagContent[1] );
                    //     $extSrc .= $s;
                    // }
                // }

                RuleResult::Matched(after_end, Spanned::new(Token::Extension {
                    name,
                    attributes,
                    content: Some(Span::new(t.span.end, t.span.end + content_len))
                }, t.span.start, after_end))
            },

            Token::EndTag { .. } => {
                // This production is impossible unless the start tag was
                // missing or invalid
                RuleResult::Matched(pos, t.map_node(|_| Token::Text))
            },
            _ => unreachable!("got a non-tag token from xmlish_tag")
        }
    }} { t }

    /// An extension tag. The entire tag and its contents are consumed at once.
    ///
    /// ```wikitext
    /// <extension-tag>Value</extension-tag>
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    ///
    /// If the extension tag is not syntactically valid, this production will be
    /// rejected.
    rule wellformed_extension_tag(ctx: &Context) -> Spanned<Token>
    = t:maybe_extension_tag(ctx)
      &assert(matches!(t.node, Token::Extension { .. }), "well-formed extension tag")
    { t }

    /// A `<nowiki>` tag.
    rule nowiki(ctx: &Context) -> Spanned<Token>
    = &nowiki_check(ctx)
      t:wellformed_extension_tag(ctx)
    { t }

    /// A lookahead that matches if the parser is in a valid state for parsing
    /// a `<nowiki>` tag.
    rule nowiki_check(ctx: &Context)
    = &html_or_empty(ctx)
      start:xmlish_start()
      &assert(start.0.eq_ignore_ascii_case("nowiki"), "nowiki")

    /// The contents of a `<nowiki>` tag.
    ///
    /// Used by lang_variant productions to protect special language names or
    /// conversion strings.
    rule nowiki_text(ctx: &Context) -> Spanned<Token>
    = t:nowiki(ctx)
    { todo!("unwrap content inside inner tags and decode entities to plain text") }

    /// An extension tag which has been replaced by a strip marker.
    rule strip_marker() -> Spanned<Token>
    = spanned(<
      const_value(MARKER_PREFIX) id:spanned(<(!const_value(MARKER_SUFFIX) [_])+>) const_value(MARKER_SUFFIX)
      { Token::StripMarker(id.span) }
    >)

    /////////////////
    // Annotations //
    /////////////////

    /// An old-style translation variable close tag.
    ///
    /// ```wikitext
    /// <tvar|id>Value</>
    ///               ^^^
    /// ```
    // FIXME: Temporary (?) hack to let us not horribly break on old tvar syntax
    // In coordination with language team, get rid of this hack once all old uses
    // are migrated to new syntax (T274881).
    rule tvar_old_syntax_closing_HACK(ctx: &Context) -> Spanned<Token>
    = &assert(ctx.tag_kind != Some(TagKind::Annotation), "non-annotation tag")
      t:spanned(<"</>" { Token::EndAnnotation { name: either::Left("tvar") } }>)
      &assert({
          state.config.annotations_enabled && state.config.annotation_tags.contains("tvar")
      }, "tvar")
    { t }

    /// A lookahead that matches if the parser is in a valid state for parsing
    /// an annotation tag.
    rule annotation_check(ctx: &Context)
    = &assert(ctx.tag_kind != Some(TagKind::Annotation), "non-annotation tag")
      start:xmlish_start()
      &assert(state.config.annotation_tags.contains(&start.0), "annotation tag")

    /// An annotation tag.
    ///
    /// ```wikitext
    /// <tvar|id>Value</tvar>
    /// ^^^^^^^^^     ^^^^^^^
    /// <tvar id="id">Value</tvar>
    /// ^^^^^^^^^^^^^^     ^^^^^^^
    /// ```
    rule annotation_tag(ctx: &Context) -> Spanned<Token>
    = &assert(state.config.annotations_enabled, "enabled annotations")
      t:(
        tvar_old_syntax_closing_HACK(ctx)
        / (
            &annotation_check(ctx)
            t:xmlish_tag(&ctx.with_tag_kind(Some(TagKind::Annotation)))
            t:#{|input, pos| {
                let (name, mut attributes, is_end) = match t.node {
                    Token::StartTag { name, attributes, self_closing } => (name, attributes, self_closing),
                    Token::EndTag { name } => (name, vec![], true),
                    _ => unreachable!()
                };

                let name_str = &input[name.into_range()];

                let (tag_name, attributes) = if !contains_ignore_case(&state.config.annotation_tags, name_str)
                    && let Some((tag_name, name_attr)) = name_str.split_once('|')
                {
                    let name = Spanned {
                        node: AnnoAttribute {
                            name: either::Left("name"),
                            value: Some(Span::new(name.end - name_attr.len(), name.end))
                        },
                        span: name,
                    };

                    (tag_name, vec![name])
                } else {
                    let attributes = attributes.into_iter().map(|Spanned { node: argument @ Argument { .. }, span }| {
                        let name = argument.name().unwrap_or_default();
                        // If the key or the value is not a string,
                        // we replace it by the thing that generated it and
                        // consider that Wikitext as a raw string instead.
                        // Possible follow-up in T295168 for attribute sanitation
                        let name = if let [Spanned { node: Token::Text, span }] = name {
                            *span
                        } else if let (Some(first), Some(last)) = (name.first(), name.last()) {
                            first.span.merge(last.span)
                        } else {
                            span
                        };

                        let value = argument.value();
                        let value = (!value.is_empty()).then(|| {
                            if let [Spanned { node: Token::Text, span }] = value {
                                *span
                            } else if let (Some(first), Some(last)) = (value.first(), value.last()) {
                                first.span.merge(last.span)
                            } else {
                                span
                            }
                        });

                        Spanned {
                            node: AnnoAttribute { name: either::Right(name), value },
                            span,
                        }
                    }).collect::<Vec<_>>();

                    (name_str, attributes)
                };

                let node = if is_end {
                    Token::EndAnnotation { name: either::Right(name) }
                } else {
                    Token::StartAnnotation { name, attributes }
                };

                // TODO: state.has_annotations.set(true);
                RuleResult::Matched(pos, Spanned {
                    node,
                    span: t.span,
                })
            }} { t }
        )
      )
    { t }

    ///////////////
    // Templates //
    ///////////////

    /// A template or template parameter substitution.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    ///
    /// {{{parameter_name|default}}}
    /// ```
    ///
    /// Precedence: template parameters win over templates. See
    /// http://www.mediawiki.org/wiki/Preprocessor_ABNF#Ideal_precedence
    /// 4:    {{{{·}}}}    →     {·{{{·}}}·}
    /// 5:   {{{{{·}}}}}   →    {{·{{{·}}}·}}
    /// 6:  {{{{{{·}}}}}}  →   {{{·{{{·}}}·}}}
    /// 7: {{{{{{{·}}}}}}} → {·{{{·{{{·}}}·}}}·}
    /// This is only if close has > 3 braces; otherwise we just match open
    /// and close as we find them.
    rule template_param_or_template(ctx: &Context) -> Vec<Spanned<Token>>
    = &"{{"
      t:(
          &assert(ctx.after_expansion, "after expansion") t:spanned(<"{{" "{"? { Token::Text }>)
            { vec![t] }
          / &("{{" &("{{{"+ !"{") template_param(ctx)) t:(template(ctx) / broken_template(ctx))
            { vec![t] }
          / p:spanned(<"{" &("{{{"+ !"{") { Token::Text }>)? t:template_param(ctx)
            { p.into_iter().chain(iter::once(t)).collect() }
          / p:spanned(<"{" &("{{" !"{") { Token::Text }>)? t:template(ctx)
            { p.into_iter().chain(iter::once(t)).collect() }
          / t:broken_template(ctx)
            { vec![t] }
      )
    { t }

    /// A template substitution.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    /// ```
    #[cache]
    rule template(ctx: &Context) -> Spanned<Token>
    = ctx:({ ctx.with_prod_kind(Some(ProdKind::Template)) })
      t:spanned(<
        "{{"
        nl_comment_space()*
        target:template_target(&ctx)
        arguments:(
          nl_comment_space()*
          "|"
          t:(template_arg_empty() / template_arg(&ctx))
          { t }
        )*
        nl_comment_space()*
        inline_breaks(&ctx)
        "}}"
        { Token::Template { target, arguments } }
        / "{{" space_or_newline()* "}}" { Token::Text }
    >)
    { t }

    /// A minimal parser for template arguments. Used for debugging.
    pub rule debug_template_args() -> Vec<Spanned<Argument>>
    = ctx:({ Context::default().with_prod_kind(Some(ProdKind::Template)) })
      t:(template_arg_empty() / template_arg(&ctx)) ** (nl_comment_space()* "|")
    { t }

    /// An empty template argument.
    ///
    /// ```wikitext
    /// {{Template name| |key=value}}
    ///                 ^
    /// ```
    rule template_arg_empty() -> Spanned<Argument>
    = t:spanned(<nl_comment_space()*>)
      &("|" / "}}")
    { t.map_node(|t| Argument {
        content: reduce_tree(t),
        delimiter: None,
        terminator: None,
    }) }

    /// A template argument.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    ///                 ^^^^^^^^^^^^^^^^^ ^^^^^^^^^
    /// ```
    ///
    /// (In Parsoid, terms for arguments and parameters were reversed.)
    rule template_arg(ctx: &Context) -> Spanned<Argument>
    = spanned(<
        name:template_arg_name(ctx)
        dv:(
            delimiter:spanned(<
                // Any whitespace before the delimiter will have been consumed
                // by `template_arg_name`
                "="
                space()*
                { Token::Text }
            >)
            value:template_arg_value(ctx)?
            { (delimiter, value) }
        )?
        {
            if let Some((delimiter, value)) = dv {
                let name_len = name.len();
                let content = name.into_iter()
                    .chain(iter::once(delimiter))
                    .chain(value.into_iter().flatten())
                    .collect();
                Argument { content, delimiter: Some(name_len), terminator: None }
            } else {
                Argument { content: name, delimiter: None, terminator: None }
            }
        }
      / &['|'|'}']
        { Argument { content: vec![], delimiter: None, terminator: None } }
    >)

    /// A template argument name (or value, if there is no `=`).
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    ///                 ^^^^^^^^^^^^^^^^^ ^^^
    /// ```
    rule template_arg_name(ctx: &Context) -> Vec<Spanned<Token>>
    = template_arg_text(&ctx.with_equal()) / &"=" { vec![] }

    /// A template with no terminator.
    ///
    /// ```wikitext
    /// {{Template name
    /// ```
    ///
    /// The original Wikitext parser did not backtrack for mismatched pairs;
    /// once it saw `[[ {{` it would only ever look for `}}`. This means the
    /// obvious rule for parsing templates…
    ///
    /// ```peg
    /// rule template()
    ///   = "{{" t:template_content() "}}" { Token::Template(t) }
    ///   / "{{" { Token::Text }
    /// ```
    ///
    /// …does not work, because for the input "[[ {{ ]]" it would produce
    /// `Link { content: Text("{{") }` while the “correct” production is
    /// actually `Text("[[ {{ ]]")`.
    ///
    /// To address this, whenever a mismatched pair is encountered, the fallback
    /// subexpression (`rule broken_<whatever>`) consumes the opening delimiter
    /// and then *clears* `ctx.prod_kind`. This causes whichever terminator
    /// *had* been set by the parent rule to no longer match in `inline_breaks`.
    /// This causes the child to consume it as text instead of stopping at the
    /// terminator, which makes the parent rule fail and match its fallback
    /// subexpression, and on and on, until all the previous unclosed delimiters
    /// are consumed as text.
    ///
    /// So, again using the example "[[ {{ ]]", the order of operation becomes:
    ///
    /// 1. wikilink,        template,              "]]", *ERROR* (no "}}")
    ///    Some(Link)       Some(Template)
    /// 2. wikilink,        broken_template,    "{{ ]]", *ERROR* (no "]]")
    ///    Some(Link)       None
    /// 3. broken_wikilink, broken_template, "[[ {{ ]]", *OK*
    ///    None             None
    ///
    // TODO: As an additional optimisation for pathological cases, it is known
    // that if a broken expression of a given `ProdKind` is encountered, every
    // other expression of that same `ProdKind` will also never match, and so a
    // single global set of failed `ProdKind`s could be stored and used as a
    // precondition. The legacy parser does this, whereas this parser relies on
    // packrat caching, which is slower and uses much more memory (since instead
    // of just setting a single bit at eof and checking the same bit, it has to
    // store a `RuleResult` for every position in the input).
    rule broken_template(ctx: &Context) -> Spanned<Token>
    = spanned(<"{{" { ctx.prod_kind.set(None); Token::Text }>)

    /// A template parameter.
    ///
    /// ```wikitext
    /// {{{parameter_name|default}}}
    /// ```
    #[cache]
    rule template_param(ctx: &Context) -> Spanned<Token>
    = ctx:({ ctx.with_prod_kind(Some(ProdKind::Template)) })
      t:spanned(<
        "{{{"
        nl_comment_space()*
        name:(t:template_target(&ctx)? { t.unwrap_or(vec![]) })
        arguments:(
            nl_comment_space()* "|"
            t:(
                t:nl_comment_space()*
                &("|" / "}}}")
                { reduce_tree(t) }
                / template_arg_value(&ctx)
            ) { t }
        )*
        nl_comment_space()*
        inline_breaks(&ctx)
        "}}}"
        { Token::Parameter {
            name,
            default: arguments.into_iter().next()
        } }
    >)
    { t }

    /// A template or template parameter argument value which consumes K-Vs as
    /// a single run of text.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    ///                 ^^^^^^^^^^^^^^^^^ ^^^^^^^^^
    ///
    /// {{{parameter_name|default}}}
    ///                   ^^^^^^^
    /// ```
    rule template_arg_value(ctx: &Context) -> Vec<Spanned<Token>>
    = template_arg_text(&ctx.without_equal())

    /// A template or template parameter argument value.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    ///                 ^^^^^^^^^^^^^^^^^     ^^^^^
    ///
    /// {{{parameter_name|default}}}
    ///                   ^^^^^^^
    /// ```
    rule template_arg_text(ctx: &Context) -> Vec<Spanned<Token>>
    = t:(
        nested_block(
            &ctx.without_table()
                .without_extlink()
                .with_template_arg()
                .without_table_cell_attrs()
        )
        / t:newline_token() { vec![t] }
    )+
    { reduce_tree(t.into_iter().flatten()) }

    /// The first segment of a template or template parameter.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    ///   ^^^^^^^^^^^^^
    ///
    /// {{{parameter_name|default}}}
    ///    ^^^^^^^^^^^^^^
    /// ```
    ///
    /// (In parsoid: `inlineline_in_tpls`, `nested_inlineline`)
    rule template_target(ctx: &Context) -> Vec<Spanned<Token>>
    = ctx:({
        ctx.without_equal()
            .without_table()
            .without_extlink()
            .with_template_arg()
            .without_table_cell_attrs()
      })
      t:(
        t:(
          !inline_breaks(&ctx)
          t:inlineline(&ctx) { t }
        ) { t }
        / t:newline_token()
          { vec![t] }
      )+
    { reduce_tree(t.into_iter().flatten()) }

    /// A context-sensitive newline, whitespace, or comment.
    rule nl_comment_space() -> Spanned<Token>
    = newline_token()
    / space_or_comment()

    ///////////////////////
    // LanguageConverter //
    ///////////////////////

    // It is weirdly hard to find the documentation on this feature. It is here:
    // <https://www.mediawiki.org/wiki/Special:MyLanguage/Writing_systems/Syntax>

    /// A language conversion, template, or template parameter.
    ///
    /// Note that "rightmost opening" precedence rule (see
    /// https://www.mediawiki.org/wiki/Preprocessor_ABNF) means that neither
    /// `-{{` nor `-{{{` are parsed as a `-{` token, but `-{{{{` is, since `{{{`
    /// has precedence over `{{`.
    rule lang_variant_or_tpl(ctx: &Context) -> Vec<Spanned<Token>>
    = &("-{" &("{{{"+ !"{") template_param(ctx)) t:lang_variant(ctx)
      { vec![t] }
    / s:spanned(<"-" &("{{{"+ !"{") { Token::Text }>) t:template_param(ctx)
      { vec![s, t] }
    / s:spanned(<"-" &("{{" "{{{"* !"{") { Token::Text }>) t:template(ctx)
      { vec![s, t] }
    / &"-{" t:lang_variant(ctx)
      { vec![t] }

    /// A language conversion with no terminator.
    rule broken_lang_variant(ctx: &Context) -> Spanned<Token>
    = t:spanned(<"-{" { ctx.prod_kind.set(None); Token::Text }>)

    /// A language conversion.
    ///
    /// ```wikitext
    /// -{ text }-
    ///
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    /// ```
    #[cache]
    rule lang_variant(ctx: &Context) -> Spanned<Token>
    = // FIXME: Maybe this should suppress `table` and `table_cell_attr` like
      // `template_arg_text` does?
      lang_variant_preproc(&ctx.with_prod_kind(Some(ProdKind::Lang)).without_extlink())
    / broken_lang_variant(ctx)

    /// A language conversion.
    // TODO: Very little about this makes sense. Either the flags are discarded
    // or the language variants are discarded, and it converts into hash sets
    // only to turn them right back into vectors again, and it discards these
    // intermediates in favour of KVs.
    rule lang_variant_preproc(ctx: &Context) -> Spanned<Token>
    = spanned(<
      "-{"
      f:lang_variant_preproc_flags(ctx)
      variants:lang_variant_preproc_variants(ctx, f.1)
      inline_breaks(ctx)
      "}-"
      {
          let (flags, raw) = f;
          if state.config.language_conversion_enabled {
              Token::LangVariant { flags, variants, raw }
          } else {
              Token::Text
          }
      }
    >)

    /// Produces processed language conversion flags if the language converter is
    /// enabled.
    ///
    /// ```wikitext
    /// -{ flag1 ; flag2 | ... }-
    ///    ^^^^^^^^^^^^^
    /// ```
    rule lang_variant_preproc_flags(ctx: &Context) -> (Option<LangFlags>, bool)
    = &assert(state.config.language_conversion_enabled, "lang converter enabled")
      t:opt_lang_variant_flags(ctx)
      {
          let raw = if let LangFlags::Common(flags) = &t {
              flags.contains(&'R') || flags.contains(&'N')
          } else {
              // In Parsoid, this checked if variants was set, but there
              // are only two possibilities!
              true
          };
          (Some(t), raw)
      }
    / &assert(!state.config.language_conversion_enabled, "lang converter disabled")
      { (None, true) }

    /// Processed language conversion flags.
    ///
    /// ```wikitext
    /// -{ flag1 ; flag2 | ... }-
    ///    ^^^^^^^^^^^^^
    /// ```
    rule opt_lang_variant_flags(ctx: &Context) -> LangFlags
    = f:(t:lang_variant_flags(ctx) "|" { t })?
    {
        // Collect & separate flags and variants into a hashtable (by key) and ordered list
        let mut flags = HashSet::new();
        let mut variants = HashSet::new();

        if let Some(f) = f {
            for item in f.node {
                match item {
                    VariantFlag::Flag(c) => {
                        flags.insert(c);
                    },
                    VariantFlag::Name(n) => {
                        variants.insert(n);
                    }
                }
            }
        }

        if !variants.is_empty() {
            return LangFlags::Combined(variants);
        }

        // Parse flags (this logic is from core/languages/ConverterRule.php
        // in the parseFlags() function)
        if flags.is_empty() {
            flags.insert(LangFlags::DOLLAR_S);
        } else if flags.contains(&'R') {
            flags.retain(|f| *f == 'R');
        } else if flags.contains(&'N') {
            flags.retain(|f| *f == 'N');
        } else if flags.contains(&'-') {
            flags.retain(|f| *f == '-');
        } else if flags.contains(&'T') && flags.len() == 1 {
            flags.insert('H');
        } else if flags.contains(&'H') {
            flags.retain(|f| matches!(f, 'T' | 'D'));
            flags.insert(LangFlags::DOLLAR_PLUS);
            flags.insert('H');
        } else {
            if flags.contains(&'A') {
                flags.insert(LangFlags::DOLLAR_PLUS);
                flags.insert(LangFlags::DOLLAR_S);
            }
            if flags.contains(&'D') {
                flags.remove(&LangFlags::DOLLAR_S);
            }
        }
        LangFlags::Common(flags)
    }

    /// Unprocessed language conversion flags.
    ///
    /// ```wikitext
    /// -{ flag1 ; flag2 | ... }-
    ///    ^^^^^^^^^^^^^
    /// ```
    rule lang_variant_flags(ctx: &Context) -> Spanned<Vec<VariantFlag>>
    = spanned(<
        space_or_newline()*
        f:lang_variant_flag(ctx) ++ (space_or_newline()* ";" space_or_newline()*)
        space_or_newline()*
        { f.into_iter().flatten().collect() }
      / space_or_newline()* { vec![] }
    >)

    /// A language conversion flag.
    ///
    /// ```wikitext
    /// -{ flag1 ; flag2 | ... }-
    ///    ^^^^^   ^^^^^
    /// ```
    rule lang_variant_flag(ctx: &Context) -> Option<VariantFlag>
    = c:['-'|'+'|'A'..='Z']
      { Some(VariantFlag::Flag(c)) }
    / n:lang_variant_name(ctx)
      { Some(VariantFlag::Name(n.span)) }
    / spanned(<(
        !space_or_newline()
        !nowiki(ctx)
        [^'{'|'}'|'|'|';'])+
      >)
      { None }

    /// Produces processed language conversion variants according to the given raw
    /// flag.
    ///
    /// ```wikitext
    /// -{ text }-
    ///    ^^^^
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                    ^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule lang_variant_preproc_variants(ctx: &Context, raw: bool) -> Vec<Spanned<LangVariant>>
    = &assert(raw, "raw")
      t:spanned(<text:lang_variant_text(ctx) { LangVariant::Text { text } }>)
      { vec![t] }
    / &assert(!raw, "not raw")
      t:lang_variant_option_list(ctx)
      { t }

    /// A language conversion options list or text content.
    ///
    /// ```wikitext
    /// -{ text }-
    ///    ^^^^
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                    ^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule lang_variant_option_list(ctx: &Context) -> Vec<Spanned<LangVariant>>
    = o:lang_variant_option(ctx)
      rest:(";" t:lang_variant_option(ctx) { t })*
      tr:(";" t:spanned(<$(bogus_lang_variant_option(ctx))>) { t })*
      {
          // if the last bogus option is just spaces, keep them; otherwise
          // drop all this bogus stuff on the ground
          let tr = tr.last().and_then(|text| {
              (!text.contains(|c: char| !c.is_whitespace())).then(|| {
                  text.map_node(|_| LangVariant::Empty)
              })
          });

          iter::once(o)
              .chain(rest)
              .chain(tr.into_iter())
              .collect()
      }
    / t:spanned(<text:lang_variant_text(ctx) { LangVariant::Text { text } }>)
      { vec![t] }

    /// A language conversion options list variant.
    ///
    /// ```wikitext
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///           ^^^^^^^^^^^^^^^^   ^^^^^^^^^^^^^^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                    ^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule lang_variant_option(ctx: &Context) -> Spanned<LangVariant>
    = lang_variant_two_way(ctx)
    / lang_variant_one_way(ctx)

    /// A language conversion options list two-way variant.
    ///
    /// ```wikitext
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///           ^^^^^^^^^^^^^^^^   ^^^^^^^^^^^^^^^^^^
    /// ```
    rule lang_variant_two_way(ctx: &Context) -> Spanned<LangVariant>
    = spanned(<
      space_or_newline()*
      lang:lang_variant_name(ctx)
      space_or_newline()*
      ":"
      space_or_newline()*
      text:lang_variant_nowiki(ctx, false)
      { LangVariant::TwoWay { lang, text } }
    >)

    /// A language conversion options list one-way variant.
    ///
    /// ```wikitext
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                    ^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule lang_variant_one_way(ctx: &Context) -> Spanned<LangVariant>
    = spanned(<
      space_or_newline()*
      from:lang_variant_nowiki(ctx, true)
      "=>"
      space_or_newline()*
      lang:lang_variant_name(ctx)
      space_or_newline()*
      ":"
      space_or_newline()*
      to:lang_variant_nowiki(ctx, false)
      { LangVariant::OneWay { from, lang: Box::new(lang), to } }
    >)

    /// A language conversion language variant.
    ///
    /// ```wikitext
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///           ^^^^^^^^           ^^^^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                            ^^^^^^^
    /// ```
    rule lang_variant_name(ctx: &Context) -> Spanned<Token>
    = spanned(<['a'..='z'] ['-'|'a'..='z'|'A'..='Z']+ { Token::Text }>)
      // Escaped otherwise-unrepresentable language names
      // Primarily for supporting html2html round trips; PHP doesn't support
      // using nowikis here (yet!)
    / nowiki_text(ctx)

    /// A language conversion options list variant text which may be
    /// unrepresentable without breaking Wikitext.
    ///
    /// ```wikitext
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///                      ^^^^^              ^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                    ^^^^              ^^
    /// ```
    ///
    /// html2wt support: If a language name or conversion string can't be
    /// represented w/o breaking Wikitext, just wrap it in a <nowiki>.
    /// PHP doesn't support this (yet), but Parsoid does.
    #[cache]
    rule lang_variant_nowiki(ctx: &Context, no_arrow: bool) -> Vec<Spanned<Token>>
    = t:nowiki_text(ctx)
      space_or_newline()*
      { vec![t] }
    / ctx:({
          let ctx = ctx.with_semicolon();
          if no_arrow { ctx.with_arrow() } else { ctx }
      })
      t:lang_variant_text(&ctx)
      { t }

    /// A language conversion options list variant text.
    ///
    /// ```wikitext
    /// -{ text }-
    ///    ^^^^
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///                      ^^^^^              ^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                    ^^^^              ^^
    /// ```
    rule lang_variant_text(ctx: &Context) -> Vec<Spanned<Token>>
    = t:(inlineline(ctx) / "|" { vec![] })*
    { reduce_tree(t.into_iter().flatten()) }

    /// Junk after a language conversion options list.
    ///
    /// ```wikitext
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; junk }-
    ///                                                 ^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; junk }-
    ///                                           ^^^^
    /// ```
    rule bogus_lang_variant_option(ctx: &Context)
    = lang_variant_text(ctx)?

    ////////////////////
    // Internal links //
    ////////////////////

    /// A `<gallery>` extension tag line parser.
    ///
    /// ```wikitext
    /// Target|extra|arguments
    ///        ^^^^^^^^^^^^^^^
    /// ```
    pub rule gallery_image_options() -> Vec<Spanned<Argument>>
    = ctx:({ Context::default().with_after_expansion().with_template_arg() })
      args:spanned(<
        !eof()
        nd:(
          t:template_arg_name(&ctx)
          d:spanned(<"=" space()* { Token::Text }>)
          { (t, d) }
        )?
        value:wikilink_content_text(&ctx)?
        end:(t:spanned(<"|" { Token::Text }>) { Some(t) } / eof() { None })
        { make_argument(nd, value, end) }
      >)*
    { args }

    /// A single expanded wikilink optionally prefixed by whitespace and strip
    /// markers.
    ///
    /// TODO: This is a hack for dealing with category link whitespace (ugh).
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    /// ```
    #[no_eof]
    pub rule wikilink_single_target() -> (usize, &'input str)
    = ctx:({ Context::default().with_prod_kind(Some(ProdKind::Link)) })
      (strip_marker() / space_or_newline())*
      link:wikilink_preproc_valid(&ctx)
      t:#{|input, pos| {
        if let Spanned { node: Token::Link { target, .. }, span: link_span } = link
            && let [Spanned { node: Token::Text, span }] = target.as_slice()
        {
            RuleResult::Matched(pos, (link_span.start, &input[span.into_range()]))
        } else {
            RuleResult::Failed
        }
      }}
    { t }

    /// A wikilink.
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    /// ```
    rule wikilink(ctx: &Context) -> Vec<Spanned<Token>>
    = wikilink_preproc(&ctx.with_prod_kind(Some(ProdKind::Link)))
    / &("[[" { ctx.prod_kind.set(None); }) t:broken_wikilink(ctx) { t }

    /// An intermediate production rule that exists only to avoid recreating
    /// the context object.
    rule wikilink_preproc(ctx: &Context) -> Vec<Spanned<Token>>
    = t:wikilink_preproc_valid(ctx) { vec![t] }
    / t:wikilink_preproc_invalid(ctx)

    /// A well-formed and valid wikilink item production.
    ///
    /// ```wikitext
    /// [[Link target]]
    /// [[Link target|extra|arguments]]
    /// ```
    ///
    /// (In parsoid: wikilink_preproc_internal)
    rule wikilink_preproc_valid(ctx: &Context) -> Spanned<Token>
    = spanned(<
      "[["
      target:wikilink_target(ctx)?
      content:wikilink_content(ctx)
      inline_breaks(ctx)
      "]]"
      trail:wikilink_trail()?
      {
          // <https://www.mediawiki.org/wiki/Help:Links#Pipe_trick>
          let pipe_trick = matches!(
              content.as_slice(),
              [Spanned { span, node: Argument { content, .. }, .. }] if span.is_empty()
          );
          if !pipe_trick && let Some(target) = target {
              Token::Link {
                  target,
                  content,
                  trail,
              }
          } else {
              Token::Text
          }
      }
    >)

    /// A wikilink link trail.
    ///
    /// ```wikitext
    /// [[Link target]]ings
    ///                ^^^^
    /// ```
    rule wikilink_trail() -> Span
    = #{|input, pos| {
        if let Some(captures) = state.config.link_trail_pattern.captures(&input[pos..]).ok().flatten()
            && let Some(trail) = captures.get(1) {
            RuleResult::Matched(pos + trail.end(), Span::new(pos, pos + trail.end()))
        } else {
            RuleResult::Failed
        }
    }}

    /// A syntatically valid but semantically invalid wikilink production.
    ///
    /// ```wikitext
    /// [[<invalid-target/>]]
    /// ```
    rule wikilink_preproc_invalid(ctx: &Context) -> Vec<Spanned<Token>>
      = a:spanned(<"[[" { Token::Text }>)
        b:inlineline(ctx)
        c:spanned(<"]]" { Token::Text }>)
      { reduce_tree(
            iter::once(a)
                .chain(b)
                .chain(iter::once(c))
      ) }

    /// A wikilink with no terminator. This could also be a confused external
    /// link preceded by a literal "[".
    ///
    /// ```wikitext
    /// [[Link target
    ///
    /// [[//example.com]
    /// ```
    rule broken_wikilink(ctx: &Context) -> Vec<Spanned<Token>>
    = t0:spanned(<"[" { Token::Text }>)
      t1:extlink(&ctx.with_prod_kind(None))
    { vec![t0, t1] }
    / t:spanned(<"[[" { Token::Text }>) { vec![t] }

    /// The target of a wikilink.
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    ///   ^^^^^^^^^^^
    /// ```
    ///
    /// (In parsoid: `wikilink_preprocessor_text`)
    rule wikilink_target(ctx: &Context) -> Vec<Spanned<Token>>
    = t:(t:wikilink_target_simple(ctx) { vec![t] } / wikilink_target_complex(ctx))+
    { reduce_tree(t.into_iter().flatten()) }

    /// A simple wikilink target production.
    ///
    /// ```wikitext
    /// [[Some {{{1|link}}} &Eacute; <!-- extra -->{{target}}|extra|arguments]]
    ///   ^^^^
    /// ```
    rule wikilink_target_simple(ctx: &Context) -> Spanned<Token>
    = spanned(<
      [^'<'|'['|'{'|'\n'|'\r'|'\t'|'|'|'!'|']'|'}'|' '|'&'|'-']+
      { Token::Text }
    >)

    /// A complex wikilink target production.
    ///
    /// ```wikitext
    /// [[Some {{{1|link}}} &Eacute; <!-- extra -->{{target}}|extra|arguments]]
    ///       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule wikilink_target_complex(ctx: &Context) -> Vec<Spanned<Token>>
    = !inline_breaks(ctx)
      !pipe()
      t:(
        directive(ctx)
        // TODO: This is going to create a lot of intermediate tokens; is
        // there a reason to not collect into a single Token::Text here?
        / (!"]]" t:spanned(<(text_char() / ['!'|'<'|'-'|'}'|']'|'\n'|'\r']) { Token::Text }>) { vec![t] })
      )
    { t }

    /// The argument list of a wikilink.
    ///
    /// Normally, wikilinks have only zero or one parameters and subsequent
    /// arguments are ignored. Images, however, are syntactically identical to
    /// wikilinks and have multiple parameters.
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    ///              ^^^^^^^^^^^^^^^^
    /// ```
    rule wikilink_content(ctx: &Context) -> Vec<Spanned<Argument>>
    = ctx:({ ctx.with_linkdesc() })
      (pipe() / &"]]")
      t:spanned(<
        !"]]"
        nd:(
          // It is actually important that trailing spaces are maintained in the
          // argument name because at least the `link` argument behaves
          // differently in the presence of trailing whitespace.
          t:template_arg_name(&ctx)
          d:spanned(<"=" space()* { Token::Text }>)
          { (t, d) }
        )?
        value:wikilink_content_text(&ctx.without_equal())?
        end:(
            t:spanned(<pipe() { Token::Text }>)
            { Some(t) }
          / &"]]"
            { None }
        )
        { make_argument(nd, value, end) }
      >)*
    { t }

    /// A wikilink argument.
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    ///               ^^^^^ ^^^^^^^^^
    rule wikilink_content_text(ctx: &Context) -> Vec<Spanned<Token>>
    = // Tables are allowed inside image captions.
      // Suppress the equal flag temporarily in this rule to consume the '=' here.
      ctx:({ ctx.without_equal() })
      t:(
          wikilink_content_text_line(&ctx)
          / urltext(&ctx)
          / wikilink_content_text_element(&ctx)
      )+
    { reduce_tree(t.into_iter().flatten()) }

    /// A wikilink argument containing line content.
    ///
    /// ```wikitext
    /// [[Link target|
    /// =h2=
    /// ----|arguments]]
    /// ^^^^
    ///
    // TODO: This irrelevant nonsense from Parsoid:
    // This group is similar to `block_line` but `list_item` is omitted since
    // `doBlockLevels` happens after `handleInternalLinks2`, where newlines are
    // stripped
    rule wikilink_content_text_line(ctx: &Context) -> Vec<Spanned<Token>>
    = s:sol(&ctx)
      e:(
          heading(&ctx)
        / t:hr(&ctx) { vec![t] }
        / full_table_in_link_caption(&ctx)
      )
    { reduce_tree(s.into_iter().chain(e)) }

    /// A wikilink argument containing tags, templates, etc.
    ///
    /// ```wikitext
    /// [[Link target|{{Content}}|arguments]]
    ///               ^^^^^^^^^^^
    ///
    // TODO: This seems wrong, nested productions of links should be invalid?
    rule wikilink_content_text_element(ctx: &Context) -> Vec<Spanned<Token>>
    = !inline_breaks(&ctx)
      t:(
          inline_element(&ctx)
          / t:spanned(<"[" text_char()+ "]" &(!"]" / "]]") { Token::Text }>) { vec![t] }
          / t:spanned(<[_] { Token::Text }>) { vec![t] }
      )
    { t }

    ////////////////////
    // External links //
    ////////////////////

    /// An external link item.
    ///
    /// ```wikitext
    /// [//example.com External site]
    /// ```
    rule extlink(ctx: &Context) -> Spanned<Token>
    = &assert(!ctx.extlink, "non-extlink")
      ctx:({ ctx.with_extlink() })
      t:spanned(<
          "["
          target:extlink_target(&ctx)
          (space() / unispace())*
          content:(t:inlineline(&ctx)? { t.unwrap_or(vec![]) })
          "]"
          { Token::ExternalLink { target, content } }
      >)
    { t }

    /// The target part of an external link item.
    ///
    /// ```wikitext
    /// [//example.com External site]
    ///  ^^^^^^^^^^^^^
    /// ```
    rule extlink_target(ctx: &Context) -> Vec<Spanned<Token>>
    = addr:spanned(<(url_protocol() ipv6urladdr())? { Token::Text }>)
      target:(t:extlink_nonipv6url(ctx)? { t.unwrap_or(vec![]) })
      t:#{|input, pos| {
          let parts = reduce_tree(
              iter::once(addr).chain(target)
          );

          let is_valid = if let [Spanned { span, node: Token::Text }] = parts.as_slice() {
              let target = &input[span.into_range()];
              // Protocol must be valid and there ought to be at least
              // one post-protocol character
              state.config.protocols.iter().any(|protocol| {
                  let matches = target.get(..protocol.len())
                    .is_some_and(|target| target.eq_ignore_ascii_case(protocol));
                  matches && protocol.len() < target.len()
              })
          } else {
              !parts.is_empty()
          };

          if is_valid {
              RuleResult::Matched(pos, parts)
          } else {
              RuleResult::Failed
          }
      }}
    { t }

    /// The path part of an external link item target.
    ///
    /// ```wikitext
    /// [//example.com/{{{path}}} External site]
    ///    ^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    // added special separator character class inline: separates url from
    // description / text
    rule extlink_nonipv6url(ctx: &Context) -> Vec<Spanned<Token>>
    = // Prevent breaking on pipes when we're in a link description.
      // See the test, 'Images with the "|" character in the comment'.
      ctx:({ ctx.without_linkdesc() })
      t:(
        t:extlink_nonipv6url_simple() { vec![t] }
        / extlink_nonipv6url_complex(&ctx)
        // single quotes are ok, double quotes are bad
        / t:spanned(<"'" !"'" { Token::Text }>) { vec![t] }
      )+
    { reduce_tree(t.into_iter().flatten()) }

    /// A simple production of the path part of an external link item target.
    ///
    /// ```wikitext
    /// [//example.com/{{{path}}} External site]
    ///    ^^^^^^^^^^^^
    /// ```
    rule extlink_nonipv6url_simple() -> Spanned<Token>
    = spanned(<no_punctuation_char_extlink()+ { Token::Text }>)

    /// A complex production of the path part of an external link item target.
    ///
    /// ```wikitext
    /// [//example.com/{{{path}}} External site]
    ///                ^^^^^^^^^^
    /// ```
    rule extlink_nonipv6url_complex(ctx: &Context) -> Vec<Spanned<Token>>
    = !inline_breaks(&ctx)
      t:(
          directive(&ctx)
        / t:spanned(<['&'|'|'|'{'|'}'|'-'|'!'|'='] { Token::Text }>) { vec![t] }
      )
    { t }

    /// A run of plain text, including behavior switches, which may contain
    /// auto-linkable URLs.
    rule urltext(ctx: &Context) -> Vec<Spanned<Token>>
    = t:(urltext_special_performance_hack(ctx)
        / &"&" t:htmlentity() { vec![t] }
        / &"__" t:behavior_switch() { vec![t] }
        / t:spanned(<text_char() { Token::Text }>) { vec![t] }
      )+
    { reduce_tree(t.into_iter().flatten()) }

    /// A run of plain text containing no Wikitext, possibly followed by an
    /// autolink-able string.
    ///
    /// ```wikitext
    /// Lorem ipsum dolor https://example.com sit amet
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    ///
    // TODO:
    // This production *appears* to be designed (documenting “why” in comments
    // does not seem to be a strong suit of the upstream code) to avoid testing
    // each position for autolink production during long text runs. However, it
    // is not clear why exactly a regular expression would be superior here
    // since has to do all the same checking that the PEG does.
    rule urltext_special_performance_hack(ctx: &Context) -> Vec<Spanned<Token>>
    = plain:spanned(<
        #{|input, pos| {
          if let Some(captures) = state.urltext_lookahead.captures(&input[pos..]) && let (Some(plain), stop) = (captures.get(1), captures.get(2))
          {
              let found_autolink = stop.is_some_and(|stop| !stop.is_empty());
              RuleResult::Matched(pos + plain.len(), (Token::Text, found_autolink))
          } else {
              RuleResult::Failed
          }
        }}
      >)
      autolink:(
          &assert(plain.node.1, "autolink match")
          t:autolink(ctx) { t }
      )?
    {?
        let plain = plain.map_node(|(node, _)| node);
        match (plain.span.is_empty(), autolink) {
            (true, Some(autolink)) => Ok(vec![autolink]),
            (false, Some(autolink)) => Ok(vec![plain, autolink]),
            (false, None) => Ok(vec![plain]),
            (true, None) => Err("plain text or autolink")
        }
    }

    /// A plain text URL or magic string which is automatically converted to a
    /// link.
    ///
    /// ```wikitext
    /// https://example.com
    /// ISBN 0-7475-3269-9
    /// PMID 1923
    /// RFC 42
    /// ```
    rule autolink(ctx: &Context) -> Spanned<Token>
    = #{|input, pos| {
        // Autolinks must end on a word boundary.
        // `urltext_special_performance_hack` is responsible for advancing the
        // cursor to an appropriate position to check for this
        if !ctx.extlink
            && !matches!(
                input[..pos].chars().nth_back(0),
                Some(c) if c.is_ascii_alphanumeric() || c == '_'
        ) {
            RuleResult::Matched(pos, ())
        } else {
            RuleResult::Failed
        }
    }}
    t:(
        autourl(ctx)
        / autoref()
        / isbn()
    )
    { t }

    /// A plain text URL which is automatically converted to a link, plus any
    /// trailing punctuation.
    ///
    /// ```wikitext
    /// https://example.com
    /// ```
    rule autourl(ctx: &Context) -> Spanned<Token>
    = !"//" // protocol-relative autolinks not allowed (T32269)
      t:spanned(<
        proto:spanned(<url_protocol() { Token::Text }>)
        prefix:spanned(<ipv6urladdr()? { Token::Text }>)
        path:(
            !inline_breaks(ctx)
            t:autourl_path_segment(ctx)
            { t }
        )*
        t:#{|input, pos| {
            let proto_len = proto.span.len();
            let mut path = reduce_tree(
                iter::once(proto).chain(iter::once(prefix)).chain(path.into_iter().flatten())
            );

            // Exclude any terminating punctuation (',' '.' etc.) from the
            // autolinked URL
            let end = if let Some(Spanned { span, node: Token::Text }) = path.last_mut() {
                let fragment = &input[span.into_range()];
                let include_bracket = fragment.contains('(');
                // Operating on bytes instead of chars is a
                // micro-optimisation
                let end = fragment.bytes().rposition(|b| {
                    !matches!(b, b',' | b';' | b'.' | b':' | b'!' | b'?')
                    && (include_bracket || b != b')')
                });
                if let Some(end) = end {
                    *span = Span::new(span.start, span.start + end + 1);
                }

                // Disconnect the span from `path.last_mut()` to call
                // `path.len()`
                let span = *span;

                // ensure we haven't stripped everything: T106945
                if span.len() <= proto_len && path.len() == 1 {
                    return RuleResult::Failed;
                }

                span.end
            } else {
                pos
            };

            RuleResult::Matched(end, Token::Autolink {
                target: path,
                content: vec![],
            })
        }}
        { t }
    >)
    { t }

    /// The authority and path of an autolink URL.
    ///
    /// ```wikitext
    /// https://example.com/{{Path}}?query#Anchor
    ///         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule autourl_path_segment(ctx: &Context) -> Vec<Spanned<Token>>
    = // single quotes are ok, double quotes are bad
      t:spanned(<(no_punctuation_char() / ("'" !"'")) { Token::Text }>) { vec![t] }
      / t:comment() { vec![t] }
      / template_param_or_template(ctx)
      / t:spanned(<"{" { Token::Text }>) { vec![t] }
      / t:autourl_html_entity(ctx) { vec![t] }

    /// A non-terminating HTML entity or bare ampersand inside an autolink URL.
    ///
    /// ```wikitext
    /// https://example.com/?1&amp;2
    ///                       ^^^^^
    ///
    /// https://example.com/?1&2
    ///                       ^
    /// ```
    rule autourl_html_entity(ctx: &Context) -> Spanned<Token>
    = !(
        rhe:raw_htmlentity()
        assert(matches!(rhe, Some('<' | '>' | '\u{00A0}')), "not greater-than, less-than, or nbsp")
      )
      t:(
          &"&" t:htmlentity() { t }
          / spanned(<"&" { Token::Text }>)
      )
    { t }

    /// A magic string which is automatically converted to a link.
    ///
    /// ```wikitext
    /// ISBN 0-7475-3269-9
    /// PMID 1923
    /// RFC 42
    /// ```
    rule autoref() -> Spanned<Token>
    = spanned(<
        content:spanned(<
            kind:$(RFC() / PMID())
            space_or_nbsp()+
            identifier:$(['0'..='9']+)
            { (kind, identifier) }
        >)
        end_of_word()
        {
            let (kind, identifier) = content.node;
            let target = if kind == "RFC" {
                format!("https://datatracker.ietf.org/doc/html/rfc{identifier}")
            } else {
                format!("//www.ncbi.nlm.nih.gov/pubmed/{identifier}?dopt=Abstract")
            };
            Token::Autolink {
                target: vec![Spanned::new(
                    Token::Generated(target),
                    content.span.start,
                    content.span.end
                )],
                content: vec![content.map_node(|_| Token::Text)]
            }
        }
    >)

    /// Magic word for RFC autolinks.
    rule RFC()
    = &assert(state.config.magic_links.rfc, "rfc autolink enabled")
      "RFC"

    /// Magic word for PubMed autolinks.
    rule PMID()
    = &assert(state.config.magic_links.pmid, "pmid autolink enabled")
      "PMID"

    /// An ISBN magic word which is automatically converted to a link.
    ///
    /// ```wikitext
    /// ISBN 0-7475-3269-9
    /// ISBN 978-07475-3269-X
    /// ```
    rule isbn() -> Spanned<Token>
    = &assert(state.config.magic_links.isbn, "isbn autolink enabled")
      t:spanned(<
        content:spanned(<
            "ISBN"
            space_or_nbsp()+
            isbn:$(
                ['0'..='9']
                (space_or_nbsp_or_dash()? ['0'..='9'])+
                (space_or_nbsp_or_dash()? ['x'|'X'])?
            ) { isbn }
        >)
        end_of_word()
        {?
            let isbn = content.node.chars().filter(|c| {
                c.is_ascii_digit() || matches!(c, 'x'|'X')
            }).collect::<String>();

            if isbn.len() == 10 ||
                (isbn.len() == 13 && matches!(&isbn[0..3], "978"|"979"))
            {
                let target = format!("Special:BookSources/{isbn}");
                Ok(Token::Autolink {
                    target: vec![Spanned {
                        node: Token::Generated(target),
                        span: content.span,
                    }],
                    content: vec![content.map_node(|_| Token::Text)]
                })
            } else {
                Err("valid isbn code")
            }
        }
    >)
    { t }

    /// Separator characters in magic links.
    rule space_or_nbsp()
    = space()
    / unispace()
    / &"&" e:raw_htmlentity() &assert(e == Some('\u{00A0}'), "non-breaking space")

    /// Separator characters in ISBN magic links.
    rule space_or_nbsp_or_dash()
    = space_or_nbsp() / "-"

    /// The protocol part of a URL.
    ///
    /// ```text
    /// https://example.com   //example.com   mailto:test@example.com
    /// ^^^^^^^^              ^^              ^^^^^^^
    /// ```
    rule url_protocol() -> &'input str
    = p:$(
          "//"
        / ['A'..='Z'|'a'..='z'] ['-'|'A'..='Z'|'a'..='z'|'0'..='9'|'+'|'.']* ":" "//"?
      )
      &assert(
        state.config.protocols.iter().any(|proto| proto.eq_ignore_ascii_case(p)),
        "valid protocol"
      )
    { p }

    /// A non-terminating HTML entity inside a URL.
    ///
    /// ```wikitext
    /// https://example.com/?1&amp;2
    ///                       ^^^^^
    /// ```
    rule url_entity() -> Spanned<Token>
    = !("&" ['l'|'L'|'g'|'G'] ['t'|'T'] ";")
      t:(
          &"&" t:htmlentity() { t }
        / spanned(<"&" { Token::Text }>)
      )
    { t }

    /// A simplified IPv6 address.
    ///
    /// This is extracted from `EXT_LINK_ADDR` in Parser.php.
    /// The IPv4 address and "at least one character of a host name" portions
    /// are punted to the `path` component of the `autourl` and `url`
    /// productions.
    rule ipv6urladdr() -> &'input str
    = $("[" ['0'..='9'|'A'..='F'|'a'..='f'|':'|'.']+ "]")

    /// Any character which is not syntatically relevant to a plain text URL in
    /// Wikitext.
    rule no_punctuation_char()
    = [^'<'|'>'|'&'|'"'|'\''|'['|']'|'{'|' '|'\r'|'\n'
      |'\x00'..='\x20'|'\x7f'|'\u{00A0}'|'\u{1680}'|'\u{180E}'
      |'\u{2000}'..='\u{200A}'|'\u{202F}'|'\u{205F}'|'\u{3000}']

    /// Any character which is not syntatically relevant to a URL inside an
    /// `extlink`.
    rule no_punctuation_char_extlink()
    = [^'<'|'&'|'"'|'\''|'['|']'|'{'|'}'|' '|'\t'|'\r'|'\n'
      |'|'|'!'|'-'|'='|'\u{00A0}'|'\u{1680}'|'\u{180E}'
      |'\u{2000}'..='\u{200A}'|'\u{202F}'|'\u{205F}'|'\u{3000}']

    //////////
    // Text //
    //////////

    /// Any item which can exist within a plain text context.
    rule directive(ctx: &Context) -> Vec<Spanned<Token>>
    = t:strip_marker() { vec![t] }
    / t:comment() { vec![t] }
    / t:annotation_tag(ctx) { vec![t] }
    / t:wellformed_extension_tag(ctx) { vec![t] }
    / t:template_param_or_template(ctx) { t }
    / &"-{" t:lang_variant_or_tpl(ctx) { t }
    / &"&" t:htmlentity() { vec![t] }
    / include_limits(ctx)

    /// A behavior switch.
    ///
    /// ```wikitext
    /// __TOC__
    /// ```
    ///
    /// <https://www.mediawiki.org/wiki/Help:Magic_words#Behavior_switches>
    rule behavior_switch() -> Spanned<Token>
    = spanned(<
        "__" name:spanned(<$((!"__" (text_char() / "-"))+)>) "__"
        {
            if contains_ignore_case(&state.config.behavior_switch_words, name.node) {
                Token::BehaviorSwitch { name: name.span }
            } else {
                Token::Text
            }
        }
    >)

    /// A bold or italic text style.
    ///
    /// ```wikitext
    /// ''italic'' '''bold''' '''''bold and italic'''''
    /// ^^      ^^ ^^^    ^^^ ^^^^^               ^^^^^
    /// ```
    ///
    /// Later processing relies on newline tokens being emitted for each line of
    /// text to balance quotes per line.
    ///
    /// It is not possible to use a simple pair rule here as we need to support
    /// mis-nested bolds/italics and MediaWiki's special heuristics for
    /// apostrophes, which are all context-dependent.
    rule quote() -> Vec<Spanned<Token>>
    = quotes:spanned(<$("''" "'"*)>)
    t:#{|input, pos| {
        // sequences of four or more than five quotes are assumed to start
        // with some number of plain-text apostrophes.
        let (len, extra) = if quotes.len() == 4 {
            (3, Some(Spanned::new(Token::Text, quotes.span.start, quotes.span.start + 1)))
        } else if quotes.len() > 5 {
            (5, Some(Spanned::new(Token::Text, quotes.span.start, quotes.span.end - 5)))
        } else {
            (quotes.len(), None)
        };

        let mut quotes = quotes.map_node(|_| {
            Token::TextStyle (
                match len {
                    2 => TextStyle::Italic,
                    3 => TextStyle::Bold({
                        let at = extra.as_ref().map_or(quotes.span.start, |extra| extra.span.end);
                        // TODO: Kinda seems like this should be using a more
                        // generic rule for Unicode space? But Parsoid only checks
                        // ASCII ' '.
                        if at > 0 && input.as_bytes()[at - 1] == b' ' {
                            TextStylePosition::Space
                        } else if at > 1 && input.as_bytes()[at - 2] == b' ' {
                            TextStylePosition::Orphan
                        } else {
                            TextStylePosition::Normal
                        }
                    }),
                    5 => TextStyle::BoldItalic,
                    _=> unreachable!()
                }
            )
        });

        RuleResult::Matched(pos, if let Some(extra) = extra {
            quotes.span.start = extra.span.end;
            vec![extra, quotes]
        } else {
            vec![quotes]
        })
    }}
    { t }

    /// All characters that cannot start syntactic structures in the middle of a
    /// line.
    // XXX: ] and other end delimiters should probably only be activated inside
    // structures to avoid unnecessarily leaving the text rule on plain
    // content.
    //
    // TODO: Much of this is should really be context-dependent (syntactic
    // flags). The `wikilink_preprocessor_text` rule is an example where
    // `text_char` is not quite right and had to be augmented. Try to minimize /
    // clarify this carefully!
    rule text_char() = "&" / [c if !STOP_CHAR.contains(c)]

    /// A hack production to defeat false positive infinite loop detection in
    /// the peg generator.
    rule not_empty() = ""

    /// An HTML comment.
    // The old parser does a straight `str.replace(/<!--((?!-->).)*-->/g, "")`
    // but we may want to emit these instead of deleting them.
    rule comment() -> Spanned<Token>
    = spanned(<"<!--" c:spanned(<(([^'-']+ / !"-->" [_])*) { Token::Text }>) end:$("-->" / eof()) {
        Token::Comment { content: c.span, unclosed: end.is_empty() }
    }>)

    /// An HTML entity.
    rule htmlentity() -> Spanned<Token>
    = spanned(<s:raw_htmlentity()
    {
        if let Some(value) = s {
            Token::Entity { value }
        } else {
            Token::Text
        }
    }>)

    /// A decoded HTML entity.
    rule raw_htmlentity() -> Option<char>
    = m:$("&" (['#'|'0'..='9'|'a'..='z'|'A'..='Z']+ / "רלמ" / "رلم") ";")
    {
        if m == "&רלמ;" || m == "&رلم;" {
            Some('\u{200f}')
        } else {
            let s = html_escape::decode_html_entities(m);
            if s == m {
                // &<not-an-entity>;
                None
            } else {
                s.chars().next()
            }
        }
    }

    /// A context-sensitive newline.
    rule newline_token() -> Spanned<Token>
    = spanned(<newline() { Token::NewLine }>)

    /// A newline.
    rule newline() = "\r"? "\n"

    /// A context-sensitive whitespace or comment.
    rule space_or_comment() -> Spanned<Token>
    = spanned(<space() { Token::Text }>) / comment()

    /// Non-line-ending whitespace.
    rule space() = [' '|'\t']

    /// A positive lookahead for the start of input.
    rule sof()
    = pos:position!() {? if pos == 0 { Ok(()) } else { Err("sof") } }

    /// A positive lookahead for the end of input.
    rule eof() = ![_]

    /// Any newline or end of file.
    rule eolf() = newline() / eof()

    /// Characters that match the PCRE "\s" class.
    rule space_or_newline() = [' '|'\t'|'\n'|'\r'|'\x0c']

    /// Characters that match the PCRE "\b" class.
    rule end_of_word()
    = eof() / !['A'..='Z'|'a'..='z'|'0'..='9'|'_']

    /// Unicode "separator, space" characters matching the PCRE "\p{Zs}" class.
    rule unispace() = [c if c.is_whitespace()]

    /// Asserts a precondition given by `cond`.
    rule assert(cond: bool, msg: &'static str)
    = {? if cond { Ok(()) } else { Err(msg) }}

    /// Matches a static string from a constant value.
    rule const_value(lit: &'static str)
    = input:$([_]*<{lit.len()}>)
    {? if input == lit { Ok(()) } else { Err(lit) } }

    /// Wraps some `T` in a span.
    rule spanned<T>(r: rule<T>) -> Spanned<T>
    = start:position!() node:r() end:position!()
    { Spanned::new(node, start, end) }
}}

/// Finds the start and end position of the next XML-like close tag which
/// matches the given tag name somewhere in the given input. The tag name
/// will be compared case-insensitively.
///
/// This avoids the overhead of compiling and/or caching regular expressions
/// for every possible tag, without bothering to first check that any such
/// overhead exists or matters. :-)
fn find_end_tag(input: &str, tag_name: &str) -> Option<(usize, usize)> {
    let mut iter = input.char_indices().peekable();

    let max_start = input.len().saturating_sub(tag_name.len() + 3);
    loop {
        let mut start = None;
        while let Some((pos, c)) = iter.next() {
            if pos > max_start {
                return None;
            }

            if c == '<'
                && let Some((next, _)) = iter.next_if(|(_, c)| *c == '/')
                && input.is_char_boundary(next + 1 + tag_name.len())
                && input[next + 1..next + 1 + tag_name.len()].eq_ignore_ascii_case(tag_name)
            {
                start = Some(pos);
                iter.nth(tag_name.len() - 1);
                break;
            }
        }
        if let Some(start) = start {
            while iter.next_if(|(_, b)| b.is_ascii_whitespace()).is_some() {}
            if let Some((pos, '>')) = iter.peek() {
                break Some((start, pos + 1));
            }
        } else {
            break None;
        }
    }
}

/// A lookahead that matches if the input is at a terminator for whatever
/// inline item is currently being parsed, according to `ctx`.
fn inline_breaks(state: &Parser<'_>, input: &str, pos: usize, ctx: &Context) -> RuleResult<()> {
    let mut iter = input[pos..].chars();
    let html_or_empty = matches!(ctx.tag_kind, Some(TagKind::Html) | None);
    let at_terminator = match iter.next().unwrap() {
        '=' => {
            if (ctx.arrow && iter.next() == Some('>')) || (ctx.equal && html_or_empty) {
                // `-{ from => variant }-`
                //          ^
                // `{{t|k=v}}`
                //       ^
                // `<ext-or-anno {{t|k=v}} />` ???
                //                    X
                true
            } else if ctx.h {
                // `=heading=== <!-- junk --></inc-or-anno>␤`
                //          ^
                pos == input.len() - 1 || state.heading_end_lookahead.is_match(&input[pos + 1..])
            } else {
                false
            }
        }
        '|' => {
            // `<ext-or-inc-or-anno | />` ???
            //                      X
            html_or_empty
                // `{{a|b|c}}`
                //       ^
            && (ctx.template_arg
                    // `{| ... k="v"| ...`
                    //              ^
                    || ctx.table_cell_attrs
                    // `[[a|b|c]]`
                    //       ^
                    || ctx.linkdesc
                    // TODO: What are these pipe–square-bracket productions?
                    // `{| ... d |[link??? |] || d2 |}`
                    //           ^         ^  ^     ^
                    || (ctx.table && matches!(iter.next(), Some('[' | ']' | '|' | '}')))
                    // `{| ... d |{{!}} d2 ...` (equivalent to `||`)
                    //           ^
                    || (ctx.table && input[pos..].starts_with("|{{!}}")))
        }
        '!' => {
            // `{| ! h !! h2 ...`
            //         ^
            // `{| ! h {{!!}} ...`
            //           X
            ctx.table_head.get()
                && !matches!(ctx.prod_kind.get(), Some(ProdKind::Template))
                && iter.next() == Some('!')
        }
        '{' => {
            // `{| ... k="v"{{!}}`  (equivalent to `|`)
            //              ^
            // `{| d {{!}}{{!}} d2 {{!}}| ...`  (equivalent to `||`)
            //       ^             ^
            (ctx.table_cell_attrs && input[pos..].starts_with("{{!}}"))
                || (ctx.table
                    && (input[pos..].starts_with("{{!}}{{!}}")
                        || input[pos..].starts_with("{{!}}|")))
        }
        '}' => match ctx.prod_kind.get() {
            // `{{a}}`
            //     ^
            Some(ProdKind::Template) => iter.next() == Some('}'),
            // `-{a}-`
            //     ^
            Some(ProdKind::Lang) => iter.next() == Some('-'),
            _ => false,
        },
        ':' => {
            // `; dt : dd`
            //       ^
            // `; [http://]`
            //         X
            // `; [[link|b:c]]`
            //            X
            // `; -{variant:to}-`
            //             X
            // `; {{Template:Foo}}`
            //              X
            ctx.colon
                && !ctx.extlink
                && !ctx.linkdesc
                && !matches!(
                    ctx.prod_kind.get(),
                    Some(ProdKind::Lang | ProdKind::Template)
                )
        }
        // `-{ ... variant : to ; }-`
        //                      ^
        ';' => ctx.semicolon,
        c @ ('\r' | '\n') => {
            if !ctx.table || (c == '\n' && ctx.linkdesc) {
                // `␤`
                //  X
                // `{| [[a|␤ ...`
                //         X
                false
            } else {
                let extra = usize::from(c == '\r' && iter.next() == Some('\n'));
                let mut ok = false;
                for c in input[pos + 1 + extra..].bytes() {
                    if matches!(c, b'!' | b'|') {
                        // `{| ... ␤ ! ...`
                        // `{| ... ␤ | ...`
                        //           ^
                        ok = true;
                        break;
                    } else if !c.is_ascii_whitespace() {
                        break;
                    }
                }
                ok
            }
        }
        // `{| ... k="v" [[link???]] ...`
        //               ^
        '[' => ctx.table_cell_attrs && iter.next() == Some('['),
        // `{| ... k="v" -{lang???}- ...`
        //               ^
        '-' => ctx.table_cell_attrs && iter.next() == Some('{'),
        ']' => {
            // `[http://example.com]`
            //                     ^
            ctx.extlink
                // `[[a]]`
                //     ^
                || (matches!(ctx.prod_kind.get(), Some(ProdKind::Link)) && iter.next() == Some(']'))
        }
        _ => panic!("unhandled case"),
    };

    if at_terminator {
        RuleResult::Matched(pos, ())
    } else {
        RuleResult::Failed
    }
}

/// Balances text style tokens by decomposing bold styles into italics.
fn balance_quotes(t: impl IntoIterator<Item = Spanned<Token>>) -> Vec<Spanned<Token>> {
    let mut acc = Vec::new();
    let mut bold_count = 0;
    let mut italic_count = 0;
    let mut first_single_letter_word = None;
    let mut first_multi_letter_word = None;
    let mut first_space = None;

    for (index, mut t) in t.into_iter().enumerate() {
        match &mut t.node {
            Token::TextStyle(TextStyle::Bold(position)) => {
                bold_count += 1;
                match position {
                    TextStylePosition::Normal => first_multi_letter_word.get_or_insert(index),
                    TextStylePosition::Orphan => first_single_letter_word.get_or_insert(index),
                    TextStylePosition::Space => first_space.get_or_insert(index),
                };
            }
            Token::TextStyle(TextStyle::Italic) => {
                italic_count += 1;
            }
            Token::TextStyle(TextStyle::BoldItalic) => {
                bold_count += 1;
                italic_count += 1;
            }
            Token::ExternalLink { content, .. }
            | Token::Heading { content, .. }
            | Token::ListItem { content, .. } => {
                *content = balance_quotes(core::mem::take(content));
            }
            Token::Link { content, .. } => {
                for arg in content {
                    arg.node.content = balance_quotes(core::mem::take(&mut arg.node.content));
                }
            }
            _ => {}
        }
        acc.push(t);
    }

    // If both bold and italic are imbalanced, fix this by decomposing a bold
    // into text + italic
    if bold_count & 1 != 0
        && italic_count & 1 != 0
        && let Some(victim) = first_single_letter_word
            .or(first_multi_letter_word)
            .or(first_space)
    {
        let Spanned {
            span,
            node: Token::TextStyle(style),
        } = &mut acc[victim]
        else {
            unreachable!()
        };
        let new_token = Spanned::new(Token::Text, span.start, span.start + 1);
        span.start += 1;
        *style = TextStyle::Italic;
        acc.insert(victim, new_token);
    }

    acc
}

/// Creates an [`Argument`] for a wikilink argument from an optional name and
/// given value.
fn make_argument(
    nd: Option<(Vec<Spanned<Token>>, Spanned<Token>)>,
    value: Option<Vec<Spanned<Token>>>,
    end: Option<Spanned<Token>>,
) -> Argument {
    let value_len = value.as_ref().map_or(0, Vec::len);
    if let Some((name, delimiter_token)) = nd {
        let delimiter = Some(name.len());
        let terminator = end.as_ref().map(|_| name.len() + 1 + value_len);
        let content = name
            .into_iter()
            .chain(iter::once(delimiter_token))
            .chain(value.into_iter().flatten())
            .chain(end)
            .collect();
        Argument {
            content,
            delimiter,
            terminator,
        }
    } else {
        let terminator = end.as_ref().map(|_| value_len);
        let content = value.into_iter().flatten().chain(end).collect();

        Argument {
            content,
            delimiter: None,
            terminator,
        }
    }
}

/// The intermediate type of a parsed XML attribute.
type AttributeValue = (Spanned<Token>, Vec<Spanned<Token>>, Option<Spanned<Token>>);

/// Creates an [`Argument`] for an XML attribute from the given name and
/// optional value.
fn make_attribute(name: Vec<Spanned<Token>>, value: Option<AttributeValue>) -> Argument {
    let delimiter = Some(name.len());
    if let Some((delimiter_token, value, end_quote)) = value {
        let terminator = end_quote.as_ref().map(|_| {
            name.len() + /* delimiter_token */ 1 + value.len()
        });
        let content = name
            .into_iter()
            .chain(iter::once(delimiter_token))
            .chain(value)
            .chain(end_quote)
            .collect();
        Argument {
            content,
            delimiter,
            terminator,
        }
    } else {
        Argument {
            content: name,
            delimiter,
            terminator: None,
        }
    }
}

/// Collapses runs of text nodes into a single node, prunes empty text nodes,
/// merges lists.
fn reduce_tree(t: impl IntoIterator<Item = Spanned<Token>>) -> Vec<Spanned<Token>> {
    let mut v = Vec::<Spanned<Token>>::new();
    let mut iter = t.into_iter().peekable();
    let mut table_count = 0_u32;
    while let Some(mut token) = iter.next() {
        if matches!(token.node, Token::TableStart { .. }) {
            table_count += 1;
        } else if matches!(token.node, Token::TableEnd) {
            table_count = table_count.saturating_sub(1);
        }

        if matches!(token.node, Token::Text)
            && let Some(Spanned { span: text_span, node: Token::Text }) = v.last_mut()
            // Text spans may be discontiguous if they are split by a discarded
            // inclusion control tag
            && text_span.end == token.span.start
        {
            *text_span = text_span.merge(token.span);
        } else if let Token::ListItem { content, .. } = &mut token.node
            && matches!(
                content.first(),
                Some(Spanned {
                    node: Token::TableStart { .. },
                    ..
                })
            )
        {
            // Normally a newline at the end of a list item would trigger the
            // end of the list item and close all the elements inside, but list
            // items which start with a Wikitext table (which can only happen
            // with the dt hack since "{|" is normally only valid at the start
            // of a line) are yet one more super special snowflake production
            // that needs to make like a Kirby and suck in everything until the
            // end of the table. If there is no end of the table, guess what:
            // unlike a normal production where this would cause tokens to decay
            // to plain text, in this situation, *everything* gets to go in the
            // table!
            for token in iter.by_ref() {
                let done = matches!(token.node, Token::TableEnd);
                content.push(token);
                if done {
                    break;
                }
            }
            *content = reduce_tree(core::mem::take(content));
            token.span.end = content.last().unwrap().span.end;
            v.push(token);
        } else if let Some(
            last @ Spanned {
                node: Token::TableStart { .. } | Token::TableRow { .. },
                ..
            },
        ) = v.last_mut()
        {
            // MW does insane things when content is in an invalid position. The
            // content is fostered out of the table, but then some weirdo
            // whitespace rules are applied where p-wrapping will start if a
            // block-level element is encountered, but newlines do not trigger
            // p-wrapping like they usually do. This code does not do that;
            // instead, it simply blasts any content that is in an illegal
            // position between elements that can actually contain content.
            // I am sure this will end up breaking something or another, but it
            // fixes more than it breaks, since templates routinely generate
            // bogus table row tokens with large amounts of whitespace in
            // between.
            if matches!(token.node, Token::TableRow { .. } | Token::TableEnd) {
                if matches!(last.node, Token::TableStart { .. }) {
                    // This is a valid production.
                    v.push(token);
                } else {
                    // Earlier Wikitext table rows are ignored by MW if they are
                    // immediately followed by a table row.
                    let start = last.span.start;
                    let is_contiguous = last.span.end == token.span.start;
                    *last = token;
                    if is_contiguous {
                        last.span.start = start;
                    }
                }
            } else if table_count == 0
                || matches!(
                    token.node,
                    Token::TableCaption { .. }
                        | Token::TableData { .. }
                        | Token::TableHeading { .. }
                        | Token::Template { .. }
                        | Token::Parameter { .. },
                )
            {
                // Templates and parameters are opaque and may produce a table
                // element later, and table row tokens that are not inside
                // tables may not actually be table rows
                v.push(token);
            } else {
                // Everything else that is not a table element is in an invalid
                // position and needs to GO AWAY. To comply exactly with MW this
                // should foster the content by dumping it before the nearest
                // table start tag, but there is also the possibility that the
                // thing that looked like a table row was actually plain text
                // so TODO: it is not good enough to just delete the content
                // outright…
                if token.span.start == last.span.end {
                    last.span.end = token.span.end;
                }
            }
        } else if matches!(token.node, Token::NewLine)
            && let (
                Some(Spanned {
                    span: li_span,
                    node: Token::ListItem { .. },
                }),
                Some(Spanned {
                    node: Token::ListItem { content, .. },
                    ..
                }),
            ) = (v.last_mut(), iter.peek())
            && !matches!(
                content.first(),
                Some(Spanned {
                    node: Token::TableStart { .. },
                    ..
                })
            )
        {
            // This fixup collapses contiguous list items separated by newlines.
            // Due to the way the `sol` rule is designed it is not possible to
            // just consume the newline token in the list item rules without
            // rewriting the grammar, which feels too annoying to do now.
            // Keeping the token would make it ambiguous as an end-of-list
            // signal, requiring additional processing elsewhere, but it
            // still needs to be represented in the list item span to ensure
            // list items can be reserialised properly. List items that start
            // with tables are special and start new lists even if they are
            // adjacent to other list items.
            li_span.end = token.span.end;
        } else if token.node != Token::Text || !token.span.is_empty() {
            v.push(token);
        }
    }
    v
}

/// Parser context information required for correct handling of inline
/// terminators (via `inline_breaks`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
struct Context {
    /// After template expansion has occurred.
    after_expansion: bool,
    /// In an external link.
    extlink: bool,
    /// In a table where the start and end delimiters cannot come from
    /// templates.
    full_table: bool,
    /// In a block heading.
    h: bool,
    /// In a wikilink argument.
    linkdesc: bool,
    /// In a container item.
    prod_kind: Cell<Option<ProdKind>>,
    /// In a table.
    table: bool,
    /// In a table caption content.
    table_caption: bool,
    /// In a table cell attribute.
    table_cell_attrs: bool,
    /// In a block nested in a table.
    table_data_block: bool,
    /// In a table heading cell.
    table_head: Cell<bool>,
    /// In an XML-like tag.
    tag_kind: Option<TagKind>,
    /// In a template argument.
    template_arg: bool,
    /// In a production where a `=>` is an inline terminator.
    arrow: bool,
    /// In a production where a `:` is an inline terminator.
    colon: bool,
    /// In a production where `=` is an inline terminator.
    equal: bool,
    /// In a production where a `;` is an inline terminator.
    semicolon: bool,
}

impl Context {
    /// Makes any `{{` or `{{{` plain text instead of a template or template
    /// parameter expansion.
    fn with_after_expansion(&self) -> Self {
        let mut this = self.clone();
        this.after_expansion = true;
        this
    }

    /// Makes `=>` an inline terminator.
    fn with_arrow(&self) -> Self {
        let mut this = self.clone();
        this.arrow = true;
        this
    }

    /// Makes `:` an inline terminator.
    fn with_colon(&self) -> Self {
        let mut this = self.clone();
        this.colon = true;
        this
    }

    /// Makes `=` an inline terminator.
    fn with_equal(&self) -> Self {
        let mut this = self.clone();
        this.equal = true;
        this
    }

    // MLM AMIRITE?
    /// Makes `=` *not* an inline terminator.
    fn without_equal(&self) -> Self {
        let mut this = self.clone();
        this.equal = false;
        this
    }

    /// Makes `]` an inline terminator.
    fn with_extlink(&self) -> Self {
        let mut this = self.clone();
        this.extlink = true;
        this
    }

    /// Makes `]` *not* an inline terminator.
    fn without_extlink(&self) -> Self {
        let mut this = self.clone();
        this.extlink = false;
        this
    }

    /// Makes the context a table where the table start and end delimiters must
    /// not come from a template.
    fn with_full_table(&self) -> Self {
        let mut this = self.clone();
        this.full_table = true;
        this
    }

    /// Makes the context a block heading.
    fn with_h(&self) -> Self {
        let mut this = self.clone();
        this.h = true;
        this
    }

    /// Makes the context a wikilink argument.
    fn with_linkdesc(&self) -> Self {
        let mut this = self.clone();
        this.linkdesc = true;
        this
    }

    /// Makes the context *not* a wikilink argument.
    fn without_linkdesc(&self) -> Self {
        let mut this = self.clone();
        this.linkdesc = false;
        this
    }

    /// Makes the context an expression with a terminator of `ProdKind`.
    fn with_prod_kind(&self, kind: Option<ProdKind>) -> Self {
        let this = self.clone();
        this.prod_kind.set(kind);
        this
    }

    /// Makes `;` an inline terminator.
    fn with_semicolon(&self) -> Self {
        let mut this = self.clone();
        this.semicolon = true;
        this
    }

    /// Makes the context a table.
    fn with_table(&self) -> Self {
        let mut this = self.clone();
        this.table = true;
        this
    }

    /// Makes the context *not* a table.
    fn without_table(&self) -> Self {
        let mut this = self.clone();
        this.table = false;
        this
    }

    /// Makes the context a table caption.
    fn with_table_caption(&self) -> Self {
        let mut this = self.clone();
        this.table_caption = true;
        this
    }

    /// Makes the context a table cell attribute list.
    fn with_table_cell_attrs(&self) -> Self {
        let mut this = self.clone();
        this.table_cell_attrs = true;
        this
    }

    /// Makes the context *not* a table cell attribute list.
    fn without_table_cell_attrs(&self) -> Self {
        let mut this = self.clone();
        this.table_cell_attrs = false;
        this
    }

    /// Makes the context a block nested in a table.
    fn with_table_data_block(&self) -> Self {
        let mut this = self.clone();
        this.table_data_block = true;
        this
    }

    /// Makes the context *not* a block nested in a table.
    fn without_table_data_block(&self) -> Self {
        let mut this = self.clone();
        this.table_data_block = false;
        this
    }

    /// Makes the context a table heading.
    fn with_table_head(&self) -> Self {
        let this = self.clone();
        this.table_head.set(true);
        this
    }

    /// Makes the context an XML-ish tag of the given kind.
    fn with_tag_kind(&self, tag_kind: Option<TagKind>) -> Self {
        let mut this = self.clone();
        this.tag_kind = tag_kind;
        this
    }

    /// Makes the context a template argument.
    fn with_template_arg(&self) -> Self {
        let mut this = self.clone();
        this.template_arg = true;
        this
    }
}

impl core::hash::Hash for Context {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.extlink.hash(state);
        self.full_table.hash(state);
        self.h.hash(state);
        self.linkdesc.hash(state);
        self.prod_kind.get().hash(state);
        self.table.hash(state);
        self.table_caption.hash(state);
        self.table_cell_attrs.hash(state);
        self.table_data_block.hash(state);
        self.table_head.get().hash(state);
        self.tag_kind.hash(state);
        self.template_arg.hash(state);
        self.arrow.hash(state);
        self.colon.hash(state);
        self.equal.hash(state);
        self.semicolon.hash(state);
    }
}

/// A container production state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ProdKind {
    /// In a wikilink production.
    Link,
    /// In a template production.
    Template,
    /// In a language conversion production.
    Lang,
}

/// An XML-like tag production state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum TagKind {
    /// In an HTML tag.
    Html,
    /// In an annotation tag.
    Annotation,
    /// In an extension tag.
    Extension,
    /// In an inclusion control tag.
    Inclusion,
}

/// An intermediate representation of a language variant option.
enum VariantFlag {
    /// The option is a flag.
    Flag(char),
    /// The option is a BCP 47 language code.
    Name(Span),
}

/// All characters that can start syntactic structures in the middle of a line.
pub(super) const STOP_CHAR: &str = "\x7f-'<[{\n\r:;]}|!=&";

/// Inclusion control tags.
static INCLUDE_TAGS: phf::Set<&str> = phf::phf_set! {
    "noinclude", "includeonly", "onlyinclude"
};

/// Returns true if any `candidates` case-insensitively match `value`.
#[inline]
fn contains_ignore_case(candidates: &phf::Set<&str>, value: &str) -> bool {
    // TODO: Use a case-insensitive hashable type instead of allocating.
    candidates.contains(&value.to_ascii_lowercase())
}
