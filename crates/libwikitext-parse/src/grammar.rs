//! Parsing expression grammar for Wikitext.

#![expect(
    clippy::cast_possible_truncation,
    reason = "Wikitext ≥2**32 is impossible"
)]
#![expect(clippy::too_many_arguments, reason = "hidden peg arguments")]

use super::{
    Argument, CommonLangFlags, HeadingLevel, InclusionMode, LangFlags, LangVariant, MARKER_PREFIX,
    MARKER_SUFFIX, MagicLink, Parser, PreprocessorOptions, Span, Spanned, TextStyle,
    TextStylePosition, Token, VOID_TAGS,
};
use core::iter;
use libmisc::{to_ascii_lower, to_lower};
use libphp_rs::strtr;
use libwikitext_common::{
    config::BitMap,
    title::{self, Namespace},
    title_decode,
};
use peg::RuleResult;
use std::collections::HashSet;
use unicode_general_category::{GeneralCategory, get_general_category};

peg::parser! {pub grammar wikitext(o: &Parser<'_>) for str {
    //////////////
    // Wikitext //
    //////////////

    /// Parses all lines of a preprocessed Wikitext document.
    pub rule start() -> Vec<Spanned<Token>>
    = first:(
        // In WikitextContentHandler, redirect is just sliced off the front
        // without any care for line position, which means that if there had
        // been more stuff on the redirect line, that more stuff becomes the
        // first line even if it does not match `at_sol`
        r:redirect()
        l:(line() / eof() { vec![] })
        { iter::once(r).chain(reduce_tree(balance_quotes(l))) }
      )?
      rest:(&at_sol() t:line() { reduce_tree(balance_quotes(t)) })*
    {
        let rest = rest.into_iter().flatten();
        first.into_iter().flatten().chain(rest).collect()
    }

    /// Whole-line expressions.
    rule line() -> Vec<Spanned<Token>>
    = comment_block()
    / t:table() ie:line_after_table()?
      { t.into_iter().chain(ie.into_iter().flatten()).collect() }
    / list()
    / hr_line()
      // The parser needs to act as if comments are stripped, even if they are
      // not, for these expressions to match
    / c:(behavior_switch() / comment_tag())* t:space_sensitive_line() n:line_eol()
    { c.into_iter().chain(iter::once(t)).chain(n).collect() }
    / i:inline()+ n:line_eol()
    { reduce_tree(i.into_iter().chain(n)) }
    / n:newline()
    { vec![n] }

    /// Remaining inline content after a table end token.
    ///
    /// ```wikitext
    /// |} more content
    ///   ^^^^^^^^^^^^^
    /// ```
    rule line_after_table() -> Vec<Spanned<Token>>
    = !at_sol() i:inline()* e:line_eol() { reduce_tree(i.into_iter().chain(e)) }

    /// Whole-line expressions that either require or reject whitespace at the
    /// start of a line.
    rule space_sensitive_line() -> Spanned<Token>
    = heading()

    /// An end of line token.
    rule line_eol() -> Option<Spanned<Token>>
    = t:newline() { Some(t) }
    / eof() { None }

    /// Intra-line expressions.
    rule inline() -> Spanned<Token>
    = wikilink()
    / external_link()
    / magic_link()
    / inline_in_tag()

    /// Expressions allowed inside non-image Wikitext tags.
    rule inline_in_tag() -> Spanned<Token>
    = extension_tag()
    / html_tag()
    / text_style()
    / behavior_switch()
    / inline_in_attr()

    /// Expressions allowed inside a Wikitext link URL.
    rule inline_in_url() -> Spanned<Token>
    = behavior_switch()
    / inline_in_any()

    /// Expressions allowed inside HTML attributes.
    rule inline_in_attr() -> Spanned<Token>
    = strip_marker()
    / inline_in_late_attr()

    /// Expressions allowed inside late HTML attributes.
    rule inline_in_late_attr() -> Spanned<Token>
    = language_tag()
    / inline_in_any()

    /// Expressions allowed in any inline context.
    rule inline_in_any() -> Spanned<Token>
    = comment_tag()
    / entity()
    / text()

    //////////////
    // Redirect //
    //////////////

    #[no_eof]
    pub rule single_redirect() -> &'input str
    = r:redirect()
      r:({?
        if let Token::Redirect { link } = r.node
          && let Token::Link { target, .. } = link.node
          && let [ Spanned { node: Token::Text, span } ] = target.as_slice()
        {
          Ok(*span)
        } else {
          Err("non-complex wikitext link")
        }
      })
      r:#{|input, pos| RuleResult::Matched(pos, &input[r.into_range()]) }
    { r }

    /// An article redirect block.
    ///
    /// ```wikitext
    /// #REDIRECT: [[Target]] <!-- extra stuff -->
    /// ^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule redirect() -> Spanned<Token>
    = spanned(<
        space_nl()*
        redirect_magic()
        space_nl()*
        ":"?
        space_nl()*
        link:wikilink_link()
        space_nl()*
        { Token::Redirect { link: Box::new(link) } }
    >)

    /// A redirect magic word.
    ///
    /// ```wikitext
    /// #REDIRECT: [[Target]]
    /// ^^^^^^^^^
    /// ```
    rule redirect_magic()
    = #{|input, pos| if let Some(result) = o.redirect.find(&input[pos..]) {
        RuleResult::Matched(pos + result.end(), ())
    } else {
        RuleResult::Failed
    }}

    //////////////////
    // Preprocessor //
    //////////////////

    /// A minimal parser for template arguments. Used for debugging.
    pub rule debug_template_args(pp: &PreprocessorOptions) -> Vec<Spanned<Argument>>
    = t:spanned(<template_argument_kv(pp, PpTerm::PIPE)>) ** "|"
    { t }

    /// Generates a half-parsed token tree for template expansion.
    pub rule preprocess(pp: &PreprocessorOptions) -> Vec<Spanned<Token>>
    = pp_items(pp, PpTerm::empty())

    /// Zero or more preprocessor items.
    rule pp_items(pp: &PreprocessorOptions, flags: PpTerm) -> Vec<Spanned<Token>>
    = items:pp_item(pp, flags)*
    { reduce_tree(items.into_iter().flatten()) }

    /// A preprocessor item.
    rule pp_item(pp: &PreprocessorOptions, flags: PpTerm) -> Vec<Spanned<Token>>
    = t:pp_illegal() { vec![t] }
    / pp_tag(pp)
    / t:pp_heading(pp) { vec![t] }
    / pp_template(pp)
    / half_language_tag(pp)
    / half_wikilink(pp)
    / t:pp_text(flags) { vec![t] }

    /// A run of non-token text.
    rule pp_text(flags: PpTerm) -> Spanned<Token>
    = spanned(<text_part(flags)+ { Token::Text }>)

    /// A part of a run of non-token text.
    rule text_part(flags: PpTerm)
    = [^'['|']'|'{'|'}'|'<'|'-'|'|'|'='|'\0'|'\x7f']+
    / !assert(flags.in_heading() || flags.contains(PpTerm::EQUALS), "=") "="+
    / !assert(flags.contains(PpTerm::PIPE), "|") "|"+
    / !assert(flags.in_link(), "]]") "]"+
    / !assert(flags.in_parameter() || flags.in_template() || flags.in_convert(), "}}") "}"+
    / !assert(flags.in_template() || flags.in_convert(), "}") "}}" !"}"
    / !assert(flags.in_convert(), "}-") "}-"
    / "-" !"{"
    / "[" !"["
    / "]" !"]"
    / "{" !"{"
    / "}" !("}" / "-")
    / assert(flags.in_heading(), "=") !heading_term() "="

    //////////////////////////
    // Half-parsed Wikitext //
    //////////////////////////

    /// A half-parsed language conversion tag.
    ///
    /// Full parsing is not done because this parse will be discarded after
    /// template expansion, so it is faster to skip it.
    ///
    /// ```wikitext
    /// -{ text }-
    ///
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    /// ```
    ///
    /// For runs of `{`, the longest right-most token wins:
    ///
    /// 1 `-{`
    /// 2 `- {{`
    /// 3 `- {{{`
    /// 4 `-{ {{{`
    /// 5 `- {{ {{{`
    /// 6 `- {{{ {{{`
    /// 7 `-{ {{{ {{{`
    #[cache]
    rule half_language_tag(pp: &PreprocessorOptions) -> Vec<Spanned<Token>>
    = &assert(o.config.language_conversion_enabled, "conversion enabled")
      // 1, 4, 7, ...
      t:half_parsed_wikitext(pp, PpTerm::IN_CONVERT, <"-{" &("{{{"* !"{")>, <"}-">, <"-">)
    { t }
      // 2, 3, ..
      // Required here because pp_text will not match "-{"
    / t:spanned(<"-" &"{" { Token::Text }>)
    { vec![t] }

    /// A half-parsed Wikilink.
    ///
    /// Full parsing is not done because this parse will be discarded after
    /// template expansion, so it is faster to skip it.
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    /// ```
    #[cache]
    rule half_wikilink(pp: &PreprocessorOptions) -> Vec<Spanned<Token>>
    = half_parsed_wikitext(pp, PpTerm::IN_LINK, <"[[">, <"]]">, <"[">)

    /// A generic rule for half-parsed Wikitext expressions.
    rule half_parsed_wikitext(
        pp: &PreprocessorOptions,
        flags: PpTerm,
        before: rule<()>,
        after: rule<()>,
        broken: rule<()>,
    ) -> Vec<Spanned<Token>>
    = start:spanned(<before() { Token::Text }>)
      content:pp_items(pp, flags)
      end:spanned(<after() { Token::Text }>)
    { iter::once(start).chain(content).chain(iter::once(end)).collect() }
      // A failed parse here with a valid `before` means that whatever prior
      // thing was unclosed will never close; the “token stack” here is
      // literally the call stack for the PEG which means that this call must
      // consume the rest of the content. It is important to check `before`
      // because if it simply does not match in the first rule (e.g. `-foo` is
      // not even a start of a language conversion tag), it should not match in
      // the failure rule either.
      // In cases like `{{x[[` where both expressions are doomed, caching in the
      // parent rule ensures that continuing to parse here is not a waste of
      // time even though the grandparent will fail to match too and there will
      // be a potentially long backtrack
    / &before()
      broken:spanned(<broken() { Token::Text }>)
      rest:pp_items(pp, PpTerm::empty())
    { iter::once(broken).chain(rest).collect() }

    //////////////
    // Headings //
    //////////////

    /// A heading with optional trailing whitespace and comments.
    ///
    /// ```wikitext
    /// =h1=  <!-- -->  <!-- -->
    /// ==h2==
    /// ===h3===
    /// ```
    ///
    /// etc.
    rule heading() -> Spanned<Token>
    = spanned(<
        start:heading_start()
        space_s()*
        // If `c` matches, `e` needs to see at least one `=` since otherwise
        // this rule would also match some non-heading template argument on a
        // new line that starts with a `=`. If it is just an “oops, all `=`”
        // header, `c` will be `None` and then `s` needs to be at least 3 long
        // for the same reason
        ce:(
            c:(!heading_term() t:inline() { t })*
            e:heading_end()
            { (reduce_tree(c), e) }
        )?
        &assert(ce.is_some() || start.len() > 2, "heading")
        heading_trail()
        (nl() &eolf())*
        { make_heading(start, ce) }
    >)

    /// A heading with optional trailing whitespace and comments parsed by the
    /// preprocessor.
    ///
    /// ```wikitext
    /// =h1=  <!-- -->  <!-- -->
    /// ==h2==
    /// ===h3===
    /// ```
    ///
    /// etc.
    ///
    /// Unlike a full heading parse, this parse fails if there are comments at
    /// the start of the line, the inner contents are half-parsed, and sequences
    /// of trailing newlines are not consumed.
    rule pp_heading(pp: &PreprocessorOptions) -> Spanned<Token>
    = spanned(<
        &at_sol()
        start:heading_start()
        // The preprocessor needs to retain whitespace in the contents because
        // `== ==` is an h2 with no body but `====` is an h1 with body `==`
        ce:(c:pp_items(pp, PpTerm::IN_HEADING) e:heading_end() { (c, e) })?
        &assert(ce.is_some() || start.len() > 2, "heading")
        heading_trail()
        { make_heading(start, ce) }
    >)

    /// The start of a heading.
    ///
    /// ```wikitext
    /// = h1 =  <!-- -->  <!-- -->
    /// ^^
    /// == h2 ==
    /// ^^^
    /// ===h3===
    /// ^^^
    /// ```
    rule heading_start() -> Span
    = s:spanned(<"="+ {}>)
    { s.span }

    /// The end of a heading.
    ///
    /// ```wikitext
    /// = h1 =  <!-- -->  <!-- -->
    ///     ^^
    /// == h2 ==
    ///      ^^^
    /// ===h3===
    ///      ^^^
    /// ```
    rule heading_end() -> Span
    = space_s()* s:spanned(<"="+ {}>)
    { s.span }

    /// The entire terminator of a heading.
    ///
    /// ```wikitext
    /// = h1 =  <!-- -->  <!-- -->␤
    ///     ^^^^^^^^^^^^^^^^^^^^^^^
    /// == h2 ==␤
    ///      ^^^^
    /// ===h3===␤
    ///      ^^^^
    /// ```
    rule heading_term()
    = heading_end() heading_trail()

    /// Trailing whitespace and comments that are part of a Wikitext heading but
    /// not part of its content.
    ///
    /// ```wikitext
    /// =h1=  <!-- -->  <!-- -->␤
    ///     ^^^^^^^^^^^^^^^^^^^^^
    /// ==h2==␤
    ///       ^
    /// ===h3===␤
    ///         ^
    /// ```
    rule heading_trail()
    = (inline_space()+ / comment_tag())* &eolf()

    ///////////
    // Lines //
    ///////////

    /// A whole-line horizontal rule expression with optional trailing content.
    ///
    /// ```wikitext
    /// ---- with extra content
    /// -----
    /// ```
    ///
    /// etc.
    rule hr_line() -> Vec<Spanned<Token>>
    = c:comment_tag()* h:hr() i:inline()* n:line_eol()
    { reduce_tree(c.into_iter().chain(iter::once(h)).chain(i).chain(n)) }

    /// A horizontal rule.
    ///
    /// ```wikitext
    /// ---- with extra content
    /// ^^^^
    /// ----- with extra content
    /// ^^^^^
    /// ```
    ///
    /// etc.
    rule hr() -> Spanned<Token>
    = spanned(<"-"*<4,> { Token::HorizontalRule }>)

    /// An unordered, ordered, or definition list item.
    ///
    /// ```wikitext
    /// * Unordered
    /// # Ordered
    /// ; Term
    /// : Detail
    /// ```
    rule list() -> Vec<Spanned<Token>>
      // TODO: list should also parse successfully if there was a `general`
      // strip marker at the start of the line that evaluates to empty string.
    = c:comment_tag()*
      t:spanned(<
        bullets:spanned(<['*'|'#'|';'|':']+ {}>)
        space_s()*
        content:(!rtrim_term() t:(inline_dd() / inline()) { t } / wikilink_category())*
        space_s()*
        // Keeping the eol inside the list item makes it easier to generate list
        // containers while streaming tokens because a newline inside of a list
        // item means that the list is continuing, where a newline outside of
        // a list item means that the list has ended
        // TODO: This is ugly and bad, actually.
        e:list_continuation()?
        { Token::ListItem {
            bullets: bullets.span,
            content: reduce_tree(reduce_dd(content).into_iter().chain(e.into_iter().flatten())),
        } }
      >)
      e:(e:line_eol() !list_start() { e })?
    { c.into_iter().chain(iter::once(t)).chain(e.flatten()).collect() }

    /// A minimal unambiguous start of a list item.
    rule list_start()
    = comment_tag()* !table_hack_start() ['*'|'#'|';'|':']

    // TODO: Docs
    rule list_continuation() -> Option<Spanned<Token>>
    = e:rtrim_eol() &list_start() { e }

    /// An inline definition detail.
    ///
    /// ```wikitext
    /// ; Term
    /// ; Term : Detail : Detail : Detail
    ///       ^^^      ^^^      ^^^
    /// ```
    ///
    /// Because correct rendering relies on matching these inline items to the
    /// nearest new definition term in a way which requires interleaving, it is
    /// not possible to easily disambiguate them in the grammar.
    rule inline_dd() -> Spanned<Token>
    = spanned(<space_s()* t:spanned(<":">) space_s()* { Token::InlineListItem }>)

    ////////////
    // Tables //
    ////////////

    /// A Wikitext table expression.
    ///
    /// ```wikitext
    /// {| k="v" ␤|+ c-k="v" | c ␤|- r-k="v" ␤! h-k="v" | h !! h2 ␤| d-k="v" | d || d2 ␤|}
    /// ```
    rule table() -> Vec<Spanned<Token>>
    = prefix:table_prefix()* part:table_part()
    { prefix.into_iter().chain(part).collect() }

    /// Content that may prefix a table line which should be absorbed into the
    /// next table token.
    rule table_prefix() -> Spanned<Token>
      // Since tables are insensitive to whitespace at the start, they are also
      // insensitive to comment tags at the start, since comments were not
      // present in this position in the original parser
    = p:position!() space_s()* t:comment_tag()
    { let mut t = t; t.span.start = p as u32; t }

    /// The list of possible table expressions.
    rule table_part() -> Vec<Spanned<Token>>
    = table_hack_start()
    / t:table_end() { vec![t] }
    / table_single()
    / table_caption()
    / table_head()
    / table_data()

    /// Table expressions that contain only attributes.
    rule table_single() -> Vec<Spanned<Token>>
    = t:(table_start() / table_row()) e:line_eol()
    { iter::once(t).chain(e).collect() }

    /// Table start tags are supposed to only start on a new line, but then
    /// someone decided to add a hack to indent tables with `<dd>`s because
    /// CSS was too hard I guess, and here we are.
    ///
    /// ```wikitext
    /// ::{| k="v" ␤
    /// ^^^^^^^^^^^^
    /// ```
    rule table_hack_start() -> Vec<Spanned<Token>>
    = first:spanned(<
        space_s()*
        bullets:spanned(<":"+ {}>)
        first:table_start()
        e:line_eol()
        content:(!table_hack_end() t:line() { t })*
        term:table_hack_end()
        { Token::ListItem {
            bullets: bullets.span,
            content: iter::once(first)
                .chain(e)
                .chain(content.into_iter().flatten())
                .chain(term)
                .collect(),
        } }
      >)
      rest:(!rtrim_term() t:inline() { t })*
      space_s()*
    { reduce_tree(iter::once(first).chain(rest)) }

    /// The end of the indented table hack.
    ///
    /// ```wikitext
    /// ::{| k="v" ␤|}
    ///             ^^
    /// ```
    rule table_hack_end() -> Vec<Spanned<Token>>
    = p:table_prefix()* e:(t:table_end() { Some(t) } / eof() { None })
    { let mut p = p; p.extend(e); p }

    /// A table start tag.
    ///
    /// ```wikitext
    /// {| k="v" ␤|+ c-k="v" | c ␤|- r-k="v" ␤! h-k="v" | h !! h2 ␤| d-k="v" | d || d2 ␤|}
    /// ^^^^^^^^^^
    /// ```
    rule table_start() -> Spanned<Token>
    = spanned(<
        space_s()* "{|"
        attributes:html_attributes(<nl() {}>)
        { Token::TableStart { attributes } }
    >)

    /// A table row tag.
    ///
    /// ```wikitext
    /// {| k="v" ␤|+ c-k="v" | c ␤|- r-k="v" ␤! h-k="v" | h !! h2 ␤| d-k="v" | d || d2 ␤|}
    ///                            ^^^^^^^^^^^^
    /// ```
    rule table_row() -> Spanned<Token>
    = spanned(<
        space_s()* "|" "-"+
        // Because this table row token might actually be just a plain text `|-`
        // at the start of a line outside of a table, the list of attributes
        // has to be parsed as a list of any inline thing
        attributes:inline()*
        { Token::TableRow { attributes: reduce_tree(attributes) } }
    >)

    /// A table end tag.
    ///
    /// ```wikitext
    /// {| k="v" ␤|+ c-k="v" | c ␤|- r-k="v" ␤! h-k="v" | h !! h2 ␤| d-k="v" | d || d2 ␤|}
    ///                                                                                   ^^
    /// ```
    ///
    /// Tables may be put inside Wikilinks using the image caption hack, or
    /// list items using the indent hack. This means that, unlike other table
    /// rules, `table_end` cannot consume a whole line by itself because there
    /// are two possible terminators (`nl()` or `"]]"`) and two possible rules
    /// about end-of-line whitespace (trim or preserve).
    rule table_end() -> Spanned<Token>
    = spanned(<space_s()* "|}" { Token::TableEnd }>)

    /// A table caption tag.
    ///
    /// ```wikitext
    /// {| k="v" ␤|+ c-k="v" | c ␤|- r-k="v" ␤! h-k="v" | h !! h2 ␤| d-k="v" | d || d2 ␤|}
    ///           ^^^^^^^^^^^^^^^^^
    /// ```
    rule table_caption() -> Vec<Spanned<Token>>
    = table_cells(<"|+">, <"||">, |attributes| Token::TableCaption { attributes })

    /// A table head tag.
    ///
    /// ```wikitext
    /// {| k="v" ␤|+ c-k="v" | c ␤|- r-k="v" ␤! h-k="v" | h !! h2 ␤| d-k="v" | d || d2 ␤|}
    ///                                        ^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule table_head() -> Vec<Spanned<Token>>
    = table_cells(<"!">, <"||" / "!!">, |attributes| Token::TableHeading { attributes })

    /// A table data tag.
    ///
    /// ```wikitext
    /// {| k="v" ␤|+ c-k="v" | c ␤|- r-k="v" ␤! h-k="v" | h !! h2 ␤| d-k="v" | d || d2 ␤|}
    ///                                                             ^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule table_data() -> Vec<Spanned<Token>>
    = table_cells(<"|">, <"||">, |attributes| Token::TableData { attributes })

    /// A generic rule for table cells with optional attributes and zero or more
    /// content cells.
    ///
    /// ```wikitext
    /// {| k="v" ␤|+ c-k="v" | c ␤|- r-k="v" ␤! h-k="v" | h !! h2 ␤| d-k="v" | d || d2 ␤|}
    ///            ^^^^^^^^^^^^^^^^            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule table_cells<F>(
        first_start: rule<()>,
        rest_start: rule<()>,
        token_map: F,
    ) -> Vec<Spanned<Token>>
        where F: Fn(Vec<Spanned<Token>>) -> Token
    = first:table_cell(first_start, &rest_start)
      rest:table_cell(&rest_start, &rest_start)*
      end:rtrim_eol()
    { reduce_tree(iter::once(first)
        .chain(rest)
        .flat_map(|(start, attributes, rest)| {
            let cell = Spanned::new(token_map(attributes), start.start, start.end);
            iter::once(cell).chain(rest)
        })
        .chain(end))
    }

    /// A single table cell with optional attributes and content.
    ///
    /// ```wikitext
    /// {| k="v" ␤|+ c-k="v" | c ␤|- r-k="v" ␤! h-k="v" | h !! h2 ␤| d-k="v" | d || d2 ␤|}
    ///             ^^^^^^^^^^^^^^^             ^^^^^^^^^^^^^^^^^^^^ ^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule table_cell(start: rule<()>, term: rule<()>)
        -> (Span, Vec<Spanned<Token>>, Vec<Spanned<Token>>)
    = sa:spanned(<
        space_s()*
        start()
        space_s()*
        // The ambiguous parse of a table cell with a "|" inside versus a
        // terminator for table attributes is disambiguated by whether the
        // expression contains "[[" or "-{", which is tested here by terminating
        // at those sequences which will cause "|" to fail to match. (Authors have
        // to put any "|" literal in a `<nowiki>` because the original parser is
        // so context-unaware the only way to successfully get that character in
        // this position is by having it in a strip marker.)
        a:(t:html_attributes(<"[[" / "-{" / "|" / nl() {}>) "|" !"|" { t })?
        space_s()*
        { a }
      >)
      body:(!(space_s()* (term() / nl())) t:inline() { t })*
    { (sa.span, sa.node.unwrap_or_default(), body) }

    /// The end of a table line.
    rule rtrim_eol() -> Option<Spanned<Token>>
    = t:spanned(<rtrim_term() { Token::NewLine }>) { Some(t) }
    / eof() { None }

    /// A termination rule for right-side ASCII whitespace trimming expressions.
    rule rtrim_term()
    = space_s()* eolf()

    ///////////
    // Links //
    ///////////

    /// A `<gallery>` extension tag line parser.
    ///
    /// ```wikitext
    /// Target|extra|arguments
    ///        ^^^^^^^^^^^^^^^
    /// ```
    pub rule gallery_image_options() -> Vec<Spanned<Argument>>
    = spanned(<wikilink_argument_kv(<inline()>, <eolf() {}>)>) ** "|"

    /// A wikilink, category, or image, or some text that looked like a Wikilink
    /// but wasn’t.
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    /// ```
    rule wikilink() -> Spanned<Token>
    = wikilink_category()
    / wikilink_image()
    / wikilink_link()
      // The possibilities in this position are:
      //
      // "[[[" - A literal "[[" followed by maybe a Wikilink or external link
      //        (take "[[")
      // "[[" - A literal "[" followed by maybe an external link (take "[")
      // "[" - Maybe an external link (fail)
      //
      // Absent is the possibility that this could be a "[" and then a Wikilink
      // because the original parser ‘parsed’ by splitting the input on "[["
    / spanned(<"[" &"[" ("[" &"[")? { Token::Text }>)

    /// A category wikilink.
    ///
    /// ```wikitext
    /// [[Category:Target|Sort key]]
    /// ```
    ///
    /// This has to be parsed separately from other Wikilinks because it has
    /// a left-handed whitespace erasure rule.
    rule wikilink_category() -> Spanned<Token>
    = spanned(<
        ("\n" space_nl()*)?
        "[["
        target:wikilink_category_target()
        content:wikilink_content(<!"[[" t:inline_in_tag() { t }>)
        "]]"
        {
            // TODO: Use a different token type
            Token::Link {
                prefix: <_>::default(),
                content,
                target,
                trail: <_>::default(),
            }
        }
    >)

    /// The target of a category wikilink.
    ///
    /// ```wikitext
    /// [[Category:Target|Sort key]]
    ///   ^^^^^^^^^^^^^^^
    /// ```
    rule wikilink_category_target() -> Vec<Spanned<Token>>
    = start:position!() t:wikilink_target()
      // TODO: This also is supposed to apply to interlanguage titles.
    #{|input, end| assert_namespace(o, input, start, end, Namespace::CATEGORY) }
    { t }

    /// An image wikilink.
    ///
    /// ```wikitext

    /// ```
    ///
    /// This has to be parsed separately from other Wikilinks because the
    /// content part is parsed differently.
    rule wikilink_image() -> Spanned<Token>
    = spanned(<
        "[["
        target:wikilink_image_target()
        content:wikilink_content(<inline()>)
        "]]"
        {
            // TODO: Use a different token type
            Token::Link {
                prefix: <_>::default(),
                content,
                target,
                trail: <_>::default(),
            }
        }
    >)

    /// The target of an image wikilink.
    ///
    /// ```wikitext
    /// [[File:Image.jpg|flag|caption]]
    ///   ^^^^^^^^^^^^^^
    /// ```
    rule wikilink_image_target() -> Vec<Spanned<Token>>
    = start:position!() t:wikilink_target()
    #{|input, end| assert_namespace(o, input, start, end, Namespace::FILE) }
    { t }

    /// A wikilink.
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    /// ```
    rule wikilink_link() -> Spanned<Token>
    = spanned(<
        prefix:wikilink_prefix()?
        "[["
        target:wikilink_target()
        content:wikilink_content(<!"[[" t:inline_in_tag() { t }>)
        "]]"
        trail:wikilink_trail()?
        { Token::Link {
            prefix: prefix.unwrap_or_default(),
            content,
            target,
            trail: trail.unwrap_or_default(),
        } }
    >)

    /// Text preceding a Wikilink that should be absorbed into the start of
    /// the hyperlink.
    ///
    /// ```wikitext
    /// al[[Link target|extra|arguments]]
    /// ^^
    /// ```
    rule wikilink_prefix() -> Vec<Spanned<Token>>
    = &wikilink_prefix_match() t:(!"[[" t:inline() { t })*
    { reduce_tree(t) }

    /// A check for a matching Wikilink prefix.
    rule wikilink_prefix_match()
    = #{|input, pos| {
        let pattern = o.config.link_prefix_pattern.as_ref();
        if pattern.is_some_and(|p| p.is_match(&input.as_bytes()[pos..])) {
            RuleResult::Matched(pos, ())
        } else {
            RuleResult::Failed
        }
    }}

    /// The target part of a Wikilink.
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    ///   ^^^^^^^^^^^
    /// ```
    // TODO: If it does not feel like death to get the root article name
    // passed around everywhere, this could just create the final link title.
    rule wikilink_target() -> Vec<Spanned<Token>>
    = s:spanned(<(" " / "%20")+ { Token::Text }>)?
      // Technically this is supposed to decode any percent-encoding before
      // checking for a URL scheme, but in practice it makes no sense that part
      // would be URL encoded, so hopefully nobody did that (lol).
      !url_scheme() t:wikilink_target_part()*
    { reduce_tree(s.into_iter().chain(t)) }

    rule wikilink_target_part() -> Spanned<Token>
    = entity()
    / !("[[" / strip_marker()) t:spanned(<#{|input, pos| wikilink_target_char(
        input, pos, &o.config.valid_title_bytes
    )} { Token::Text }>)
    { t }

    /// Text content part of a Wikilink.
    ///
    /// ```wikitext
    /// [[Link target|extra|arguments]]
    ///               ^^^^^^^^^^^^^^^
    /// ```
    rule wikilink_content(item: rule<Spanned<Token>>) -> Vec<Spanned<Argument>>
    = start:position!()
      t:wikilink_argument(&item)*
      bracket:wikilink_content_end_bracket(start)?
    {?
        fn is_pipe_trick(args: &[Spanned<Argument>]) -> bool {
            matches!(args, [
                Spanned { node: Argument { content, .. }, .. }
            ] if content.is_empty())
        }

        let mut t = t;
        if let (Some(last), Some(bracket)) = (t.last_mut(), bracket) {
            last.span.end = bracket.span.end;
            last.node.content.push(bracket);
        }

        if is_pipe_trick(&t) {
            Err("wikilink content")
        } else {
            Ok(t)
        }
    }

    /// A Wikilink argument with pipe delimiter.
    ///
    /// ```wikitext
    /// [[target|numbered argument |key=value]]
    ///         ^^^^^^^^^^^^^^^^^^ ^^^^^^^^^^
    /// ```
    rule wikilink_argument(item: rule<Spanned<Token>>) -> Spanned<Argument>
      // Keeping the delimiter outside of the argument span allows the arguments
      // list to be glued back together into text in a generic way that applies
      // to all lists of spans, instead of requiring special work to extract the
      // difference from the start of the argument and the start of its first
      // content token
    = "|"
      t:spanned(<
        n:newline()
        v:(&at_sol() t:wikilink_argument_line(&item) { t })+
        { make_argument(reduce_tree(iter::once(n).chain(v.into_iter().flatten())), None) }
        / kv:wikilink_argument_kv(&item, <"|" / "]]">)?
        { kv.unwrap_or_default() }
      >)
    { t }

    /// A Wikilink argument.
    ///
    /// ```wikitext
    /// [[target|numbered argument|key=value]]
    ///          ^^^^^^^^^^^^^^^^^ ^^^^^^^^^
    /// ```
    rule wikilink_argument_kv(item: rule<Spanned<Token>>, term: rule<()>) -> Argument
    = key:wikilink_inline(&item, <"=" / term()>)*
      value:(d:spanned(<"=" { Token::Text }>) v:wikilink_inline(&item, &term)* { (d, v) })?
    { make_argument(key, value) }

    /// Inline expressions allowed in a Wikilink argument.
    ///
    /// More expressions are allowed when an argument breaks onto a new line;
    /// see `wikilink_argument_line`.
    rule wikilink_inline(item: rule<Spanned<Token>>, term: rule<()>) -> Spanned<Token>
    = !term() t:item()
    { t }
    / newline()

    /// A whole-line expression in a Wikilink argument.
    ///
    /// Only expressions that are converted to HTML before Wikilinks are
    /// converted to HTML are applicable in this position.
    ///
    /// ```wikitext
    /// [[Link target|
    /// =h2=
    /// ----|arguments]]
    /// ^^^^
    /// ```
    rule wikilink_argument_line(item: rule<Spanned<Token>>) -> Vec<Spanned<Token>>
    = comment_block()
    / t:table() ie:wikilink_after_table()?
    { reduce_tree(t.into_iter().chain(ie.into_iter().flatten())) }
    / hr_line()
    / c:(behavior_switch() / comment_tag())* h:heading() n:line_eol()
    { reduce_tree(c.into_iter().chain(iter::once(h)).chain(n)) }
    / !"|" i:(!"]]" t:item() { t })+ n:line_eol()
    { reduce_tree(i.into_iter().chain(n)) }
    / n:newline()
    { vec![n] }

    /// Remaining inline content after a table end token inside of a Wikilink.
    ///
    /// ```wikitext
    /// [[Image:TableCaption.png|
    /// {|
    ///  |
    ///  |} more content]]
    ///    ^^^^^^^^^^^^^
    /// ```
    rule wikilink_after_table() -> Vec<Spanned<Token>>
    = (!at_sol() t:(!"]]" t:inline_in_tag() { t })* n:line_eol()
    { reduce_tree(t.into_iter().chain(n)) })

    /// An ambiguous bracket at the end of a wikilink.
    ///
    /// ```wikitext
    /// [[Link target|[]]]
    ///                ^
    /// [[Link target| ]]]
    ///                x
    /// ```
    rule wikilink_content_end_bracket(start: usize) -> Spanned<Token>
    = #{|input, pos| {
        let consume_bracket = input[pos..].starts_with("]]]") && input[start..pos].contains('[');
        if consume_bracket {
            let end = pos + 1;
            RuleResult::Matched(end, Spanned::new(Token::Text, pos as u32, end as u32))
        } else {
            RuleResult::Failed
        }
    }}

    /// Text after a Wikilink that should be absorbed into the hyperlink.
    ///
    /// ```wikitext
    /// [[Link target]]ings
    ///                ^^^^
    /// ```
    rule wikilink_trail() -> Vec<Spanned<Token>>
    = start:position!()
      i:wikilink_trail_raw()
    {? wikilink_trail_content(i, o, start).map_err(|_| "trail") }

    /// The content of a Wikilink trail.
    ///
    /// ```wikitext
    /// [[Link target]]ings
    ///                ^^^^
    /// ```
    ///
    /// This contortion is necessary because the link trail regular expression
    /// can match arbitrary sequences, including potentially cutting through the
    /// middle of tokens. For example, a link trail `[a-z&]*` matching the
    /// Wikilink `[[a]]foo&lt;` should result in `[Text("foo&")]` whereas a
    /// link trail `[a-z&;]*` on the same Wikitext should result in
    /// `[Text("foo"), Entity('<')]`. The least invasive way to do this is to
    /// parse the truncated input separately. As with `only_include`, the whole
    /// input is passed and then immediately advanced to the start position so
    /// that the resulting spans are correct.
    pub(self) rule wikilink_trail_content(start_at: usize) -> Vec<Spanned<Token>>
    = #{|_, _| RuleResult::Matched(start_at, ()) }
      t:(!"[[" t:inline() { t })*
    { reduce_tree(t) }

    /// Returns the input truncated at the end of a Wikilink trail, if there is
    /// one.
    rule wikilink_trail_raw() -> &'input str
    = #{|input, pos| {
        let pattern = &o.config.link_trail_pattern;
        let captures = pattern.captures(&input[pos..]).ok().flatten();
        if let Some(captures) = captures && let Some(trail) = captures.get(1) {
            let end = pos + trail.end();
            RuleResult::Matched(end, &input[..end])
        } else {
            RuleResult::Failed
        }
    }}

    /// An external link.
    ///
    /// ```wikitext
    /// [//example.com External site]
    /// ```
    rule external_link() -> Spanned<Token>
    = spanned(<
        "["
        target:external_link_target()
        unispace()*
        // Wikilinks are allowed inside external links in the original parser
        // because they are processed before external links and replaced by a
        // placeholder which is allowed in the content position
        content:(!external_link_term() t:(wikilink() / inline_in_tag()) { t })*
        "]"
        { Token::ExternalLink { content: reduce_tree(content), target } }
      >)

    /// The target part of an external link.
    ///
    /// ```wikitext
    /// [//example.com External site]
    ///  ^^^^^^^^^^^^^
    /// ```
    pub rule external_link_target() -> Vec<Spanned<Token>>
    = s:url_scheme()
      h:(url_ipish() / external_link_url_class())
      rest:external_link_url_class()*
    { reduce_tree(iter::once(s).chain(iter::once(h)).chain(rest)) }

    /// Other tokens valid in the target part of an external link.
    ///
    /// ```wikitext
    /// [//example.com External site]
    ///  ^^^^^^^^^^^^^
    /// ```
    rule external_link_url_class() -> Spanned<Token>
    = !url_term() t:inline_in_url()
      !assert(matches!(t.node, Token::Entity('<' | '>')), "&lt; or &gt;")
    { t }

    /// A terminator for the text part of an external link.
    ///
    /// ```wikitext
    /// [//example.com External site]
    ///                             ^
    /// [//example.com Broken� link]
    ///                      ^
    /// ```
    rule external_link_term()
    = [']'|'\x00'..='\x08'|'\x0a'..='\x1f'|char::REPLACEMENT_CHARACTER]

    /// A character sequence that is interpreted as a link to a resource.
    rule magic_link() -> Spanned<Token>
    = &boundary() t:magic_link_item()
    { t }

    /// The list of possible magic link expressions.
    rule magic_link_item() -> Spanned<Token>
    = magic_auto_link()
    / magic_pmid()
    / magic_rfc()
    / magic_isbn()

    /// A bare URL to be automatically converted into a hyperlink.
    ///
    /// ```wikitext
    /// Bare http://example.com/path?query#hash?
    ///      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// In brackets (http://example.com)
    ///              ^^^^^^^^^^^^^^^^^^
    /// With brackets (http://example.com/p)(ath#hash)
    ///                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule magic_auto_link() -> Spanned<Token>
    = spanned(<
        p:position!()
        s:(!"//" t:url_scheme() { t })
        h:(url_ipish() / magic_class(<")" {}>))
        rest:magic_class(<magic_class_bracket_term(p) {}>)*
        { Token::Autolink(reduce_tree([s, h].into_iter().chain(rest))) }
    >)

    /// A valid character in an magic link URL.
    rule magic_class(term: rule<()>) -> Spanned<Token>
    = !(([','|';'|'\\'|'.'|':'|'!'|'?'] / term())* url_term())
      t:inline_in_url()
      !assert(matches!(t.node, Token::Entity('<' | '>' | '\u{00a0}')), "&lt; &gt; or &nbsp;")
    { t }

    /// A terminator for magic URLs that maybe end with a `)`.
    rule magic_class_bracket_term(p: usize)
    = ")" &url_term() #{|input, pos| {
        if input[p..pos].contains('(') {
            RuleResult::Failed
        } else {
            RuleResult::Matched(pos, ())
        }
    }}

    /// A reference to a PubMed article.
    ///
    /// ```wikitext
    /// PMID 1234
    /// ```
    rule magic_pmid() -> Spanned<Token>
    = spanned(<
        &assert(o.config.magic_links.pmid, "PMID enabled")
        "PMID" magic_space()+ id:spanned(<digit()+ {}>) &boundary()
        { Token::MagicLink(MagicLink::Pmid(id.span)) }
    >)

    /// A reference to an RFC.
    ///
    /// ```wikitext
    /// RFC 42
    /// ```
    rule magic_rfc() -> Spanned<Token>
    = spanned(<
        &assert(o.config.magic_links.rfc, "RFC enabled")
        "RFC" magic_space()+ id:spanned(<digit()+ {}>) &boundary()
        { Token::MagicLink(MagicLink::Rfc(id.span)) }
    >)

    /// A reference to an ISBN.
    ///
    /// ```wikitext
    /// ISBN 978-3-16-148410-0
    /// ```
    rule magic_isbn() -> Spanned<Token>
    = spanned(<
        &assert(o.config.magic_links.rfc, "ISBN enabled")
        "ISBN" magic_space()+ id:$(magic_isbn_id()) &boundary()
        {
            Token::MagicLink(MagicLink::Isbn({
                let id = html_escape::decode_html_entities(id);
                strtr(&id, &[("\u{00a0}", " "), ("x", "X")]).into_owned()
            }))
        }
    >)

    /// An ISBN identifier.
    ///
    /// ```wikitext
    /// ISBN 978-3-16-148410-0
    ///      ^^^^^^^^^^^^^^^^^
    /// ```
    rule magic_isbn_id()
    = "97" ['8'|'9'] magic_isbn_space() magic_isbn_id_short()
    / magic_isbn_id_short()

    /// A 10-digit ISBN identifier.
    ///
    /// ```wikitext
    /// ISBN 978-0-1234-56
    ///      ^^^^^^^^^^^^^
    /// ```
    rule magic_isbn_id_short()
    = (digit() magic_isbn_space())*<9> (digit() / ['X'|'x'])

    /// Allow whitespace in an ISBN identifier.
    rule magic_isbn_space()
    = (magic_space() / "-")?

    /// Allowed whitespace in a magic link identifier.
    rule magic_space()
    = unispace()
    / e:raw_entity() &assert(*e == '\u{00a0}', "&nbsp;")

    /// The scheme part of a URL.
    ///
    /// ```text
    /// https://example.com   //example.com   mailto:test@example.com
    /// ^^^^^^^^              ^^              ^^^^^^^
    /// ```
    rule url_scheme() -> Spanned<Token>
    = spanned(<#{|input, pos| {
        o.config.protocols_pattern.find(&input[pos..]).map_or(RuleResult::Failed, |scheme| {
            RuleResult::Matched(pos + scheme.end(), Token::Text)
        })
    }}>)

    /// A lazy match for an IPv4 or IPv6 host.
    ///
    /// ```text
    /// https://127.0.0.1   //[::1]
    ///         ^^^^^^^^^     ^^^^^
    /// ```
    rule url_ipish() -> Spanned<Token>
    = spanned(<(
        "[" ['0'..='9'|'A'..='F'|'a'..='f'|':'|'.']+ "]"
      / ['0'..='9'|'.']+
    ) { Token::Text }>)

    /// A terminator for any URL in Wikitext.
    rule url_term()
    = unispace()
    / eof()
    / behavior_switch()
      // In the original parser, text styles had already been converted to HTML
      // before parsing an external link, so would be excluded due to being an
      // HTML tag
    / text_style()
    / strip_marker()
    / [']'|'['|'<'|'>'|'"'|'\x00'..='\x20'|char::REPLACEMENT_CHARACTER]

    ///////////////
    // Templates //
    ///////////////

    /// A template expansion, template parameter, or literal `"{"`.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    ///
    /// {{{parameter_name|default}}}
    /// ```
    ///
    /// For runs of `{`, the longest right-most token wins:
    ///
    /// 1 `{`
    /// 2 `{{`
    /// 3 `{{{`
    /// 4 `{ {{{` (unless there is no `}}}`, then `{{ {{`)
    /// 5 `{{ {{{`
    /// 6 `{{{ {{{` (unless there is no `}}}`, then `{{ {{ {{`)
    /// 7 `{ {{{ {{{`
    /// 8 `{{ {{{ {{{`
    ///
    /// TODO: This is not working correctly in all cases because the original
    /// parser actually seems to work by counting up and down. So
    /// `{{{{1x|1}}{{1x|x}}|foo}}}` should be `{{ {{ }} {{ }} }} "}"`, but using
    /// a “right-most” rule as the ABNF implies actually results in
    /// `"{" {{{ "}}" {{ }} }}}`. Neato!
    #[cache]
    rule pp_template(pp: &PreprocessorOptions) -> Vec<Spanned<Token>>
    = // 2, 5, 8, ...
      &("{{" &("{{{"* !"{")) t:template_expansion(pp) { vec![t] }
      // 3, 4 (1+3); 6 (3+3), 7; ...
    / template_parameter(pp)
      // 4 (2+2), 6 (2+?), ...
    / &"{{{{" t:template_expansion(pp) { vec![t] }
      // 1
    / t:spanned(<"{" { Token::Text }>) { vec![t] }

    /// A template parameter.
    ///
    /// ```wikitext
    /// {{{parameter_name|default}}}
    /// ```
    rule template_parameter(pp: &PreprocessorOptions) -> Vec<Spanned<Token>>
      // 4, 7, ...
    = p:spanned(<"{" &("{{{"+ !"{") { Token::Text }>)?
      // 3, 6, ...
      t:template_or_parameter(pp, PpTerm::IN_PARAMETER, <"{{{" !("{{"+ !"{")>, <"}}}">)
    {
        let t = t.map_node(|(name, arguments)| {
            let default = arguments.into_iter().next().map(|a| a.node.content);
            Token::Parameter { default, name }
        });
        p.into_iter().chain(iter::once(t)).collect()
    }

    /// A template expansion.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    /// ```
    rule template_expansion(pp: &PreprocessorOptions) -> Spanned<Token>
      // Converting `{{!}}` into a token early is a performance optimisation
    = spanned(<"{{!}}" { Token::Generated("|".into()) }>)
    / t:template_or_parameter(pp, PpTerm::IN_TEMPLATE, <"{{">, <"}}">)
    { t.map_node(|(target, arguments)| {
        Token::Template { arguments, target }
    })}

    /// A generic rule for template expansions or template parameters.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    ///
    /// {{{parameter_name|default}}}
    /// ```
    rule template_or_parameter(pp: &PreprocessorOptions, flags: PpTerm, before: rule<()>, after: rule<()>)
        -> Spanned<(Vec<Spanned<Token>>, Vec<Spanned<Argument>>)>
    = spanned(<
        before()
        name:pp_items(pp, flags | PpTerm::PIPE)
        arguments:template_argument(pp, flags)*
        after()
        { (name, arguments) }
    >)

    /// A template argument with pipe delimiter.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument |key=value}}
    ///                ^^^^^^^^^^^^^^^^^^ ^^^^^^^^^^
    /// ```
    rule template_argument(pp: &PreprocessorOptions, flags: PpTerm) -> Spanned<Argument>
    = // Keeping the delimiter outside of the argument span allows the arguments
      // list to be glued back together into text in a generic way that applies
      // to all lists of spans, instead of requiring special work to extract the
      // difference from the start of the argument and the start of its first
      // content token
      "|"
      t:spanned(<
        kv:template_argument_kv(pp, flags | PpTerm::PIPE)?
        { kv.unwrap_or(Argument { content: vec![], delimiter: None, terminator: None }) }
      >)
    { t }

    /// A template argument.
    ///
    /// ```wikitext
    /// {{Template name|numbered argument|key=value}}
    ///                 ^^^^^^^^^^^^^^^^^ ^^^^^^^^^
    /// ```
    rule template_argument_kv(pp: &PreprocessorOptions, flags: PpTerm) -> Argument
    = key:pp_items(pp, flags | PpTerm::EQUALS)
      value:(d:spanned(<"=" { Token::Text }>) v:pp_items(pp, flags) { (d, v) })?
    { make_argument(key, value) }

    //////////
    // Tags //
    //////////

    /// A comment, extension tag, inclusion control tag, or literal `"<"`.
    #[cache]
    rule pp_tag(pp: &PreprocessorOptions) -> Vec<Spanned<Token>>
    = comment()
    / t:extension_tag() { vec![t] }
    / pp_ignore(pp)
    / t:spanned(<"<" { Token::Text }>) { vec![t] }

    /// A comment block or inline comment.
    ///
    /// ```wikitext
    ///   <!-- a -->  <!-- b -->  ␤
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// inline<!-- comment -->
    ///       ^^^^^^^^^^^^^^^^
    /// ```
    rule comment() -> Vec<Spanned<Token>>
    = comment_block()
    / c:comment_tag() { vec![c] }

    /// A collection of comments at the start of a line with whitespace on
    /// either side.
    ///
    /// ```wikitext
    /// ␤  <!-- a -->  <!-- b -->  ␤
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule comment_block() -> Vec<Spanned<Token>>
      // A newline cannot be consumed at the start because it may need to be
      // consumed by the end of the previous comment block or heading
    = first:spanned(<
        &after_nl()
        inline_space()*
        c:comment_tag()
        inline_space()*
        { c }
      >)
      rest:spanned(<c:comment_tag() inline_space()* { c }>)*
      // This *must* be a newline; it does *not* apply at eof
      end:newline()
    {
      let (mut first, mut rest) = (first, rest);
      let last = rest.last_mut().unwrap_or(&mut first);
      last.span.end = end.span.end;
      iter::once(first.map_node(|node| node.node))
          .chain(rest.into_iter().map(|c| c.map_node(|c| c.node)))
          .collect()
    }

    /// An HTML comment tag.
    ///
    /// ```wikitext
    ///   <!-- a -->  <!-- b -->  ␤
    ///   ^^^^^^^^^^  ^^^^^^^^^^
    /// ```
    rule comment_tag() -> Spanned<Token>
    = spanned(<"<!--" c:spanned(<(([^'-']+ / !"-->" [_])*) { Token::Text }>) end:$("-->" / eof()) {
        Token::Comment { content: c.span, unclosed: end.is_empty() }
    }>)

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
    rule extension_tag() -> Spanned<Token>
    = spanned(<
      "<"
      name:spanned(<$(extension_tag_name())>)
      attributes:extension_tag_attributes(<"/"? ">">)
      self_closing:(c:"/"? { c.is_some() })
      ">"
      content:#{|input, pos| {
          if self_closing {
              RuleResult::Matched(pos, None)
          } else if let Some((body_end, tag_end)) = find_end_tag(&input[pos..], name.as_ref(),
            // TODO: Use a non-allocating comparator
            |a, b| to_lower(a) == to_lower(b))
          {
              RuleResult::Matched(pos + tag_end, Some(Span::new(pos as u32, (pos + body_end) as u32)))
          } else {
              RuleResult::Failed
          }
      }}
      { Token::Extension { attributes, content, name: name.span } }
    >)

    /// A configured extension tag name.
    ///
    /// ```wikitext
    /// <extension-tag>Value</extension-tag>
    ///  ^^^^^^^^^^^^^
    /// ```
    ///
    /// This list is less restrictive than HTML, which requires all tag names to
    /// be ASCII alphanumeric.
    rule extension_tag_name()
    = name:$([^' '|'\t'|'\n'|'\r'|'\x0c'|'/'|'>']+)
      &assert(
        contains_ignore_case_unicode(&o.config.extension_tags, name),
        "extension tag"
      )

    /// A list of tag attributes delimited by garbage.
    ///
    /// ```wikitext
    /// <tag-name <bogus attr="value" attr2 = value>content</tag-name>
    ///          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule extension_tag_attributes(term: rule<()>) -> Vec<Spanned<Argument>>
    = extension_tag_attribute_junk(&term)
      attributes:(
        a:extension_tag_attribute(&term)
        extension_tag_attribute_junk(&term)
        { a }
      )*
    { attributes }

    /// Non-attribute data in an attribute position that should be discarded.
    /// Normally this is just whitespace, but it’s actually a mystery box that
    /// could be almost anything. It could even be a 🛥!
    ///
    /// ```wikitext
    /// <tag-name <bogus attr="value" attr2 = value>content</tag-name>
    ///          ^^^^^^^^            ^
    /// ```
    rule extension_tag_attribute_junk(term: rule<()>)
    = (!(term() / extension_tag_attribute(&term)) [_])*

    /// A tag attribute.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2 = value>content</tag-name>
    ///           ^^^^^^^^^^^^ ^^^^^^^^^^^^^
    /// ```
    rule extension_tag_attribute(term: rule<()>) -> Spanned<Argument>
    = spanned(<
        name:attribute_name(&term)
        value:attribute_value(<spanned(<[_] { Token::Text }>)>, &term)?
      { make_attribute(name, value) }
    >)

    /// An allowed HTML5 start or end tag.
    ///
    /// ```wikitext
    /// <span dir="ltr"><not-allowed-tag></span>
    /// ^^^^^^^^^^^^^^^^                 ^^^^^^^
    /// ```
    rule html_tag() -> Spanned<Token>
    = spanned(<"<" t:(html_end_tag() / html_start_tag()) ">" { t }>)

    /// The inside of an allowed HTML5 start tag.
    ///
    /// ```wikitext
    /// <span dir="ltr"><not-allowed-tag></span>
    ///  ^^^^^^^^^^^^^^
    /// ```
    rule html_start_tag() -> Token
    = name:html_tag_name()
      (space_nl()+ / &("/"? ">"))
      attributes:html_attributes(<space_nl()* "/"? ">">)
      space_nl()*
      self_closing:"/"?
    {
        let self_closing = self_closing.is_some() | VOID_TAGS.contains(&to_ascii_lower(*name));
        Token::StartTag { attributes, name: name.span, self_closing }
    }

    /// The inside of an allowed HTML5 end tag.
    ///
    /// ```wikitext
    /// <span dir="ltr"><not-allowed-tag></span>
    ///                                   ^^^^^
    /// ```
    rule html_end_tag() -> Token
    = "/" name:html_tag_name()
      // Wikitext allows a solidus here but this is illegal in HTML
      ("/" / space_nl())*
    {
        if *name == "br" {
            Token::StartTag {
                attributes: <_>::default(),
                name: name.span,
                self_closing: true
            }
        } else {
            Token::EndTag { name: name.span }
        }
    }

    /// An allowed HTML5 tag name.
    ///
    /// ```wikitext
    /// <span dir="ltr"><not-allowed-tag></span>
    ///  ^^^^                              ^^^^
    /// ```
    rule html_tag_name() -> Spanned<&'input str>
    = name:spanned(<$(alnum()+)>)
      &assert(contains_ignore_case(&HTML5_TAGS, *name), "html tag")
    { name }

    /// A list of tag attributes delimited by garbage which may contain strip
    /// markers whose contents must also participate in the attribute parsing.
    ///
    /// Because the strip marker content must participate, only the raw list of
    /// tokens is produced. They must be converted to attribute lists later by
    /// unstripping the markers.
    ///
    /// ```wikitext
    /// <tag-name <bogus attr="value" attr2 = value>content</tag-name>
    ///          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule html_attributes(term: rule<()>) -> Vec<Spanned<Token>>
    = attributes:(!term() t:(inline_in_attr() / newline()) { t })*
    { reduce_tree(attributes) }

    /// A list of HTML tag attributes delimited by garbage, after unstripping
    /// strip markers.
    ///
    /// ```wikitext
    /// <tag-name <bogus attr="value" attr2 = value>content</tag-name>
    ///          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub rule late_attributes() -> Vec<Spanned<Argument>>
    = late_attribute_junk()
      attributes:(a:late_attribute() late_attribute_junk() { a })*
    { attributes }

    /// Non-attribute data in an attribute position that should be discarded.
    /// Normally this is just whitespace, but it’s actually a mystery box that
    /// could be almost anything. It could even be a 🛥!
    ///
    /// ```wikitext
    /// <tag-name <bogus attr="value" attr2 = value>content</tag-name>
    ///          ^^^^^^^^            ^
    /// ```
    rule late_attribute_junk()
    = (!late_attribute() [_])*

    /// A tag attribute.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2 = value>content</tag-name>
    ///           ^^^^^^^^^^^^ ^^^^^^^^^^^^^
    /// ```
    rule late_attribute() -> Spanned<Argument>
    = spanned(<
        name:attribute_name(<![_]>)
        value:attribute_value(<inline_in_late_attr()>, <![_]>)?
      { make_attribute(name, value) }
    >)

    /// An attribute name.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2 = value>content</tag-name>
    ///           ^^^^         ^^^^^
    /// ```
    rule attribute_name(term: rule<()>) -> Spanned<Token>
    = spanned(<attribute_name_first(&term) (!"=" attribute_name_first(&term))* { Token::Text }>)

    /// The first character of an attribute name.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2 = value>content</tag-name>
    ///           ^
    /// ```
    rule attribute_name_first(term: rule<()>)
      // Technically this is supposed to exclude `\x0b`, but that is almost
      // certainly a bug in the original that nobody ever hits
    = !(term() / space_nl() / ['/'|'>']) [_]

    /// An attribute value delimiter.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2 = value>content</tag-name>
    ///               ^             ^^^
    /// ```
    rule attribute_delimiter()
    = space_nl()* "=" space_nl()*

    /// The value part of a tag attribute.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2 = value>content</tag-name>
    ///               ^^^^^^^       ^^^^^^^^
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
    rule attribute_value(item: rule<Spanned<Token>>, term: rule<()>) -> AttributeValue
    = attribute_value_quoted(&item, &term)
    / attribute_value_unquoted(&item, &term)

    /// A quoted value part of a tag attribute.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2 = value>content</tag-name>
    ///               ^^^^^^^
    /// ```
    rule attribute_value_quoted(item: rule<Spanned<Token>>, term: rule<()>) -> AttributeValue
    = start:spanned(<attribute_delimiter() t:['\''|'"'] { t }>)
      value:(!(term() / [c if c == *start]) t:(item() / newline()) { t })*
      end:(
        &term() { None }
        / t:spanned(<[c if c == *start] { Token::Text }>) { Some(t) }
      )
    { (start.map_node(|_| Token::Text), reduce_tree(value), end) }

    /// An unquoted value part of a tag attribute.
    ///
    /// ```wikitext
    /// <tag-name attr="value" attr2 = value>content</tag-name>
    ///                              ^^^^^^^
    /// ```
    rule attribute_value_unquoted(item: rule<Spanned<Token>>, term: rule<()>) -> AttributeValue
    = start:spanned(<attribute_delimiter() { Token::Text }>)
      value:(!term() !space_nl() t:item() { t })*
    { (start, reduce_tree(value), None) }

    ///////////////////////
    // Inclusion control //
    ///////////////////////

    /// A `noinclude`, `includeonly`, or `onlyinclude` tag.
    ///
    /// ```wikitext
    /// <includeonly>a</includeonly> <noinclude>b</noinclude>
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ ^^^^^^^^^^^ ^^^^^^^^^^^^
    /// ```
    ///
    /// The behaviour of `<noinclude>` and `<includeonly>` is self-explanatory.
    /// Use the inner contents only when one Wikitext is not included by another
    /// Wikitext using a template, and vice-versa.
    ///
    /// `<onlyinclude>` is an insane thing. Content inside `<onlyinclude>`
    /// appears regardless of whether the page is included or not, and when the
    /// page *is* included, anything outside of `<onlyinclude>` is treated as if
    /// it were wrapped by `<noinclude>` (including any `<includeonly>` tags).
    ///
    /// It is impossible to parse a Wikitext document correctly without cleaving
    /// out the includes immediately because the position of these tokens is
    /// unrestricted in the Wikitext source and so it is legal to, for example,
    /// define different numbers of arguments for a template depending on
    /// whether or not the document is being included.
    rule pp_ignore(pp: &PreprocessorOptions) -> Vec<Spanned<Token>>
    = t:inclusion_control_tag()
      t:#{|input, pos| make_include(o, pp, input, pos, t)}
    { t }

    /// An inclusion control tag.
    rule inclusion_control_tag() -> InclusionControlTag
    = spanned(<
      "<"
      is_end:(e:"/"? { e.is_some() })
      mode:spanned(<inclusion_control_tag_name()>)
      // These tags do not have attributes but the grammar requires allowing
      // junk in this position (though not always the `<onlyinclude>` tags;
      // this will be treated as UB here)
      &(space_nl() / "/"? ">") ([^'/'|'>']+ / "/" !">")*
      self_closing:(c:"/"? ">" { c.is_some() })
      { (mode, self_closing, is_end) }
    >)

    /// An inclusion control tag name.
    rule inclusion_control_tag_name() -> InclusionMode
    = i("includeonly") { InclusionMode::IncludeOnly }
    / i("noinclude") { InclusionMode::NoInclude }
    / i("onlyinclude") { InclusionMode::OnlyInclude }

    /// The content of an `<onlyinclude>` tag.
    ///
    /// ```wikitext
    /// <onlyinclude>inner {{content}}</onlyinclude>
    ///              ^^^^^^^^^^^^^^^^^
    /// ```
    pub(super) rule only_include(pp: &PreprocessorOptions, start_at: usize) -> Vec<Spanned<Token>>
    = #{|_, _| RuleResult::Matched(start_at, ()) }
      t:preprocess(pp)
    { t }

    /////////////////////////
    // Language conversion //
    /////////////////////////

    /// A language conversion tag.
    ///
    /// ```wikitext
    /// -{ text }-
    ///
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    /// ```
    #[cache]
    rule language_tag() -> Spanned<Token>
    = &assert(o.config.language_conversion_enabled, "language conversion enabled")
      t:spanned(<
        "-{"
        flags:(language_flags() / { LangFlags::default() })
        variants:language_rules(flags.is_raw())
        "}-"
        { Token::LangVariant {
            flags,
            variants
        } }
      >)
    { t }

    /// Language conversion flags.
    ///
    /// ```wikitext
    /// -{ flag1 ; flag2 | ... }-
    ///    ^^^^^^^^^^^^^
    /// ```
    rule language_flags() -> LangFlags
    = flags:language_flag() ** ";" "|"
    { make_flags(flags) }

    /// Language conversion rules.
    ///
    /// ```wikitext
    /// -{ text }-
    ///   ^^^^^^
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                   ^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule language_rules(is_raw: bool) -> Vec<LangVariant>
    = assert(is_raw, "") text:language_inline()*
    { vec![LangVariant::Text { text: reduce_tree(text) }] }
    / !assert(is_raw, "")
      variants:language_rule() ** ";"
      space_nl()* ";"? space_nl()*
    { variants }

    /// Expressions allowed in the text parts of a conversion tag.
    ///
    /// ```wikitext
    /// -{ text }-
    ///    ^^^^
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///                      ^^^^^              ^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                    ^^^^              ^^
    /// ```
    rule language_inline() -> Spanned<Token>
    = !"}-" t:(inline_in_tag() / newline())
    { t }

    /// A language conversion rule.
    ///
    /// ```wikitext
    /// -{ text }-
    ///   ^^^^^^
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///          ^^^^^^^^^^^^^^^^^^ ^^^^^^^^^^^^^^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                   ^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    rule language_rule() -> LangVariant
    = uni:language_rule_uni()?
      bidi:language_rule_bidi()
    {
        let (lang, text) = bidi;
        if let Some(from) = uni {
            LangVariant::OneWay { from, lang, to: text }
        } else {
            LangVariant::TwoWay { lang, text }
        }
    }

    /// The language code to text mapping part of a language conversion.
    ///
    /// ```wikitext
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///           ^^^^^^^^^^^^^^^^   ^^^^^^^^^^^^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                            ^^^^^^^^^^^^
    /// ```
    rule language_rule_bidi() -> (Span, Vec<Spanned<Token>>)
    = variant:bidi_variant()
      space_nl()*
      to:(!(space_nl()* ";") t:language_inline() { t })*
      space_nl()*
    { (variant, reduce_tree(to)) }

    /// The language code part of a bidirectional language conversion.
    ///
    /// ```wikitext
    /// -{ flag | variant1 : text1 ; variant2 : text2 ; }-
    ///           ^^^^^^^^^^         ^^^^^^^^^^
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                            ^^^^^^^^^
    /// ```
    rule bidi_variant() -> Span
    = space_nl()*
      variant:spanned(<language_code() {}>)
      space_nl()*
      ":"
    { variant.span }

    /// The source text part of a unidirectional language conversion.
    ///
    /// ```wikitext
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                    ^^^^^^^
    /// ```
    rule language_rule_uni() -> Vec<Spanned<Token>>
    = space_nl()*
      from:(!(";" / to_arrow() bidi_variant()) t:language_inline() { t })*
      space_nl()*
      to_arrow()
    { reduce_tree(from) }

    /// The arrow token for a unidirectional language conversion.
    ///
    /// ```wikitext
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///                         ^^
    /// ```
    rule to_arrow()
      // Technically, someone could have pre-encoded `&gt;` into a document
    = "=" (">" / "&gt;")

    /// Matches a language code registered for language conversion.
    rule language_code()
    = #{|input, pos| {
        if let Some(result) = o.lang.find(&input[pos..]) {
            RuleResult::Matched(pos + result.end(), ())
        } else {
            RuleResult::Failed
        }
    }}

    /// A raw representation of a language variant flag.
    ///
    /// ```wikitext
    /// -{ flag1 ; flag2 | from => variant : to ; }-
    ///   ^^^^^^^ ^^^^^^^
    /// ```
    rule language_flag() -> VariantFlag
    = space_nl()*
      flag:(
        f:['A'|'T'|'R'|'D'|'-'|'H'|'N'] { VariantFlag::Flag(f) }
      / f:spanned(<(alnum() {} / "-")+>) { VariantFlag::Name(f.span) }
      )
      space_nl()*
    { flag }

    //////////
    // Text //
    //////////

    /// A behavior switch.
    ///
    /// ```wikitext
    /// __TOC__
    /// ```
    ///
    /// <https://www.mediawiki.org/wiki/Help:Magic_words#Behavior_switches>
    rule behavior_switch() -> Spanned<Token>
    = spanned(<#{|input, pos| {
        if let Some(alias) = o.bs.find(&input[pos..]) {
            let name = resolve_alias_ignore_case(
                &o.config.behavior_switch_words,
                alias.as_str()
            );

            name.map_or(RuleResult::Failed, |name| RuleResult::Matched(
                pos + alias.len(),
                Token::BehaviorSwitch { name }
            ))
        } else {
            RuleResult::Failed
        }
    }}>)

    /// An HTML entity.
    rule entity() -> Spanned<Token>
    = e:raw_entity()
      assert(matches!(*e,
         '\t'|'\n'
        |'\x20'..='\x7e'
        |'\u{00a0}'..='\u{d7ff}'
        |'\u{e000}'..='\u{fffd}'
        |'\u{10000}'..='\u{10ffff}'
      ), "allowed entity")
    { e.map_node(Token::Entity) }

    /// A decoded HTML entity using the MediaWiki rules, which require a
    /// `;` terminator, even for entities with optional terminators in HTML.
    rule raw_entity() -> Spanned<char>
    = spanned(<
        m:$("&" (['#'|'0'..='9'|'a'..='z'|'A'..='Z']+ / "רלמ" / "رلم") ";")
        {?
            if m == "&רלמ;" || m == "&رلم;" {
                Ok('\u{200f}')
            } else {
                let s = html_escape::decode_html_entities(m);
                if s == m {
                    // &<not-an-entity>;
                    Err("entity")
                } else {
                    s.chars().next().ok_or("entity")
                }
            }
        }
    >)

    /// Characters which may exist in the input but which are illegal in
    /// Wikitext.
    rule pp_illegal() -> Spanned<Token>
    = spanned(<"\0"+ { Token::Generated(<_>::default()) }>)
    / spanned(<t:$("\x7f"+) { Token::Generated("?".repeat(t.len())) }>)

    /// An extension tag which has been replaced by a strip marker.
    rule strip_marker() -> Spanned<Token>
    = spanned(<
        s(MARKER_PREFIX) id:spanned(<(!s(MARKER_SUFFIX) [_])+>) s(MARKER_SUFFIX)
        { Token::StripMarker(id.span) }
    >)

    /// A plain text character on a single line.
    rule text() -> Spanned<Token>
    = spanned(<!nl() [_] { Token::Text }>)

    /// A bold or italic text style.
    ///
    /// ```wikitext
    /// ''italic'' '''bold''' '''''bold and italic'''''
    /// ^^      ^^ ^^^    ^^^ ^^^^^               ^^^^^
    /// ```
    ///
    /// Later processing relies on newline tokens being emitted for each line of
    /// text to balance quotes per line.
    rule text_style() -> Spanned<Token>
    = spanned(<
        q:$("'''''" / "'''" / "''") !"'"
        t:#{|input, pos| {
            let at = pos - q.len();
            RuleResult::Matched(pos, Token::TextStyle(match q.len() {
                2 => TextStyle::Italic,
                3 => TextStyle::Bold({
                    if at > 0 && input.as_bytes()[at - 1] == b' ' {
                        TextStylePosition::Space
                    } else if at > 1 && input.as_bytes()[at - 2] == b' ' {
                        TextStylePosition::Orphan
                    } else {
                        TextStylePosition::Normal
                    }
                }),
                5 => TextStyle::BoldItalic,
                _ => unreachable!(),
            }))
        }}
        { t }
    >)

    ///////////
    // Atoms //
    ///////////

    /// A positive lookbehind for a newline.
    rule after_nl()
    = #{|input, pos| if pos != 0 && input.as_bytes()[pos - 1] == b'\n' {
        RuleResult::Matched(pos, ())
    } else {
        RuleResult::Failed
    }}

    /// ASCII alphanumerics.
    rule alnum()
    = alpha() / digit()

    /// ASCII alphabetics.
    rule alpha()
    = ['A'..='Z'|'a'..='z']

    /// Asserts a precondition given by `cond`.
    rule assert(cond: bool, msg: &'static str)
    = {? if cond { Ok(()) } else { Err(msg) }}

    /// A positive lookbehind for a newline or start of input.
    rule at_sol()
    = #{|input, pos| if pos == 0 || input.as_bytes()[pos - 1] == b'\n' {
        RuleResult::Matched(pos, ())
    } else {
        RuleResult::Failed
    }}

    /// A text boundary, equivalent to PCRE `\b` in Unicode mode.
    rule boundary()
    = #{|input, pos| {
        let prev_word = pos != 0 && input[..pos].chars().next_back().is_some_and(is_word);
        let at_word = input[pos..].chars().next().is_some_and(is_word);
        if prev_word == at_word {
            RuleResult::Failed
        } else {
            RuleResult::Matched(pos, ())
        }
    }}

    /// ASCII digits.
    rule digit()
    = ['0'..='9']

    /// A positive lookahead for the end of input.
    rule eof() = ![_]

    /// A newline or end of input.
    rule eolf() = eof() / nl()

    /// A case-insensitive literal match.
    rule i(lit: &'static str)
    = quiet!{
        #{|input, pos| {
          let end = pos + lit.len();
          if input.get(pos..end).is_some_and(|input| input.eq_ignore_ascii_case(lit)) {
              RuleResult::Matched(end, ())
          } else {
              RuleResult::Failed
          }
        }}
    } / expected!(lit)

    /// A newline token.
    rule newline() -> Spanned<Token>
    = spanned(<nl() { Token::NewLine }>)

    /// A newline.
    rule nl() = "\r"? "\n"

    /// A case-sensitive literal match.
    rule s(lit: &'static str)
    = quiet!{
        #{|input, pos| {
          let end = pos + lit.len();
          if input.get(pos..end) == Some(lit) {
              RuleResult::Matched(end, ())
          } else {
              RuleResult::Failed
          }
        }}
    } / expected!(lit)

    /// Horizontal whitespace.
    rule inline_space() = [' '|'\t']

    /// A match of the PCRE `\s` character class in ASCII mode.
    rule space_nl() = [' '|'\t'|'\n'|'\x0b'|'\x0c'|'\r']

    /// A match of the PCRE `\s` character class  in ASCII mode, without
    /// newlines.
    rule space_s() = [' '|'\t'|'\x0b'|'\x0c'] / "\r" !"\n"

    /// Wraps some `T` in a span.
    rule spanned<T>(r: rule<T>) -> Spanned<T>
    = start:position!() node:r() end:position!()
    { Spanned::new(node, u32::try_from(start).unwrap(), u32::try_from(end).unwrap()) }

    /// A match of the PCRE `\p{Zs}` character class.
    rule unispace()
    = [c if get_general_category(c) == GeneralCategory::SpaceSeparator]
}}

/// Asserts that the text at `&input[start..end]` is a match for a local
/// namespace with the given `id`.
fn assert_namespace(
    o: &Parser<'_>,
    input: &str,
    start: usize,
    end: usize,
    id: i32,
) -> RuleResult<()> {
    // TODO: If it does not feel like death to get the root article name
    // passed in here, this could just parse and emit the whole title right
    // now instead of having to double-parse it.
    let title = title_decode(&input[start..end]);
    let is_match = title.find(':').is_some_and(|delim| {
        let part = title::normalize(&title[..delim]);
        let part = part.trim_matches(' ');

        Namespace::find_by_name(o.config, part).is_some_and(|ns| ns.id == id)
            || (id == Namespace::CATEGORY
                && o.config
                    .interlanguage_map
                    .contains_key(&to_ascii_lower(part)))
    });
    if is_match {
        RuleResult::Matched(end, ())
    } else {
        RuleResult::Failed
    }
}

/// Balances text style tokens by decomposing the first ‘best’ bold style in a
/// line into an italic style if there are odd numbers of both.
fn balance_quotes(mut tokens: Vec<Spanned<Token>>) -> Vec<Spanned<Token>> {
    let mut balancer = TextStyleBalancer::default();
    balancer.count(&mut tokens);
    balancer.finish();
    tokens
}

/// Returns true if any `candidates` case-insensitively match `value`.
#[inline]
fn contains_ignore_case(candidates: &phf::Set<&str>, value: &str) -> bool {
    // TODO: Use a case-insensitive hashable type instead of allocating.
    candidates.contains(&to_ascii_lower(value))
}

/// Returns true if any `candidates` case-insensitively match `value`.
#[inline]
fn contains_ignore_case_unicode(candidates: &phf::Set<&str>, value: &str) -> bool {
    // TODO: Use a case-insensitive hashable type instead of allocating.
    candidates.contains(&to_lower(value))
}

/// Finds the start and end position of the next end tag which matches the given
/// tag name somewhere in the given input, as determined by `comparator`.
///
/// This avoids the overhead of compiling and/or caching regular expressions
/// for every possible tag, without bothering to first check that any such
/// overhead exists or matters. :-)
fn find_end_tag(
    input: &str,
    tag_name: &str,
    mut comparator: impl FnMut(&str, &str) -> bool,
) -> Option<(usize, usize)> {
    // There is no point in checking for a closing tag beyond the point where
    // it would be possible for a closing tag with this `tag_name` to exist in
    // the input
    let max_start = input.len().saturating_sub(tag_name.len() + ">".len());

    let bytes = input.as_bytes();
    memchr::memmem::find_iter(&bytes[..max_start], b"</").find_map(|start| {
        let tag_name_start = start + "</".len();
        let tag_name_end = tag_name_start + tag_name.len();
        // `tag_name_end` cannot be beyond `input` but it might be in the middle
        // of a UTF-8 code sequence, so this has to be a fallible check
        if let Some(slice) = input.get(tag_name_start..tag_name_end)
            && comparator(slice, tag_name)
            && let Some(tag_end) =
                memchr::memchr(b'>', &bytes[tag_name_end..]).map(|e| tag_name_end + e)
            && bytes[tag_name_end..tag_end]
                .iter()
                .all(u8::is_ascii_whitespace)
        {
            Some((start, tag_end + ">".len()))
        } else {
            None
        }
    })
}

/// Returns true if the given `c` is a Unicode PCRE word character.
#[allow(
    clippy::allow_attributes,
    reason = "https://github.com/rust-lang/rust-clippy/issues/13358"
)]
fn is_word(c: char) -> bool {
    #[allow(clippy::enum_glob_use, reason = "my fingers hurt")]
    use GeneralCategory::*;
    c == '_'
        || matches!(
            get_general_category(c),
            LowercaseLetter
                | ModifierLetter
                | OtherLetter
                | TitlecaseLetter
                | UppercaseLetter
                | DecimalNumber
                | LetterNumber
                | OtherNumber
        )
}

/// The intermediate type of a parsed inclusion control tag.
type InclusionControlTag = Spanned<(Spanned<InclusionMode>, bool, bool)>;

/// Creates an [`Argument`] for a template or Wikilink from the given `key` and
/// optional `value`.
fn make_argument(
    key: Vec<Spanned<Token>>,
    value: Option<(Spanned<Token>, Vec<Spanned<Token>>)>,
) -> Argument {
    let key = reduce_tree(key);
    if let Some((d, value)) = value {
        let delimiter = Some(key.len());
        let content = key
            .into_iter()
            .chain(iter::once(d))
            .chain(reduce_tree(value))
            .collect();
        Argument {
            content,
            delimiter,
            terminator: None,
        }
    } else {
        Argument {
            content: key,
            delimiter: None,
            terminator: None,
        }
    }
}

/// The intermediate type of a parsed XML attribute.
type AttributeValue = (Spanned<Token>, Vec<Spanned<Token>>, Option<Spanned<Token>>);

/// Creates an [`Argument`] for an XML attribute from the given `name` and
/// optional `value`.
fn make_attribute(name: Spanned<Token>, value: Option<AttributeValue>) -> Argument {
    let delimiter = Some(1);
    if let Some((delimiter_token, value, end_quote)) = value {
        let mut content = [name, delimiter_token]
            .into_iter()
            .chain(value)
            .collect::<Vec<_>>();
        let terminator = end_quote.as_ref().map(|_| content.len());
        content.extend(end_quote);

        Argument {
            content,
            delimiter,
            terminator,
        }
    } else {
        Argument {
            content: vec![name],
            delimiter,
            terminator: None,
        }
    }
}

/// Converts a list of intermediate language conversion flags to their final
/// form.
fn make_flags(f: Vec<VariantFlag>) -> LangFlags {
    let mut flags = CommonLangFlags::empty();
    let mut variants = HashSet::new();

    let mut unknown_flag = false;
    let flag_count = f.len();
    for item in f {
        match item {
            VariantFlag::Flag('A') => {
                flags |= CommonLangFlags::ADD | CommonLangFlags::SHOW;
            }
            VariantFlag::Flag('D') => {
                flags -= CommonLangFlags::SHOW;
            }
            VariantFlag::Flag('H') => {
                flags &= CommonLangFlags::TITLE | CommonLangFlags::DESCRIBE;
                flags |= CommonLangFlags::HOLD_IT_IN | CommonLangFlags::ADD;
            }
            VariantFlag::Flag('N') => {
                flags = CommonLangFlags::NAME;
            }
            VariantFlag::Flag('R') => {
                flags = CommonLangFlags::RAW;
            }
            VariantFlag::Flag('T') => {
                flags |= CommonLangFlags::TITLE;
                if flag_count == 1 {
                    flags |= CommonLangFlags::HOLD_IT_IN;
                }
            }
            VariantFlag::Flag('-') => {
                flags = CommonLangFlags::REMOVE;
            }
            VariantFlag::Flag(_) => {
                unknown_flag = true;
            }
            VariantFlag::Name(n) => {
                variants.insert(n);
            }
        }
    }

    if variants.is_empty() {
        if flags.is_empty() && !unknown_flag {
            flags = CommonLangFlags::SHOW;
        }

        LangFlags::Common(flags)
    } else {
        // If the current display language is in this list, or any of the
        // fallbacks of the display language are in this list, then converts
        // the value and acts af the common flags were `R`
        LangFlags::Combined(variants)
    }
}

/// Creates a balanced heading token from the given `start` run of `=` and
/// optional content and end run of `=`.
///
/// This is not efficiently representable in a simple grammar because it is
/// possible that a header is all `=`.
fn make_heading(start: Span, ce: Option<(Vec<Spanned<Token>>, Span)>) -> Token {
    let (s, c, e, level) = if let Some((c, e)) = ce {
        (start, c, e, start.len().min(e.len()))
    } else {
        let level = (start.len() - 1) / 2;
        let e = Span::new(start.start + level, start.end);
        let s = Span::new(start.start, start.start + level);
        (s, vec![], e, level)
    };
    let level = level.min(6);

    let extra_left = (s.len() > level).then(|| {
        let delta = s.len() - level;
        Spanned::new(Token::Text, s.end - delta, s.end)
    });

    let extra_right = (e.len() > level).then(|| {
        let delta = e.len() - level;
        Spanned::new(Token::Text, e.start, e.start + delta)
    });

    let content = extra_left.into_iter().chain(c).chain(extra_right).collect();

    if let Ok(Ok(level)) = u8::try_from(level).map(HeadingLevel::try_from) {
        Token::Heading { level, content }
    } else {
        panic!("calculated level {level} from s = {s:?}, e = {e:?}");
    }
}

/// Wraps or discards an inclusion control tag and its contents according to
/// the [`Options::including`] disposition of the input.
///
/// Unlike the PHP preprocessor, this one simply makes the content disappear by
/// excluding it from the AST entirely.
fn make_include(
    o: &Parser<'_>,
    pp: &PreprocessorOptions,
    input: &str,
    pos: usize,
    t: InclusionControlTag,
) -> RuleResult<Vec<Spanned<Token>>> {
    let (mode, self_closing, is_end) = t.node;
    let tag = t.map_node(|_| {
        if is_end {
            Token::EndInclude(*mode)
        } else {
            Token::StartInclude(*mode)
        }
    });

    if mode.node == InclusionMode::OnlyInclude {
        pp.has_onlyinclude.set(true);
    }

    let (body_end, end) = if self_closing || is_end {
        (pos, pos)
    } else {
        find_end_tag(
            &input[pos..],
            &input[mode.span.into_range()],
            str::eq_ignore_ascii_case,
        )
        .map_or((input.len(), input.len()), |(end_start, end_end)| {
            (pos + end_start, pos + end_end)
        })
    };

    match (pp.including, mode.node, is_end) {
        (true, InclusionMode::IncludeOnly, _)
        | (false, InclusionMode::NoInclude | InclusionMode::OnlyInclude, _)
        | (true, InclusionMode::OnlyInclude, true) => {
            // Discard the tag and keep parsing
            RuleResult::Matched(pos, vec![])
        }
        (true, InclusionMode::NoInclude, _) | (false, InclusionMode::IncludeOnly, false) => {
            // Discard the tag and its body
            RuleResult::Matched(end, vec![])
        }
        (false, InclusionMode::IncludeOnly, true) => {
            // Oops, somebody did a bug and now we must all live with that
            // mistake for forever
            RuleResult::Matched(pos, vec![tag.map_node(|_| Token::Text)])
        }
        (true, InclusionMode::OnlyInclude, false) => {
            // The content inside `<onlyinclude>` needs to be parsed separately
            // because it should be parsed as if the earlier content never
            // existed at all, but the parser is already in the middle of some
            // context that belongs to that other content. To make sure that the
            // span positions are correct, this uses a special rule which starts
            // at 0 and immediately skips to `pos`, continuing until `body_end`.
            let inner = wikitext::only_include(&input[..body_end], o, pp, pos).unwrap_or_default();
            let end_tag = Spanned::new(Token::EndInclude(*mode), body_end as u32, end as u32);
            RuleResult::Matched(
                end,
                iter::once(tag)
                    .chain(inner)
                    .chain(iter::once(end_tag))
                    .collect(),
            )
        }
    }
}

/// Consumes the next valid byte sequence that corresponds to a valid UTF-8
/// character.
fn wikilink_target_char(input: &str, start: usize, valid: &BitMap) -> RuleResult<()> {
    fn valid_ltgt(valid: &BitMap, b: u8) -> bool {
        // In the original parser, '<' and '>' are accepted in this position
        // even when they are not in the valid bytes list because these
        // characters would’ve been replaced with HTML entities in a previous
        // step
        if b == b'<' {
            b"&lt;".iter().copied().all(|b| valid.contains(b))
        } else if b == b'>' {
            b"&gt;".iter().copied().all(|b| valid.contains(b))
        } else {
            false
        }
    }

    let bytes = input.as_bytes();
    let mut pos = start;
    while pos < bytes.len()
        && let b = bytes[pos]
        && (matches!(b, b'#' | b'%') || valid.contains(b) || valid_ltgt(valid, b))
    {
        pos += 1;
        if input.is_char_boundary(pos) {
            break;
        }
    }

    if pos != start && input.is_char_boundary(pos) {
        RuleResult::Matched(pos, ())
    } else {
        RuleResult::Failed
    }
}

/// Decays inline Wikitext `<dd>` bullets in illegal positions back to plain
/// text.
fn reduce_dd(content: Vec<Spanned<Token>>) -> Vec<Spanned<Token>> {
    // Because text styles were converted to HTML tags before this algorithm ran
    // in the original parser, it is necessary to balance the text styles first
    // to track the inner/outer count correctly. Also, the original parser used
    // a dumb algorithm where tags were not matched, just counted, so it is not
    // necessary to do anything smart here to deal with the discrepancy
    let mut content = balance_quotes(content);

    let mut tag_count = 0;
    let mut bold = false;
    let mut italic = false;

    for token in &mut content {
        if let Token::StartTag { self_closing, .. } = &token.node {
            tag_count += u32::from(!self_closing);
        } else if let Token::EndTag { .. } = &token.node {
            tag_count = tag_count.saturating_sub(1);
        } else if let Token::TextStyle(style) = &token.node {
            match style {
                TextStyle::Bold(_) => bold = !bold,
                TextStyle::BoldItalic => {
                    bold = !bold;
                    italic = !italic;
                }
                TextStyle::Italic => {
                    italic = !italic;
                }
            }
        } else if let node @ Token::InlineListItem = &mut token.node
            && (tag_count != 0 || bold || italic)
        {
            *node = Token::Text;
        }
    }

    content
}

/// Collapses runs of text nodes into a single node and prunes empty text nodes.
fn reduce_tree(t: impl IntoIterator<Item = Spanned<Token>>) -> Vec<Spanned<Token>> {
    let mut v = Vec::<Spanned<Token>>::new();
    for token in t {
        if matches!(token.node, Token::Text)
            && let Some(Spanned { span: text_span, node: Token::Text }) = v.last_mut()
            // Text spans may be discontiguous if they are split by a discarded
            // inclusion control tag
            && text_span.end == token.span.start
        {
            *text_span = text_span.merge(token.span);
        } else if token.node != Token::Text || !token.span.is_empty() {
            v.push(token);
        }
    }
    v
}

/// Returns the canonical name for the given case-insensitive `alias`.
#[inline]
fn resolve_alias_ignore_case<'a>(
    candidates: &phf::Map<&str, &'a str>,
    alias: &str,
) -> Option<&'a str> {
    // TODO: Use a case-insensitive hashable type instead of allocating.
    candidates.get(&to_ascii_lower(alias)).copied()
}

bitflags::bitflags! {
    /// Preprocessor state flags to parse Wikitext using the required non-greedy
    /// rightmost-wins rules.
    ///
    /// Using flags allows normal packrat caching to be used to prevent
    /// catastrophic backtracking.
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
    struct PpTerm: u8 {
        /// Mask for exclusive in-flags.
        const IN_MASK      = Self::EQUALS.bits() - 1;
        /// Last token was `[[`, next close can only be `]]`
        const IN_LINK      = 1;
        /// Last token was `{{{`, next close can only be `}}}`
        const IN_PARAMETER = 2;
        /// Last token was `{{`, next close can only be `}}`
        const IN_TEMPLATE  = 3;
        /// Last token was `\n`, next close can only be `\n`
        const IN_HEADING   = 4;
        /// Last token was `-{`, next close can only be `}-`
        const IN_CONVERT   = 5;

        /// `=` starts a new expression.
        const EQUALS       = 1 << 3;
        /// `|` starts a new expression.
        const PIPE         = 1 << 4;
    }
}

impl PpTerm {
    /// Returns true if the parser is inside a language conversion tag.
    fn in_convert(self) -> bool {
        self & Self::IN_MASK == Self::IN_CONVERT
    }

    /// Returns true if the parser is inside a Wikitext heading.
    fn in_heading(self) -> bool {
        self & Self::IN_MASK == Self::IN_HEADING
    }

    /// Returns true if the parser is inside a Wikilink.
    fn in_link(self) -> bool {
        self & Self::IN_MASK == Self::IN_LINK
    }

    /// Returns true if the parser is inside a template parameter tag.
    fn in_parameter(self) -> bool {
        self & Self::IN_MASK == Self::IN_PARAMETER
    }

    /// Returns true if the parser is inside a template expansion tag.
    fn in_template(self) -> bool {
        self & Self::IN_MASK == Self::IN_TEMPLATE
    }
}

impl peg::Cacheable for PpTerm {
    type Cached = Self;
    type Key = Self;

    fn key(&self) -> &Self::Key {
        self
    }

    fn to_cached(&self) -> Self::Cached {
        *self
    }
}

/// A state machine for balancing text styles in a line.
#[derive(Default)]
struct TextStyleBalancer<'a> {
    /// The number of bold tokens.
    bold: u32,
    /// The best bold token to decay if there is a token mismatch after
    /// counting.
    best: Option<(&'a mut Vec<Spanned<Token>>, usize)>,
    /// The number of italic tokens.
    italic: u32,
    /// The priority of the `best` token.
    priority: u8,
}

impl TextStyleBalancer<'_> {
    /// Recursively counts text style tokens in the given token tree and finds
    /// the best victim token to decay in case of an imbalance. This will also
    /// immediately balance any styles in the content part of a [`Token::Link`],
    /// since these are supposed to be treated independently from their outer
    /// context.
    fn count(&mut self, tokens: &mut Vec<Spanned<Token>>) {
        let mut best_index = None;
        for (index, token) in tokens.iter_mut().enumerate() {
            match &mut token.node {
                Token::TextStyle(TextStyle::Bold(position)) => {
                    self.bold += 1;
                    if *position as u8 > self.priority {
                        self.priority = *position as u8;
                        best_index = Some((self.priority, index));
                    }
                }
                Token::TextStyle(TextStyle::BoldItalic) => {
                    self.bold += 1;
                    self.italic += 1;
                }
                Token::TextStyle(TextStyle::Italic) => {
                    self.italic += 1;
                }
                Token::ExternalLink { content, .. }
                | Token::Heading { content, .. }
                | Token::ListItem { content, .. }
                | Token::Line { content, .. } => {
                    self.count(content);
                }
                Token::LangVariant { variants, .. } => {
                    for variant in variants {
                        match variant {
                            LangVariant::Empty => {}
                            LangVariant::OneWay { from, to, .. } => {
                                self.count(from);
                                self.count(to);
                            }
                            LangVariant::Text { text } | LangVariant::TwoWay { text, .. } => {
                                self.count(text);
                            }
                        }
                    }
                }
                // In the original parser, quote balancing happens *after* quotes
                // have already been processed into HTML inside of Wikilinks, and
                // so the links do their own balancing
                Token::Link { content, .. } => {
                    let mut balancer = Self::default();
                    for arg in content {
                        balancer.count(&mut arg.node.content);
                    }
                    balancer.finish();
                }
                _ => {}
            }
        }

        if let Some((priority, index)) = best_index
            && self.priority == priority
        {
            // SAFETY: This very stupid lifetime erasure seems to be necessary
            // as borrowck is unable to tell that `tokens` is always borrowed
            // *once* exclusively, either itself *or* one of its children. I can
            // think of no better alternative way to do this. Index tracking is
            // awful for the potentially arbitrary nestings inside of language
            // conversion tags; a single linearised index requires walking the
            // tree twice.
            self.best = Some((
                unsafe { core::ptr::from_mut(tokens).as_mut_unchecked() },
                index,
            ));
        }
    }

    /// Finishes balancing quotes and decays a mismatched bold to italic.
    fn finish(self) {
        if self.bold & 1 != 0
            && self.italic & 1 != 0
            && let Some((victim, index)) = self.best
        {
            let Spanned {
                span,
                node: Token::TextStyle(style),
            } = &mut victim[index]
            else {
                unreachable!()
            };
            let new_token = Spanned::new(Token::Text, span.start, span.start + 1);
            span.start += 1;
            *style = TextStyle::Italic;
            victim.insert(index, new_token);
        }
    }
}

/// An intermediate representation of a language variant option.
enum VariantFlag {
    /// The option is a flag.
    Flag(char),
    /// The option is a BCP 47 language code.
    Name(Span),
}

/// HTML5 tags allowed in Wikitext.
static HTML5_TAGS: phf::Set<&str> = phf::phf_set! {
    // Explicit `<a>` tags are forbidden in Wikitext.
    "abbr",
    "b", "bdi", "bdo", "big", "blockquote", "br",
    "caption", "center", "cite", "code",
    "data", "dd", "del", "dfn", "div", "dl", "dt",
    "em",
    "font",
    "h1", "h2", "h3", "h4", "h5", "h6", "hr",
    "i", "ins",
    "kbd",
    "li",
    "mark",
    "ol",
    "p", "pre",
    "q",
    "rb", "rp", "rt", "rtc", "ruby",
    "s", "samp", "small", "span", "strike", "strong", "sub", "sup",
    "table", "td", "th", "time", "tr", "tt",
    "u", "ul",
    "var",
    "wbr",
};
