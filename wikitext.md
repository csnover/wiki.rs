# Wikitext specification (unofficial)

※ This specification document still contains some implementation-specific
  details that do not belong in a format specification.

Wikitext is conventionally UTF-8.

## Configuration

Correctly parsing Wikitext requires out-of-band configuration data to
differentiate between plain text and Wikitext tokens:

* Annotation tag names
* Double-underscore aliases
* Extension tag names
* Language conversion enabled flag
* Link prefix and suffix regular expressions
* Magic links flags
* Magic word aliases and case sensitivities
* Namespace aliases and case rules
* Parser function aliases
* Redirect keyword aliases
* Supported URI schemes
* Valid title regular expression character class
* Variable aliases
* Registered language variants and fallbacks

Correct rendering requires additional out-of-band configuration data:

* Interface message overrides
* Image domain whitelists
* Whether arbitrary HTML tags are allowed to pass through to the output
  (this information is not exposed by the siteinfo API)
* Whether the legacy or the HTML5 (or both) anchor encoding strategy should
  be used (this information is not exposed by the siteinfo API)

Both things require the blessings of at least three ancient gods.

Because content at any stage of rendering may be exposed to scripts used to
render a Wikitext document, conforming parsers also need to be more or less
conforming renderers. lol!

When designing a Wikitext parser, consider that the original format was based
around line processing using regular expressions directly against XHTML-ish
character sequences with both look-behind and look-ahead for disambiguation, and
for whitespace handling, over multiple passes, some of which emit Wikitext, some
of which emit HTML, and some of which required a DOM.

## Document conventions

This document is written mostly using syntax conventions similar to the Rust
language.

Match choices are written as `[Choice 1|Choice 2|...]`. For example,
`['A'|"B"|\s]` means to match any one of the character `'A'`, character sequence
`"B"`, or Perl-compatible regular expression character class `\s`.

Ranges bounded inclusively below and exclusively above are written as
`Start..End`. Inclusive ranges are written as `Start..=End`. Ranges only bound
on one side are written as `Start..` or `..End` or `..=End`. Taking just the
start or end of a range is written as `Range.start` or `Range.end`.

Characters are written as `'c'` and character sequences as `"cs"`. Slices of
character sequences are written as `Text[Range]`. Interpolated sequences are
written as `A{Expr}B`, where the placeholder `{Expr}` should be interpolated
with the value of the expression `Expr`. *In an interpolation*, literal `{` and
`}` are written as `{{` and `}}` and literal `"` is written as `\"`.

“Empty” is to be an empty container (string, list, etc.).

“None” is to have no value (like `None` or `NULL` or `nil`).

List literals are written as `[ Item 1, Item 2, ... ]`. Map literals are written
as `[ Key 1 => Value 1, ... ]`. Map properties are written as `Map.key`.

“Pop `Variable`” means to let `Variable` be the last item popped from a local
stack with the name `Variable`.

For avoidance of confusion, sometimes `' '` is written out as “space character”.

“ASCII whitespace” is the standard C locale
`['\t'|'\n'|'\x0b'|'\x0c'|'\r'|' ']`.

When a *configured* value is referenced (a “configured alias”, “configured URI
scheme”, etc.), this is a reference to the
[out-of-band configuration data](#configuration) required to render the
document.

---

A conforming Wikitext processor must function as if the following sequence of
steps is run in order:

<style>
.wiki-rs-step-list {
  ol ol { list-style-type: lower-roman; }
  ol ol ol { list-style-type: lower-alpha; }
  ol ol ol ol { list-style-type: upper-roman; }
  ol ol ol ol ol { list-style-type: upper-alpha; }
}
</style>
<div class="wiki-rs-step-list">

1. [Tokenize the input](#tokenize-input);
2. [Preprocess the input](#preprocess-input);
3. Delete all [comment ranges](#comment-range) in the input;
4. <a name="escape-invalid-tags"></a>
   HTML entity escape all `<` and `>` that are not part of an allowed HTML
   tag[^allowedtags];
5. [Rewrite HTML tag attributes](#html-attributes);
6. [Convert Wikitext tables to HTML](#tables);
7. [Convert Wikitext horizontal rules to HTML](#horizontal-rules);
8. [Remove double underscores](#double-underscores);
9. [Convert Wikitext headings to HTML](#headings);
10. [Convert Wikitext internal links to HTML links](#wikilinks);
11. For each line `L` in the input ending in `'\n'`:

    1. [Run text styles processing](#text-styles);
    2. If `L` is not the last line in the input, emit `'\n'`.

12. [Convert Wikitext external links to HTML](#external-links);
13. Delete all [cloaked link pseudo-strip markers](#external-link-cloaks);
14. [Convert magic links to HTML](#magic-links);
15. [Run the outline algorithm](#outlining);
16. [Unstrip strip markers](#unstrip) with mode `general`;
17. [Run the block level algorithm](#block-level-wikitext);
18. [Run the language converter](#language-conversion);
19. [Unstrip strip markers](#unstrip) with mode `nowiki`;
20. [Unstrip strip markers](#unstrip) with mode `general`[^gen2];
21. [Parse into an HTML5 DOM];
22. [Guard French quotation marks from word wrapping](#guard-quotes);
23. [Run the p-wrapping algorithm](#p-wrapping);
24. [Format elements](#format-elements).

[Parse into an HTML5 DOM]: https://html.spec.whatwg.org/multipage/parsing.html

[^allowedtags]: `["abbr"|"b"|"bdi"|"bdo"|"big"|"blockquote"|"br"|"caption"`
  `|"center"|"cite"|"code"|"data"|"dd"|"del"|"dfn"|"div"|"dl"|"dt"|"em"|"font"`
  `|"h1"|"h2"|"h3"|"h4"|"h5"|"h6"|"hr"|"i"|"ins"|"kbd"|"li"|"mark"|"ol"|"p"`
  `|"pre"|"q"|"rb"|"rp"|"rt"|"rtc"|"ruby"|"s"|"samp"|"small"|"span"|"strike"`
  `|"strong"|"sub"|"sup"|"table"|"td"|"th"|"time"|"tr"|"tt"|"u"|"ul"|"var"`
  `|"wbr"]`

[^gen2]: The rationale for this sequential unstripping was “inserted for
  transparent tag hooks (now deprecated) but some extensions (notably `<poem>`)
  rely on the extra `unstripGeneral()` after `unstripNoWiki()` so they can
  modify the contents of `<nowiki>` tags.”

## Tokenize input

Conceptually, the tokenizer MUST run by looking for tokens `{{{`, `{{`, `[[`,
`-{`, `<`, pushing a found token to the stack, and then continuing until the
corresponding `}}}`, `}}`, `]]`, `}-`, or `>` for the token on the top of the
stack is found. If the corresponding close-token for the top of the stack is
not found, that token is popped, the first character of the mismatched token
MUST be treated as plain text, and then the tokeniser starts over from the
next position.

Since this is obviously a design that will cause pathological backtracking
for documents with mismatched tokens, depending on what parser design is
used, the parser should either track each kind of token that reached EOF and
then never try to parse that token kind again or use some other
work-preserving mechanism like memoisation.

When processing a Wikitext document, the smallest atom is a Wikitext token,
but that the smallest atom that a template can produce is a *character*.
This means that any tokenising that happens at this point may change after
template expansion. For example, `[[{{1x|a]]}}`, where `Template:1x` is
`{{{1}}}`, tokenises to something like `"[[", Template("1x", "a]]")` before
template expansion and `Link("a")` after template expansion.

Quirks related to things like the interaction of extension tags and HTML tags
are caused by the fact that preprocessing is actually performed on a
pseudo-XML DSL overlaying a character sequence that happens to be Wikitext. It
is only just aware enough of Wikitext tokens to make it a non-generalisable
macro system that is needlessly confusing and difficult to reason about.

For example, `<pre <includeonly>attr</includeonly>></pre>` is an extension
tag, whilst `<pre<includeonly/>/>` is an HTML tag, because the preprocessor
requires that extension tag names end with ASCII whitespace or `['>'|"/>"]`,
so ends up tokenising as `Text("<pre"), Ignore(""), Text(">")`. (The first
case actually generates the perhaps even more surprising garbage
`Extension { name: "pre", attr: " <includeonly", body: ">attr</includeonly>>" }`
rather than what the author probably intended.)

The preprocessor emits a [token tree](#preprocessor-tokens).

1. Let `Input` be the input;
2. Let `Stack` be a stack of maps with the properties `open`, `close`, `parts`,
   `start`, `count`;
3. Replace all `'\0'` in `Input` with empty and all `'\x7f'` in `Input` with
   `'?'`;
4. Set the `Including` flag if `Input` is a template being
   [expanded from a template expression](#expand-templates);
5. Set the `Use Onlyinclude` and `Onlyinclude` flags if the `Including` flag is
   set and `Input` contains both of the case-sensitive character sequences
   `"<onlyinclude>"` and `"</onlyinclude>"`;

   ※ Because behaviour for tokenising extension tags changes based on the
     presence of inclusion control tags, inclusion control tags MUST NOT be
     processed before extension tags are processed.

6. Let `P` be `0`;
7. <a name="s7"></a>
   If `Onlyinclude` is set:

   1. Let `P` be the position of `Input` after the next case-sensitive match
      `"<onlyinclude>"`;
   2. If `P` is none, terminate here; otherwise
   3. Clear `Onlyinclude`.

8. If `Line Start` is set, let `Found` be `"Line Start"` and let `Char` be
   empty.

   Otherwise:

   1. Let `Search` be `["{{"|"[["|'<'|'\n']`;
   2. If language conversion is enabled in the configuration, append `"-{"` to
      `Search`;
   3. If `Find Pipe` is set, append `'|'` to `Search`;
   4. If `Find Equals` is set, append `'='` to `Search`;
   5. If `Stack` is empty, let `Close` be empty; otherwise

      1. Let `Close` be `Stack.close`;
      2. Append `Close` to `Search`.

   6. Let `N` be the next position of any match in `Search`;
   7. If `N` is not none, emit `Text(Input[P..N])`;
   8. Let `P` equal `N`;
   9. If `P` is the end of the input:

      1. If `Close` is `'\n'`, let `Char` be empty and `Found` be `"Line End"`.

         Otherwise:

         1. Emit the [broken token](#broken-token) of each item in `Stack` from
            left to right to `Root Accumulator`;
         2. Convert all `PossibleHeading` tokens in the `Root Accumulator` to
            `Heading` tokens;
         3. Terminate here.

      Otherwise:

      1. Let `Char` be the character sequence starting at `P`.

   10. If `Char` starts with `'|'`, let `Found` be `"Pipe"`; otherwise
   11. If `Char` starts with `'='`, let `Found` be `"Equal"`; otherwise
   12. If `Char` starts with `'<'`, let `Found` be `"Angle"`; otherwise
   13. If `Char` starts with `'\n'`, let `Found` be:

       1. If `In Heading` is set, `"Line End"`; otherwise
       2. `"Line Start"`.

       Otherwise;

   14. If `Char` starts with `Close`, let `Found` be `"Close"`; otherwise
   15. If `Char` starts with `['{'|'['|"-{"]`, let `Found` be `"Open"` and let
       `Rule` be the matched value from the rules[^tokenrules]; otherwise

       1. If `Char` is `['-'|'}']`, emit `Text(P..=P)`;
       2. Let `P` equal `P + 1`;
       3. Restart from [step 7](#s7) of the root list.

9. If `Found` is `"Angle"`:

   1. If `Use Onlyinclude` is set and the character sequence at `P` is a
      case-sensitive match for `</onlyinclude>`, set `Onlyinclude` and restart
      from [step 7](#s7); otherwise
   2. Let `Name` be the character sequence at `P + 1` which matches one of:

      1. A Unicode case-insensitive match of a configured annotation tag,
         extension tag, or inclusion control tag[^ictag], then
         `[\s|'>'|"/>"]`; or
      2. `"!--"`.

   3. If `Name` is none:

      1. Emit `Text(P..=P)`;
      2. Let `P` equal `P + 1`;
      3. Restart from [step 7](#s7).

      Otherwise;

   4. If `Name` is `"!--"`:

      1. Let `R` be the [comment range](#comment-range) from `Input` at `P`;
      2. Remove the overlapping range of any previous `Text` tokens emitted to
         the accumulator that overlap the range `R`;
      3. Emit `Comment(Input[R])`;
      4. Let the `Comment Range` of the last `Part` of `Stack` be `R`;
      5. If `R` both starts and ends with `'\n'`, set `Line Start`;
      6. Let `P` equal `R.end + 1`.

      Otherwise;

   5. Let `AS` be the position at the end of `Name`;
   6. Let `AE` be the position of the first `'>'` at or after `AS`;
   7. If `AE` is none:

      ※ Pathological backtracking may occur here. Limit backtracking with
        flags or memoisation.

      1. Emit `Text(P..=P)`;
      2. Let `P` equal `P + 1`;
      3. Restart from [step 7](#s7).

      Otherwise;

   8. If `Including` is set and `Name` matches the case-insensitive regular
      expression `/?includeonly`, or `Including` is clear and `Name` matches
      the case-insensitive regular expression `/?(?:noinclude|onlyinclude)`:

      1. Emit `Ignore(Input[P..=AE])`;
      2. Let `P` equal `AE + 1`;
      3. Restart from [step 7](#s7).

      Otherwise;

   9. If the character at `E - 1` is `/`:

      1. Let `E` equal `AE + 1`;
      2. Let `AE` equal `E - 1`;
      3. Let `Body` be none;
      4. Let `Close` be none.

      Otherwise:

      1. Let `EI` and `EO` be the start and end positions of the next XML end
         tag with a tag name that is an ASCII case-insensitive match for `Name`
         (n.b. This is weird and seems like a bug, since extension tags may be
         Unicode);

         ※ Pathological backtracking may occur here. Limit backtracking with
            flags or memoisation.

      2. If `EI` is none and `Name` is an inclusion control tag[^ictag], let
         `EI` and `EO` be the position at the end of `Input`; otherwise:

         1. Emit `Text(Input[P..=AE])`;
         2. Let `P` equal `AE + 1`;
         3. Restart from [step 7](#s7).

         Otherwise;

      3. If `Name` is an inclusion control tag[^ictag]:

         1. Emit `Ignore(Input[P..=EO]`);
         2. Let `P` equal `EO + 1`;
         3. Restart from [step 7](#s7).

         Otherwise;

      4. Let `Body` be `AE + 1..EI`.

   10. Emit `Extension(Name, AS..=AE, Body)`;
   11. Let `P` equal `EO + 1`.

   Otherwise;

10. If `Found` is `"Line Start"`:

    1. If `Line Start` is set, clear `Line Start`; otherwise
    2. Emit `Text(Input[P..=P])` and let `P` equal `P + 1`;
    3. Let `N` be the count of `=` at `P` with a maximum of 6;
    4. If `N` is above 1, or `N` is 1 and `Find Equals` is clear:

       1. Push

          ```
          [ "open" => '\n', "close" => '\n',
            "parts" => [ Text(Input[P..P + N]) ],
            "start" => P, "count" => N ]
          ```

          to `Stack`;
       2. Clear `Find Equals`;
       3. Clear `Find Pipe`;
       4. Set `In Heading`;
       5. Let `P` equal `P + N`.

    Otherwise;

11. If `Found` is `"Line End"`;

    1. Pop `Stack`;
    2. Assert `Stack.open` is `'\n'`;
    3. Let `Parts` be `Stack.parts`;

    ※ Steps 4 through 7 are just skipping any comments and white space at the
      end of a heading line in order to try to find the trailing heading tokens.

    4. Let `E` be the position of the first character in `Input` not matching
       `[' '|'\t']` scanning backward from `P`;
    5. Let `Part` be the last item of `Parts`;
    6. Let `Comment Range` be `Part.Comment Range`;
    7. If `Comment Range` is not none, and `Comment Range.end` is equal to
       `E - 1`:

       1. Let `E` equal `Part.Comment Range.start`;
       2. Let `E` be the position of the first character not matching
          `[' '|'\t']` scanning backward from `E`.

    7. Let `N` be the count of `'='` scanning backward from `E`;
    8. Let `S` equal `Stack.start`;
    9. If `E - N` equals `S`:

       1. If `N` is below 3, emit `Text(Input[S..E])`; otherwise
       2. Let `N` be minimum of the expression `((N - 1) / 2)` and 6;
       3. Emit `Heading(N, [ Text(Input[S + N..E - N]) ])`.

       Otherwise;

    10. Let `N` be the minimum of `N` and `Stack.count`;
    11. Emit `Heading(N, Part)`.

    Otherwise;

12. If `Found` is `"Open"`:

    1. If the length of `Char` is above 1, let `Count` be the count of
       characters matching the last character in `Char` that match at `P + 1`;
       otherwise
    2. Let `Count` be the count of characters matching `Char` at `P`;
    3. Let `Saved Prefix` be empty;
    4. Set `Line Start` if `P` is 0 and `Starts In SOL State` is set, or if the
       character at `P - 1` is `'\n'`;
    5. If `Char` is `"-{"` and `Count` is above the length of `Char`:

       1. Let `Saved Prefix` be `'-'`;
       2. Let `P` equal `P + 1`;
       3. Let `Char` be `'{'`;
       4. Decrement `Count` by 1;
       5. Let `Rule` be the value from the rules[^tokenrules] with the key
          `Char`;

    6. If `Count` is not below the `minimum` from `Rule`:

       1. Push

          ```
          [ "open" => Char, "close" => Rule.end,
            "prefix" => Saved Prefix,
            count => Count, start => Line Start ]
          ```

          to `Stack`;
       2. Set `Find Pipe` if `Char` does not start with `['['|'\n']`;
       3. Clear `Find Equals`;
       4. Clear `In Heading`.

       Otherwise;

    7. Emit `Text(Saved Prefix + Char * Count)`;
    8. Let `P` equal `P + Count`.

    Otherwise;

13. If `Found` is `"Close"`:

    1. Let `Max Count` equal the `count` property of `Stack`;
    2. If the `close` property of `Stack` is `"}-"` and `Char` is `'}'`,
       decrement `Max Count` by 1;
    3. If the length of `Char` is above 1, let `Count` be the length of `Char`;
       otherwise
    4. Let `Count` be the minimum of `Max Count` and the count of characters
       that match `Char` at `P`;
    5. Let `Rule` be the value from the rules[^tokenrules] with the key `Char`;
    6. If `Count` is above the `maximum` from `Rule`, let `Matching Count` be
       the `maximum` from `Rule`; otherwise:

       1. Let `Matching Count` be `Count`;
       2. While `Matching Count` is above 0 and the `names` list of `Rule` does
          not contain a key for `Matching Count`, decrement `Matching Count` by
          1.

    7. If `Matching Count` is not above 0:

       1. Emit `Text(Input[P..P + Count])`;
       2. Let `P` equal `P + Count`;
       3. Restart from [step 7](#s7).

       Otherwise;

    8. Pop `Stack`;
    9. Let `Name` be the value from the `names` list of `Rule` corresponding to
       the key `Matching Count`;
    10. If `Name` is none:

        1. Let `Element` be the [broken token](#broken-token) of `Stack` using
           `Matching Count`;
        2. Emit `Text(Input[P..P + Matching Count])` to `Element`.

        Otherwise;

        1. Let `Parts` be `Stack.parts`;
        2. Let `Title` be the unshift of `Parts`;
        3. Let `Children` be a list;
        4. If `Max Count` is `Matching Count`, and `Stack.start` is not none or
           zero, and `Stack.prefix` is empty, push `@lineStart: 1` to
           `Children`;
        5. Push `title: Title` to `Children`;
        6. Let `Index` be 1;
        7. For each `Part` in `Parts`:

           1. If `Eq Position` of `Part` is not none:

              1. Let `Equals` be the item at `Eq Position` in `Part`;
              2. Let `Arg Name` be the range `0..Eq Position`;
              3. Let `Arg Value` be the range `Eq Position + 1..`;
              4. Push `name: Arg Name, equals: Equals, value: Arg Value` to
                 `Children`.

              Otherwise;

              1. Let `Arg Name` be `@index: Index`;
              2. Increment `Index` by 1;
              3. Let `Arg Value` be `Part`;
              4. Push `name: Arg Name, value: Arg Value` to `Children`.

        8. Let `Element` be `[Name]: Children`.

    11. Let `P` equal `P + Matching Count`;
    12. If `Matching Count` is below the property `count` of `Piece`:

        1. Clear the `parts` of `Piece`;
        2. Decrement the `count` of `Piece` by `Matching Count`;
        3. If the `count` of `Piece` is not below the `minimum` of the value
           from the rules[^tokenrules] for the key `Piece.open`, push `Piece` to
           `Stack`; otherwise
        4. If the `count` of `Piece` is 1 and the `open` of `Piece` is `'{'` and
           the `Piece.prefix` is `'-'`:

           1. Let the `Piece.prefix` be empty;
           2. Let `Piece.open` be `"-{"`;
           3. Let `Piece.count` be 2;
           4. Let `Piece.close` be the `end` of the value from the
              rules[^tokenrules] with the key `Piece.open`;
           5. Push `Piece` to `Stack`.

           Otherwise;

        5. Let `S` be all of the characters from the `open` of `Piece` except
           for the final character;
        6. Let `L` be the length of `S`;
        7. Append the last character from the `open` of `Piece` to `S`,
           repeating `Count - L` times;
        8. Emit `Text(piece.prefix + S)`.

        Otherwise;

    13. If the `Piece.prefix` is not empty, emit `Text(piece.prefix)`;
    14. Restore `Find Equals`, `Find Pipe`, and `In Heading` from `Stack`;
    15. Extend `Stack` with `Element`.

    Otherwise;

14. If `Found` is `"Pipe"`:

    1. Set `Find Equals`;
    2. Push a new part to `Stack`;
    3. Let `P` equal `P + 1`.

    Otherwise;

15. If `Found` is `"Equals"`:

    1. Clear `Find Equals`;
    2. Emit TODO `Equals()` IS NOT A TOKEN;
    3. Set the `Eq Position` of the last part of `Stack` to the count of parts
       of `Stack` minus one;
    4. Let `P` equal `P + 1`.

[^tokenrules]: The token rules are:

    ```
    [
      '{' => [
         "end" => '}',
         "names" => [ 2 => "template", 3 => "tplarg" ],
         "minimum" => 2, "maximum" => 3,
      ],
      '[' => [
         "end" => ']',
         "names" => [ 2 => none ],
         "minimum" => 2, "maximum" => 2,
      ],
      "-{" => [
        "end" => "}-",
        "names" => [ 2 => none ],
        "minimum" => 2, "maximum" => 2,
      ]
    ]
    ```

### Preprocessor tokens

Possible tokens are:

* `Comment(text)`
* `Extension(tag name text, attributes text, body text)`
* `Heading(integer, token tree)`
* `Ignore(text)`
* `Parameter(parameter name text, arguments token tree)`
* `Template(title text, arguments token tree)`
* `Text(text)`

### Broken token

Using `Stack` and `Count`:

1. If `open` of `Stack` is `\n`, return `[ Stack.prefix, parts.0 ]`;
   otherwise
2. Let `S` be all of the characters from the `open` of `Piece` except for the
   final character;
3. Let `L` be the length of `S`;
4. Append the last character from the `open` of `Piece` to `S`, repeating
   `Count - L` times;
5. Let `Accumulator` be `[ Stack.prefix + S ]`;
6. Let `Index` be 0;
7. Set `First`;
8. For each `Part` in the `parts` list of `Stack`:

   1. If `First` is set, clear `First`; otherwise
   2. If the last item of `Accumulator` is `Text`, append `|`; otherwise
   3. Push `Text("|")` to `Accumulator`;
   4. For each `Node` in `Piece`:

      1. If the last item of `Accumulator` is `Text`, append `Node`; otherwise
      2. Push `Text(Node)` to `Accumulator`.

9. Return `Accumulator`.

## Preprocess input

※ There are special whitespace rules for template expansions; a naïve
  approach which simply concatenates the result of a template expansion will
  produce an incorrect final document.

Conceptually, the result of preprocessing input should be as if the plain text
from all *fully expanded* target templates already existed in the root
document’s source text before parsing ever began.

For each [`Token`](#preprocessor-tokens) in input:

1. If `Token` is `Template(Name, Arguments)`:

   1. Let `Result` be the result of [expanding the template](#expand-templates)
      using `Name` and `Arguments`;
   2. If the `Token` in the source text did not immediately follow a `'\n'` or
      the start of the input, and `Result` starts with `["{|"|':'|';'|'#'|'*']`,
      emit `'\n'`;
   3. Emit `Result`.

   Otherwise;

2. If `Token` is `Parameter(Name, Arguments)`:

   1. Let `Name` be the ASCII whitespace trimmed [expansion](#preprocess-input)
      of `Name`;
   2. If the caller provided an argument with the key `Name`, emit the
      [expansion](#preprocess-input) of that argument; otherwise
   3. If `Arguments` is not none and the first item of `Arguments` is not none,
      emit the [expansion](#preprocess-input) of the first item of `Arguments`;
      otherwise
   4. Let `Input` be the input source text;
   5. Let `R` be the range of the source text used to construct `Token`;
   5. Emit `Input[R]`.

3. If `Token` is `Comment(..)` or `Ignore(..)`, do nothing; otherwise
4. If `Token` is `Extension(..)`, emit the
   [expanded tag](#expand-extension-tags); otherwise
5. If `Token` is `Heading(Level, Body)`:

   1. Let `S` be the result of recursively preprocessing the token’s children;
   2. Let `N` be the global count of already processed `Heading` tokens;
   3. Let `M` be a non-standard [strip marker](#strip-marker) with tag name
      `"h"` and **decimal** ordinal `N`;
   4. Insert the strip marker `M` with kind `general` and content empty to the
      global list of strip markers;
   5. Emit the interpolation `"{S[..Level]}{M}{S[Level..]}"`.

   Otherwise;

6. If `Token` is `Text(T)`, emit `T`.

### Expand extension tags

※ Although this step expands its inputs, which implies that it may also expand
  templates, only token trees from the `#tag` parser function ever contain
  tokens other than `Text`. An extension tag using the XML syntax from in the
  Wikitext will only ever contain `Text` tokens, which means these expansions
  are just taking the text from the only token in the token tree.

1. Let `Name` be the [expanded](#preprocess-input) name of the tag;
2. If `Name` is an [error](#error-string), return `Name`; otherwise
3. Let `A` be the [expanded](#preprocess-input) attributes of the tag;
4. If `A` is an [error](#error-string), return `A`; otherwise
5. Let `Body` be the [expanded](#preprocess-input) body of the tag, or none if
   there is no body;
6. Let `N` be the global marker index;
7. Let `M` be a [strip marker](#strip-marker) with tag name `Name` and ordinal
   `N`;

   ※ Because the extension tag function may itself call to expand another
     extension tag, the marker MUST be created before calling the extension tag
     function since strip markers are exposed to Lua scripts which may depend
     on this side effect.

8. Increment the global marker index by 1;
9. If `Name` is an ASCII case-insensitive match for `nowiki`, let `Kind` be
   `nowiki`; otherwise, let `Kind` be `general`;
10. Let `A` be the [parsed attributes](#parse-attributes) from `A`;
11. Let `R` be the result of calling the extension tag function using the body
    `Body` and attributes `A`;
12. If `R` returns a `nowiki` hint, let `Kind` be `nowiki`;
13. Emit `M`.

### Error string

A string is considered an error if it starts with `<span class="error">`.

※ The `#iferror` parser function uses the broader regular expression
  `<(?:strong|span|p|div)\s(?:[^\s>]*\s+)*?class="(?:[^"\s>]*\s+)*?error(?:\s[^">]*)?"`.

### Expand templates

※ Implementations SHOULD implement resource limits to avoid DoS caused by
  template expansion bombs.

1. Assert that comments were tokenised out of the inside of the template
   expression during [preprocessing](#preprocess-input);
2. Let `Arguments` be the preprocessor arguments token tree;
3. Let `Name` be the [expanded](#preprocess-input) name of the template;
4. Trim ASCII whitespace from `Name`;
5. If `Name` is prefixed by a configured alias for `"subst"` or `"safesubst"`,
   and the parser is not in save mode[^subst], remove the prefix;
6. If `Arguments` is empty, and `Name` matches a configured variable alias, emit
   the variable’s value and terminate here; otherwise
7. If `Name` is prefixed by a configured alias for `"msgnw"`, remove the prefix
   and set `No Wiki`; otherwise, if `Name` is prefixed by a configured alias
   for `"msg"`, remove the prefix;
8. If `Name` is prefixed by a configured alias for `"raw"`, remove the prefix
   and set `Force Raw`;
9. If `Name` contains `[':'|'：']`:

   1. Let `Name` and `Arg0` be the left and right hand side of `Name` split by
      `[':'|'：']`;
   2. If calling the parser function with `Name`,
      [`Arg0`, and `Arguments`](#template-arguments) succeeds, emit the result
      and terminate here.

10. Run the [subpage resolution algorithm](#subpage-resolution) on `Name`;
11. If `Name` [parses as a title](#title):

    1. Let `Title` be the parsed title;
    2. If an article with the title `Title` does not exist, and the
       configuration enables link conversion, and the language converter
       contains a registered term matching `Name`, let `Title` be the result of
       [parsing](#title) the
       [matched term from the language converter](#language-conversion) as a
       title;
    3. If `Title` is already being expanded[^tpe], emit an
       [error string](#error-string) and terminate here; otherwise
    4. If `Title` is an internal title:

       1. If `Title` has the namespace `Special`, and the configuration allows
          special inclusion, and a special page with the given title exists,
          emit the result of rendering the special page using `Arguments` and
          terminate here; otherwise
       2. If an article or [shadow page](#shadow-page) with the title `Title`
          exists and the namespace of `Title` allows inclusion:

          1. Follow up to two [redirect](#redirect) directives;
          2. If `No Wiki` is set, emit the text of the `Title` template;
             otherwise
          3. Emit the result of expanding the `Title` template
             [with `Arguments`](#template-arguments) and terminate here.

          Otherwise;

       3. Let `Title` be the [prefixed](#title-glossary) text of `Title` and
          emit the interpolation `"[[:{Title}]]"`.

       Otherwise;

    5. If `Title` is an interwiki transclusion:

       1. Let `Query` be `"action=raw"` if `Force Raw` is set, otherwise
          `"action=render"`;
       2. Let `Data` be the text fetched from the remote server from the
          [full URL](#title-glossary) of `Title` using the query string `Query`;
       3. If `Force Raw` is set and `No Wiki` is clear, emit the result of
          expanding the `Data` with `Arguments` and terminate here; otherwise
       4. Emit `Data` and terminate here.

    Otherwise;

12. Emit the template expression itself, as plain text.

[^subst]: Save mode, and therefore the other `subst` rules, are out of scope of
  this document.

[^tpe]: For avoidance of doubt, if a template is not being expanded, i.e. it is
  being viewed as a document rather than being included in another document, it
  may include itself without causing an error. But if that inclusion then tries
  to include itself again, that must emit an error, because it is an expansion
  inside of an expansion.

### Redirect

Whether a title should redirect is implementation-defined. Traditionally, this
is a Wikitext document that starts with an alias for the redirect magic word,
followed by a Wikitext link corresponding to the redirect target.

### Shadow page

If the namespace of `Title` is an alias for the `MediaWiki` namespace and
`Title` is not an article:

1. Let `Key` be the [key](#title-glossary) of `Title` with lower-cased first
   letter;
2. Let `ID` and `Lang` be the left and right hand side of `Key` split at the
   last `'/'`;
3. If `Lang` is not a configured enabled language code with a non-empty autonym,
   let `ID` be `Key` and `Lang` be the user’s current language code;
4. Let `Message` be the text from the message dictionary for the language code
   `Lang`;
5. If `Message` is none or empty, return none; otherwise, return `Message`.

### Template expansion argument rules

* Arguments are not expanded until they are used by the callee.
* Positional (indexed) arguments for templates and Lua calls are 1-indexed.
* Named arguments given as `key=value` do not have a position.
* Named arguments do not influence the ordinal of positional arguments. (i.e.
  Given `{{..|k=v|A}}`, the position of `A` is 1, not 2.)
* Positional arguments are preferred over named arguments with numeric keys.
  (i.e. Given `{{..|1=A|B}}`, the result of `{{{1}}}` is `"B"`.)
* After expansion, ASCII whitespace is trimmed from named arguments but not from
  positional arguments.
* The 0th argument of a parser function, and the module name for a Lua module,
  are given in the name part of the expression, after the first `[':'|'：']`,
  after stripping prefixes.

### Comment range

1. Let `Input` be the input;
2. Let `O` be the position of `"<!--"` in `Input`;
2. Let `S` be the position of the first character which is not `[' '|'\t']` when
   scanning `Input` backward from `O`;
3. Let `C` be the position after the first match `"-->"` starting from `O`. If
   `"-->"` never matches, use the position of the end of `Input`;
4. Let `E` be the position of the first character which is not `[' '|'\t']` when
   scanning `Input` forward from `C`;
5. If the character at both `S` and `E` is `\n`, return the range `S..E`;
   otherwise
6. Return the range `O..C`.

## HTML attributes

※ Attribute value parsing uses a non-standard parse where `>` or `/>` are
  terminators for attribute values, even if they are inside a quoted-text part.
  This violates the XML and HTML standards.

For each attribute of each HTML tag in the input:

1. Let `Name` be the lowercase ASCII name of the attribute;
2. If `Name` is `"style"`:

   1. Let `Value` be the normalised value of the attribute:

      1. Decode HTML entities using the MediaWiki rules;
      2. Decode CSS escapes according to the CSS 2 grammar;
      3. If the value matches the regular expression
         `^\s*/\*[^*\\/]*\*/\s*$`, stop here; otherwise
      4. Replace CSS comments by a single space character;
      5. Split the result at `"/*"` and discard the right hand side.

   2. If `Value` contains any character in
      `['\0'..='\x08'|'\x0b'|'\x0e..=\x1f'|'\x7f'|'\u{fffd}']`, let `Value` be
      `"/* invalid control char */"`; otherwise
   3. If `Value` matches any of the following regular expressions, let
      `Value` be `"/* insecure input */"`:

      * `expression`
      * `accelerator\s*:`
      * `-o-(?:link|link-source|replace)\s*:`
      * `(?:url|src|image|image-set)\s*\(`
      * `attr\s*\([^)]+[\s,]+url`

   4. Use `Value` as the attribute value.

3. If `Name` is `"id"`, URL encode the value using the MediaWiki anchor
   encoding rules[^urlencode]; otherwise
4. If `Name` is `["aria-describedby"|"aria-flowto"|"aria-labelledby"`
   `|"aria-owns"]`:

   1. Trim ASCII whitespace from the start and end of the value;
   2. Split the value on runs of ASCII whitespace;
   3. Encode each split value using the MediaWiki anchor encoding
      rules[^urlencode];
   4. Join the list of values using a single space character between each value.

   Otherwise;

5. If `Name` is `["href"|"src"|"poster"]`, and the value does not use one of the
   configured URI schemes, discard it; otherwise
6. If `Name` is `"tabindex"` and the value is not `"0"`, discard it; otherwise
7. If `Name` is `["itemtype"|"itemid"|"itemref"]` and there is no corresponding
   `"itemscope"` attribute, discard it; otherwise
8. If `Name` is `"class"`,
8. If `Name` is not on the following whitelist for the given tag name, discard
   it:

   * Let `Common` be any of:

      * `["id"|"class"|"style"|"lang"|"dir"|"title"|"tabindex"`
        `|"aria-describedby"|"aria-flowto"|"aria-hidden"|"aria-label"`
        `|"aria-labelledby"|"aria-level"|"aria-owns"|"role"|"about"|"property"`
        `|"resource"|"datatype"|"typeof"|"itemid"|"itemprop"|"itemref"`
        `|"itemscope"|"itemtype"]`
      * The regular expression `^xmlns:[:-.\w]+$`
      * The regular expression `^data-(?!ooui|mw|parsoid)[^:= \t\r\n/>\0_＿]*$`

   * `["h1"|"h2"|"h3"|"h4"|"h5"|"h6"|"p"|"div"`
      `|"caption"]`:    `Common` + `["align"]`
   * `"meta"`:                     `["itemprop"|"content"]`
   * `"link"`:                     `["itemprop"|"href"|"title"]`
   * `["aside"|"dl"|"dt"|"dd"|"figure"|"figcaption"|"em"|"strong"|"small"`
     `|"s"|"cite"|"dfn"|"abbr"|"ruby"|"rt"|"rp"|"code"|"var"|"samp"|"kbd"`
     `|"sub"|"sup"|"i"|"b"|"u"|"mark"|"bdi"|"bdo"|"span"|"wbr"|"tbody"`
     `|"thead"|"tfoot"|"rb"|"rtc"|"strike"|"big"|"center"|"tt"]`: `Common`
   * `["hr"|"pre"]`:    `Common` + `["width"]`
   * `"blockquote"`:    `Common` + `["cite"]`
   * `"ol"`:            `Common` + `["type"|"start"|"reversed"]`
   * `"ul"`:            `Common` + `["type"]`
   * `"li"`:            `Common` + `["type"|"value"]`
   * `"a"`:             `Common` + `["href"|"rel"|"rev"]`
   * `"q"`:             `Common` + `["cite"]`
   * `"data"`:          `Common` + `["value"]`
   * `"time"`:          `Common` + `["datetime"]`
   * `"br"`:            `Common` + `["clear"]`
   * `["ins"|"del"]`:   `Common` + `["cite", "datetime"]`
   * `"source"`:        `Common` + `["type"|"src"]`
   * `"img"`:           `Common` + `["alt"|"src"|"width"|"height"|"srcset"]`
   * `"video"`:         `Common` + `["poster"|"controls"|"preload"|"width"`
                                   `|"height"]`
   * `"audio"`:         `Common` + `["controls"|"preload"|"width"|"height"]`
   * `"track"`:         `Common` + `["type"|"src"|"srclang"|"kind"|"label"]`
   * `"math"`:                     `["class"|"style"|"id"|"title"]`
   * `"table"`:         `Common` + `["summary"|"width"|"border"|"frame"|"rules"`
                                   `|"cellspacing"|"cellpadding"|"align"`
                                   `|"bgcolor"]`
   * `["col"`
     `|"colgroup"]`:    `Common` + `["span"]`
   * `"tr"`:            `Common` + `["bgcolor"|"align"|"valign"]`
   * `["td"|"th"]`:     `Common` + `["bgcolor"|"align"|"valign"|"abbr"|"axis"`
                                   `|"headers"|"scope"|"rowspan"|"colspan"`
                                   `|"nowrap"|"width"|"height"]`
   * `"font"`:          `Common` + `["size"|"color"|"face"]`

9. If the attribute name matches an attribute name which already exists
   on the tag, replace the old value of the previous attribute with the new
   value from the new attribute; otherwise
10. Add the attribute to the tag.

## Tables

1. Let `TD History`, `TR History`, `Open TR History`, `TR Attributes`, and
   `Last Tag` be stacks;
2. Let `Indent Level` be 0.

For each line of the input `L` split on `'\n'`:

1. If `L` contains only ASCII whitespace, emit the interpolation `"{L}\n"`;
   otherwise
2. If `L` matches the regular expression `^\s*(?::*)\s*\{\|`:

   1. Let `Rest` be the rest of `L` after `"{|"`;
   2. [Unstrip](#unstrip) all markers from `Rest`;
   3. Let `A` be the result of [parsing the attributes](#parse-attributes)
      from `Rest`;
   4. Let `Indent Level` be the count of `':'` at the start of `L`;
   5. Push `false` to `TD History`, `TR History`, and `Open TR History`;
   6. Push empty to `Last Tag` and `TR Attributes`.
   7. For each `Indent Level`, emit `"<dl><dd>"`;
   8. Emit the interpolation `"<table{A}>\n"`;

   Otherwise;

3. If `TD History` is empty, emit the interpolation `"{L}\n"`; otherwise
4. Trim ASCII whitespace from the start and end of `L`;
5. If `L` starts with `"|}"`:

   1. Pop `TR Attributes`;
   2. Pop `Last Tag`;
   3. Pop `TD History`. If `true`, emit the interpolation `"</{Last Tag}>"`;
   4. Pop `TR History`. If `true`, emit `"</tr>"`;
   5. Pop `Open TR History`. If not `true`, emit `"<tr><td></td></tr>"`;
   6. Emit `"</table>"`.
   7. Let `Rest` be the rest of the line after `"|}"`;
   8. If `Indent Level` is not 0:

      1. Trim ASCII whitespace from the end of `Rest`;
      2. Emit `Rest`;
      3. Emit `"</dd></dl>"` `Indent Level` times.

      Otherwise:

      1. Emit `Rest`.

   9. Emit `\n`.

   Otherwise;

6. If `L` starts with `|-`:

   1. Let `Rest` be the rest of the line after the regular expression `^\|-+`;
   2. Let `A` be the result of [parsing the attributes](#parse-attributes)
      from `Rest`;
   3. Pop and push `A` to `TR Attributes`;
   4. Pop and push `true` to `Open TR History`;
   5. Pop and push empty to `Last Tag`;
   6. Pop and push `false` to `TD History`. If `true`, emit the interpolation
      `"</{Last Tag}>"`.
   7. Pop and push `false` to `TR History`. If `true`, emit `"</tr>"`;
   8. Emit `'\n'`.

   Otherwise;

7. If `L` starts with `['|'|'!'|"|+"]`:

   1. Let `Rest` be the rest of the line after the start token;
   2. If `L` starts with `'!'`, let `Rest` be the result of replacing all `"!!"`
      that are not encapsulated by the delimiters `'<'` and `'>'` with `"||"`;
   3. Let `Cells` be the result of splitting `Rest` by `"||"`;
   4. For each `Cell` in `Cells`:

      1. Pop `Last Tag`;
      2. Pop and push `true` to `TD History`. If `true`, emit the interpolation
         `"</{Last Tag}>\n"`;
      3. If `L` does not start with `"|+"`:

         1. Pop and push empty to `TR Attributes`;
         2. Pop and push `true` to `TR History`. If not `true`, emit the
            interpolation `"<tr{TR Attributes}>\n"`;
         3. Pop and push `true` to `Open TR History`.

      4. If `L` starts with `'|'`, push `td` to `Last Tag`; otherwise
      5. If `L` starts with `'!'`, push `th` to `Last Tag`; otherwise
      6. If `L` starts with `"|+"`, push `caption` to `Last Tag`;
      7. Let `A` and `D` be the left and right hand side of `Cell` split by
         `'|'`;
      8. If `Cell` did not contain `'|'`, or `A` contains a start token for an
         internal link or language conversion:

         1. Trim ASCII whitespace from `Cell`;
         2. Emit the interpolation `"<{Last Tag}>{Cell}\n"`.

         Otherwise;

         1. Let `A` be the result of [parsing the attributes](#parse-attributes)
            from `A`;
         2. Trim ASCII whitespace from `D`;
         3. Emit the interpolation `"<{Last Tag}{A}>{D}\n"`.

Finally:

1. While `TD History` is not empty:

    1. Pop `TD History`. If `true`, emit `"</td>\n"`[^oopsbug];
    2. Pop `TR History`. If `true`, emit `"</tr>\n"`;
    3. Pop `Open TR History`. If not `true`, emit `"<tr><td></td></tr>\n"`.
    4. Emit `"</table>\n"`.

2. Remove any trailing `'\n'` from the output;
3. If the entire output is `"<table>\n<tr><td></td></tr>\n</table>"`, clear the
    output.

[^oopsbug]: This is a bug in the original parser since `TD History` may be true
  for any of `["caption"|"td"|"th"]`. Otherwise compliant parsers might want to
  emit the correct last tag instead.

### Parse attributes

1. Unstrip all strip markers from the input;
2. Trim ASCII whitespace from the input;
3. If the input is empty, return empty;
4. Let `A` be a map.

For each attribute pair extracted from the input by the regular expression
   `(?<N>[^\s/>][^\s/>=]*)(?:\s*=\s*(?:"(?<V1>[^"]*)(?:"|$)|'(?<V2>[^']*)(?:'|$)|(?<V3>[^\s>]*))?`:

1. Let `N` be the ASCII lowercase conversion of capture group `N`;
2. If `N` matches the regular expression `^[:_\p{L}\p{N}][:_.\p{L}\p{N}-]*$`:

   ※ The behaviour of matching the pair and then filtering the name is different
     than matching the name in the attribute pair. If combined, the pair
     expression may fail to match an invalid name, then improperly match parts
     of the value.

   1. Let `V` be whichever capture group `[V1|V2|V3]` is not none;
   2. Replace each run of `['\t'|'\r'|'\n'|' ']` in `V` with a single space
      character;
   3. Trim ASCII whitespace from `V`;
   4. Decode HTML entities in `V` using the special MediaWiki rules[^entity];
   5. Insert `V` to `A` with key `K`.

Finally, return `A`.

## Horizontal rules

1. For each line that starts with a sequence of at least 4 `'-'`, replace the
   sequence of `'-'` with `"<hr>"`.

## Double underscores

For each sequence of characters that corresponds with a configured double
   underscore alias, delete the sequence.

## Headings

Assert that the input contains no HTML comments.

For each `N` from 6 to 1:

1. Let `S` be 0;
2. Let `H` be a sequence of `'='` characters repeated `N` times.

While `S` is not the position of the end of the input:

1. Let `TE` and `E` be the start and end positions of the next range of ASCII
   whitespace ending with either one or more `'\n'` or the end of the input
   after `S`;
2. If `H` matches at `S`:

   1. Let `TS` be `S + N`;
   2. Subtract `N` from `TE`;
   3. If `H` matches at `TE`:

      1. Let `T` be the text in the range `TS..TE`;
      2. Trim `[' '|'\t']` from the start and end of `T`;
      3. Replace the range `S..E` with the interpolation
         `"<h{N}>{T}</h{N}>`.
      4. If `E` is not the position of the end of the input, emit `'\n'`.

3. Let `S` equal `E`.

## Wikilinks

While `S` is not the position of the end of the input:

1. Let `S` be the position of the sequence `"[["`;
2. Let `E` be the position of the next sequence `"[["`, or the end of the input
   if there is no next start token;
3. If the configuration specifies a link prefix, let `LP` be the position of the
   start of the matching prefix, scanned backward from `S`;
4. Let `TC` be the set of valid title bytes from the configuration plus the
   bytes `[b'#'|b'%']`;
5. Let `A` be the position of the first byte after the start token that is not
   in `TC`;
6. If the character sequence at `A` is not `['|'|"]]"]`, emit the range `S..E`
   as text, let `E` equal `S`, and restart the loop; otherwise
7. Let `Link` be the text in the range `S + 2..A`;
8. If `Link` contains any [strip marker](#strip-marker), emit the range `S..E`
   as text, let `E` equal `S`, and restart the loop; otherwise
9. If the character at `A` is `'|'`:

   1. Let `A` equal `A + 1`;
   2. Let `B` be the position of the next character sequence `"]]"` before `E`;
   3. If `B` is not found, or `B` equals `A`, set `Maybe Image`; otherwise

      1. If the next character after `B` is `']'`, and a character `'['` is in
         the range `A..B`, let `B` equal `B + 1`;
      2. Let `Text` be the text in the range `A..B`;
      3. Run the [text style algorithm](#text-styles) on `Text`.

   Otherwise:

   1. Assert that the character sequence at `A` is `"]]"`;
   2. Let `B` equal `A`;
   3. Let `Text` be `Link`.

10. Let `LS` be the position of the end of the matching configuration link
    suffix, scanned forward from `B + 2`, or `B + 2` if there is no configured
    link suffix;
11. If `Link` contains a character `%`:

    1. URL decode `Link`;
    2. Replace all `<` and `>` in `Link` with `&lt;` and `&gt;`.

12. Trim space characters from the left side of `Link`;
13. If `Link` starts with one of the configured URI schemes from the
    configuration, emit the range `S..E` as text, let `S` equal `E`, and
    restart the loop; otherwise
14. Set the `Force Display` flag if `Link` starts with `':'`;
15. Run the [subpage resolution algorithm](#subpage-resolution) on `Link` and
    `Text`;
16. Let `Title` be the `Link` [parsed as a title](#title);
17. If `Title` is invalid, emit the range `S..E` as text, let `S` equal `E`, and
    restart the loop;
    otherwise
18. If the `Maybe Image` flag is set:

    ※ This part of the algorithm is just trying to allow Wikilinks to be nested
      one level deep inside of an image caption, which will be processed
      separately when the media link is converted to HTML.

    1. If the `Force Display` flag is clear and the namespace of `Title` is
       `File`:
       1. Let `PE` equal `E`;
       2. <a name="ss2"></a>
          Let `P` be the position of the next `"[["` after `PE`;
       3. If `P` is none, emit the range `S..E` as text, let `S` equal `E`, and
          restart the loop; otherwise
       4. Let `L`, `R` be the positions of the next two character sequences
          `"]]"` in the input range `PE..P`;
       5. If `L` and `R` are not none:

          1. Let `B` equal `R`;
          2. Let `LS` equal `B + 2`;
          3. Let `E` equal `P`;
          4. Let `Text` be the text in the range `A..B`.

          Otherwise;

       6. If `L` is not none:

          1. Let `PE` equal `P`;
          2. Continue from [step 2](#ss2).

          Otherwise;

       7. Emit the range `S..E` as text, let `S` equal `E`, and restart the
          loop.

       Otherwise;

    2. Emit the range `S..E` as text, let `S` equal `E`, and restart the loop.

    Otherwise;

19. If the `Force Display` flag is clear and `Title` is not a
    [local interwiki](#title-glossary):

    1. If `Title` is an [interwiki](#title-glossary) title, and the
       interlanguage magic configuration option is enabled, and the caller is
       not a talk page, and the interwiki name is either a defined autonym or in
       the configured list of interlanguage link prefixes, delete text
       immediately before `S` that matches the regular expression `\n\s*$`, let
       `S` equal `E`, and restart the loop; otherwise
    2. If the namespace of `Title` is `Category`, delete text immediately before
       `S` that matches the regular expression `\n\s*$`, let `S` equal `E`, and
       restart the loop; otherwise
    3. If the namespace of `Title` is `File`:

       1. If `Text` is not empty:

          1. Let `Text` be the result of running the
             [external links algorithm](#external-links) on `Text`;
          2. Let `Text` be the result of running the
             [Wikilinks algorithm](#wikilinks) on `Text`.

       2. Let `Image` be the result of running [make image](#make-image) with
          `Title` and `Text`;
       3. Let `Image` be the result of running the
          [external link cloaking algorithm](#external-link-cloaks) on `Image`;
       4. Emit `Image`;
       5. Let `S` equal `E` and restart the loop.

    Otherwise;

20. If the namespace of `Title` is not `Special` and `Title` matches the caller,
    emit a [self link](#self-links) using `Title` and `Text`; otherwise
21. If the namespace of `Title` is `Media`:

    1. Let `Media` be the result of running the
       [media link algorithm](#media-link) using title `Title` and body `Text`;
    2. Run the [external link cloaking algorithm](#external-link-cloaks) on
       `Media`;
    3. Emit `Media`, let `S` equal `E`, and restart the loop.

    Otherwise;

22. Let `Text` be the concatenation of the text in the range `S..LP`, `Text`,
    and the text in the range `B + 2..LS`;
23. If the `Title` is not an [interwiki](#title-glossary) title, and the `Title`
    is an [always known](#always-known-title) title, emit a
    [cloaked](#external-link-cloaks) hyperlink to `Title` with `Text` as the
    body text; otherwise;
24. Emit a link comment for `Title` with `Text` as the body text;
25. Let `S` equal `E`.

### Make image

1. Let `Input` be the input and `Title` be the title;
2. Let `S` be 0;
3. Let `File` be the file metadata for `Title` retrieved from a file store;
4. Let `Validated` and `Seen Format` be a flag;
5. Let `Params` be a map of parsed image parameters;
6. Let `Param Map` be an map from a canonical name of an alias to an
   internal image parameter[^parammap].

While `S` is not the position of the end of the input:

1. Clear `Validated`;
2. Let `P` be the position of the next `'|'` not inside a
   [language conversion tag](#language-conversion), or the end of the input if
   there is no next `'|'`;
3. Let `Part` be the ASCII whitespace trimmed text from the range `Input[S..P]`;
4. If `Part` starts with a configured alias for an image parameter:

   1. Let `Name` be the canonical name for the matched configured alias and
      `Value` be the rest of `Part` after the alias;
   2. Let `Kind` be the value of `Name` in `Param Map`;
   3. If `Name` is `img_width`, and `Value` matches the case-sensitive regular
      expression `^([0-9]*)(?:x([0-9]*))?\s*(?:px)?\s*$`:

      1. Let `Width` and `Height` be the captures, parsed as integers, where an
         empty capture is parsed as the integer 0;
      2. If either `Width` or `Height` is above 0, set `Validated`;
      3. If `Width` is above 0, insert `Width` to `Params` with the key
         `"width"`;
      4. If `Height` is above 0, insert `Height` to `Params` with the key
         `"height"`.

      Otherwise;

   4. If `Name` is `["img_alt"|"img_class"|"img_link"|"img_manualthumb"]`, run
      the [image attribute algorithm](#image-attribute) on `Value`;
   5. If `Name` is `["img_alt"|"img_class"]`, set `Validated`; otherwise
   6. If `Name` is
      `["img_manualthumb"|"img_frameless"|"img_framed"|"img_thumbnail"]`, set
      `Validated` if `Seen Format` is clear, then set `Seen Format`; otherwise
   7. If `Name` is `"img_link"`:

      1. Let `Value` be the result of [parsing the link](#parse-image-link) from
         `Value`;
      2. If `Value` is not none, set `Validated`.

      Otherwise;

   8. If `Value` is empty or the ASCII trimmed `Value` is a [numeric string],
      set `Validated`;
   9. If `Validated` is set, insert `Value` to `Params` with key `Kind`.

5. If `Validated` is clear, let `Caption` be `Part`;
6. Let `S` equal `P + 1`.

Finally:

1. If the value of `Params.frame` is not `["img_framed"|"img_thumbnail"]` and
   the value of `Params.manualthumb` is none:

   1. Let `Alt` be the result of running the
      [image attribute algorithm](#image-attribute) on `Caption`;
   2. If `Params` has no `"alt"` key and `Caption` is not none, insert `Alt`
      to `Params` with the key `"alt"`;
   3. Insert `Alt` to `Params` with the key `"title"`.

2. Insert `Caption` to `Params` with the key `"caption"`;
3. Emit the result of [making image HTML](#make-image-html) with `Params`.

[numeric string]: https://www.php.net/manual/en/language.types.numeric-strings.php

[^parammap]: The default parameter map for a Wikitext document that supports
    static images:

    ```php
    [
      "img_alt" => "alt",
      "img_baseline" => "vertAlign",
      "img_border" => "border",
      "img_bottom" => "vertAlign",
      "img_center" => "horizAlign",
      "img_class" => "class",
      "img_framed" => "frame",
      "img_frameless" => "frame",
      "img_height" => "height",
      "img_left" => "horizAlign",
      "img_link" => "link",
      "img_manualthumb" => "manualthumb",
      "img_middle" => "vertAlign",
      "img_none" => "horizAlign",
      "img_right" => "horizAlign",
      "img_sub" => "vertAlign",
      "img_super" => "vertAlign",
      "img_text_bottom" => "vertAlign",
      "img_text_top" => "vertAlign",
      "img_thumbnail" => "frame",
      "img_title" => "title",
      "img_top" => "vertAlign",
      "img_upright" => "upright",
      "img_width" => "width",
    ]
    ```

  Additional media handlers can add their own arbitrary entries.

### Parse image link

Emits an enumeration with variants:

* `NoLink`
* `URL(text)`
* `Title(title)`

1. Let `Input` be the input;
2. If `Input` is empty, emit `NoLink`; otherwise
3. If `Input` starts with a configured URI scheme:

   1. If `Input` matches:

      1. A configured URI scheme; then
      2. A fuzzy IP address[^fuzzyip] or character in the URL set[^urlset]; then
      3. Zero or more characters in the URL set[^urlset].

   Then emit `URL(Input)`, otherwise emit none.

   Terminate here.

4. If `Input` contains a `'%'`, URL decode it;
5. Let `Title` be the [parsed title](#title) of `Input`;
6. If `Title` is not none, emit `Title(Title)`; otherwise
7. Emit none.

### Make image HTML

1. Let `File` be the image file;
2. Let `Params` be the parameters;
3. Let `Title` be the image link target;
4. If the image is not allowed to be displayed inline according to an
   implementation-defined condition, treat it as a plain [Wikilink](#wikilinks),
   emit a hyperlink, and terminate here; otherwise
5. Let `Classes` be a list;
6. Let `Width` be the value from `Params.width`;
7. Let `Frame` be the value from `Params.frame`;
8. Let `Manual Thumb` be the value from `Params.manualthumb`;
9. If `Width` is none and `Frame` is not `"img_framed"` and `Manual Thumb` is
   none, push `"mw-default-size"` to `Classes`;
10. If `File` is not none and `Width` is none, set the value of `Params.width`
    to an appropriate implementation-defined computed value based on data from
    `File`;
11. If `Frame` is `["img_thumbnail"|"img_framed"]` or `Manual Thumb` is not
    none, emit [a bunch of copy-pasted code](#thumb-link-2) using `Params` and
    `Classes` and terminate here;
    otherwise;
12. Let `RDFa Type` be `"mw:File"`;
13. If `Frame` is `"img_frameless"`:

    1. Append `"/Frameless"` to `RDFa Type`;
    2. If `File` is not none, set the value of `Params.width` to an appropriate
       implementation-defined computed value based on data from `File` if the
       computed value is below the value of `Params.width`.

14. Emit the [finished image](#finish-image) using `File`, `Params`,
    `RDFa Type`, `Title`, and `Classes`.

### Thumb link 2

1. If `Params.horizAlign` is none, let `Align` be empty, otherwise let `Align`
   be the value from `Params.horizAlign`;
2. If `Params.caption` is none, let `Caption` be empty, otherwise let `Caption`
   be the value from `Params.caption`;
3. If `Params.width` is none, let `Width` be 130 if `Params.upright` is not
   none, else 180, otherwise let `Width` be the value from `Params.width`;
4. Let `RDFa Type` be `"mw:File/Thumb"`;
5. If `File` does not exist:

   1. If `Params.manualframe` is none and `Params.frame` is `"img_framed"`, let
      `RDFa Type` be `"mw:File/Frame"`;
   2. If `Params.frame` is `"img_manualthumb"`:

      1. Let `Title` be the result of
         [parsing `Params.manualthumb` as a title](#title) using default
         namespace `File`;
      2. If `Title` is not none, let `File` be the file found in an
         implementation-defined way;
      3. If `File` is not none, let `Thumb` be the implementation-defined
         thumbnail file;
      4. If `Link` is none, let `Params.link` be `Title(Title)`.

      Otherwise;

   3. Let `Width` be an appropriate implementation-defined computed value based
      on data from `File` if the computed value is below `Width`, let `Thumb` be
      the implementation-defined thumbnail file.

6. Emit the [finished image](#finish-image) using `File`, `Params`, `RDFa Type`,
   and `Title`.

### Finish image

※ This list of steps is messed up and probably wrong/incomplete because someone
  decided to copy and paste a bunch of code and make tiny tweaks. So this comes
  from one of two branches of basically identical code.

1. Let `File` be the input file;
2. Let `Params` be the input parameters;
3. Let `RDFa Type` be the input RDFa type;
4. Let `Title` be the input title;
5. Let `Classes` be the input class list, or empty if there is no input class
   list;
6. Let `Alt` be the value from `Params.alt`;
7. Let `Upright` be the value from `Params.upright`;
8. Let `Thumb` be none;
9. If `File` is not none and `Params.width` is not none, let `Thumb` be an
   implementation-defined thumbnail representation of `File`;
10. If `File` is not none and `Thumb` is not none and `File` is considered a
    “bad” file according to an implementation-defined criteria, set `Bad File`;
11. If `Thumb` is none or an implementation-defined error, or `Bad File` is set:

    1. Prefix `RDFa Type` with `"mw:Error "`;
    2. Let `Label` be an implementation-defined error message, or `Alt` if there
       is no implementation-defined error message, or the
       [prefixed](#title-glossary) text of `Title` if `Alt` is none;
    3. Let `Body` be an HTML `span` element with `"class"` attribute value
       `"mw-file-element mw-broken-media"` and body `Label`;
    4. Let `Image` be a [media link](#media-link) with `Title` and `Body`.

    Otherwise:

    1. Let `Image` be the [thumbnail HTML representation](#thumb-html) of
       `Thumb`.

12. Let `Wrapper` be `"span"`;
13. Let `Caption` be empty;
14. Let `Align` be the value from `Params.horizAlign`;
15. Let `VAlign` be the value from `Params.vertAlign`;
16. If `Align` is not none:

    1. Let `Wrapper` be `"figure"`;
    2. Strip the `"img_"` prefix from `Align`;
    3. Push the interpolation `"mw-halign-{Align}"` to `Classes`;
    4. Let `Caption` be an HTML element with tag name `"figcaption"` and body
       `Params.caption`.

    Otherwise;

17. If `VAlign` is not none:

    1. Strip the `"img_"` prefix from `Align`;
    2. Replace all `'_'` with `'-'` in `Align`;
    3. Push the interpolation `"mw-valign-{VAlign}"` to `Classes`.

18. If `Params` has a key `"border"`, push `"mw-image-border"` to `Classes`;
19. If `Params` has a key `"class"`, push its value to `Classes`;
20. Let `Class` be `Classes` joined into a string using the delimiter the space
    character;
21. Let `Figure` be an HTML element with the tag name `Wrapper`, `"class"`
    attribute value `Class`, `"typeof"` attribute value `RDFa Type`, and body
    the interpolation `"{Image}{Caption}"`;
22. Replace all `'\n'` in `Figure` with the space character;
23. Emit `Figure`.

### Thumb HTML

1. Let `Thumb` be the implementation-defined thumbnail object;
2. Let `Params` be a the input parameters;
3. Let `Caption` be the value from `Params.caption`;
4. Let `Classes` be `"mw-file-element"`;
5. Let `Style` be empty;
6. If `Params.upright` is not none:

   1. Append `" mw-file-upright"` to `Classes`;
   2. Let `Style` be `"--mw-file-upright: {Params.upright};"`.

7. Let `Src` be the implementation-defined URL of `Thumb`;
8. If `Params.link` is `URL(URL)`, let `Href` be `URL`; otherwise
9. If `Params.link` is `Title(Title)`:

    1. Let `Href` be the [link URL](#title-link-url) of `Title`;
    2. If `Caption` is none, let `Caption` be the
       [prefixed](#title-glossary) text from `Title`.

    Otherwise;

10. If `Params.link` is not `NoLink`, let `Link Class` be
    `"mw-file-description"`;
11. If the value of `Params.class` is not none, append it to `Classes`;
12. Let `Image` be an HTML element with tag name `"img"`, `"alt"` attribute
    value `Params.alt` if is not none, `"src"` attribute value `Src`,
    `"decoding"` attribute value `"async"`, `"class"` attribute value `Classes`,
    and `"style"` attribute value `Style` if `Style` is not empty;
13. If `Href` is none, let `Tag Name` be `"span"`, otherwise let `Tag Name` be
    `"a"`;
14. Emit an HTML element with the tag name `Tag Name`, `"href"` attribute value
    `Href` if `Href` is not none, `"title"` attribute value `Caption` if
    `Caption` is not none, `"class"` attribute value `Link Class` if
    `Link Class` is not none, and body `Image`.

### Image attribute

1. [Unstrip](#unstrip) the input using mode `both`;
2. If the input contains one of the [HTML5 named character references] which
   does not terminate with a `';'`, replace the `'&'` of that named character
   reference with `"&amp;"`;
3. [Parse into an HTML5 DOM];
4. Let `O` be empty;
5. For each element in the DOM tree, in depth-first order:

   1. If the element is `["style"|"script"]`, skip the element and its children;
      otherwise
   2. At the start or end of a block-level element[^captionble], append a space
      character to `O`;
   3. Append the text content of the element to `O`.

6. Replace each run of `[' '|'\r'|'\n'|'\t']` in `O` with a single space
   character;
7. Trim ASCII whitespace from the start and end of `O`;
8. Emit `O`.

[HTML5 named character references]: https://html.spec.whatwg.org/multipage/named-characters.html#named-character-references

[^captionble]: `["address"|"article"|"aside"|"blockquote"|"br"|"canvas"|"dd"`
  `|"div"|"dl"|"dt"|"fieldset"|"figcaption"|"figure"|"footer"|"form"|"h1"|"h2"`
  `|"h3"|"h4"|"h5"|"h6"|"header"|"hgroup"|"hr"|"li"|"main"|"nav"|"noscript"`
  `|"ol"|"output"|"p"|"pre"|"section"|"table"|"td"|"tfoot"|"th"|"tr"|"ul"`
  `|"video"]`

### Media link

1. Let `Title` be the title;
2. Let `Text` be the body;
3. Retrieve file metadata for `Title` from a file store;
4. If the file exists:

   1. Let `URL` be the URL to the file;
   2. Let `Class` be `"internal"`;

   Otherwise:

   1. Let `URL` be the URL to the special page `"Upload"` with title `Title`;
   2. Let `Class` be `"new"`.

5. Let `Alt` be the [text](#title-glossary) of `Title`;
6. If `Text` is none, let `Text` be `Alt`;
7. Emit a hyperlink with `"href"` attribute value `URL`, `"class"` attribute
   value `Class`, `"title"` attribute value `Alt`, and body `Text`.

### External link cloaks

1. Let `Input` be the input;
2. Let `P` be the position of the first case-insensitive match of a configured
   URI scheme after a word boundary;
3. If `P` is none, emit `Input`; otherwise
4. Emit `Input[..P]`;
5. Emit a non-standard [strip marker](#strip-marker) with only a **prefix**;
6. Emit `NOPARSE`;
7. Emit `Input[P..]`.

### Always known title

True if:

1. If the title is external; or
2. If the shadow page loader says it exists; or
3. If the namespace is `Media` or `File` and the file finder says it exists; or
4. If the namespace is `Special` and the special page factory says it exists; or
5. If the namespace is `Main` and the title `key` is empty.

### Self link

1. Let `Title` and `Text` be the input [title](#title) and text, respectively;
2. Let `F` be the [fragment](#title-glossary) of `Title`;
3. If `F` is not none, emit the interpolation
   `"<a class=\"mw-selflink-fragment\" href=\"#"{F}>"`; otherwise
4. Emit `<a class="mw-selflink selflink">`;
5. If `Text` is empty, let `Text` be the HTML encoded [prefix](#title-glossary)
   text of `Title`;
6. Emit `Text`;
7. Emit `</a>`.

## Text styles

First:

1. Let `Input` be the input;
2. Let `A` be 0;
3. Let `N Italic`, `N Bold`, and `Decay Priority` be 0;
4. Let `Decay Position` be none;
5. Let `Styles` be a list.

While `A` is not the position of the end of the input:

1. Let `A` be the next position of a sequence of two or more `"'"` at or after
   `A`, or exit this loop if there are no matching sequences;
2. Let `N` be the number of apostrophes in the sequence;
3. If `N` is above 5, let `A` be `A + N - 5` and let `N` be 5;
4. If `N` is 5, increment `N Italic` and `N Bold` by 1; otherwise
5. If `N` is 4, increment `A` by 1 and set `N` to 3;
6. If `N` is 3:

   1. Increment `N Bold` by 1;
   2. If the character at `A - 1` is a space character and `Decay Priority` is
      less than 1, set `Decay Position` to the position of `A` and
      `Decay Priority` to 1; otherwise
   3. If the character at `A - 2` is a space character and `Decay Priority` is
      less than 3, set `Decay Position` to the position of `A` and
      `Decay Priority` to 3; otherwise
   4. If `Decay Priority` is less than 2, set `Decay Position` to the position
      of `A` and `Decay Priority` to 2.

   Otherwise;

7. If `N` is 2, increment `N Italic` by 1;
8. Push `(A, N)` to `Styles`;
9. Let `A` be `A + N`.

Next:

1. If both `N Italic` and `N Bold` are odd and `Decay Position` is not none,
   insert one apostrophe at `Decay Position` and reduce the `N` of the style at
   `Decay Position` by 1;
2. Let `S` be `None`;
3. Let `LA` be 0;
4. For each character position `A` and text style `N` in `Styles`:

   1. Emit `Input[LA..A]`;
   2. If `N` is 5:

      1. If `S` is `I`, emit `"</i><b>"`, and set `S` to `B`; otherwise
      2. If `S` is `B`, emit `"</b><i>"` and set `S` to `I`; otherwise
      3. If `S` is `BI`, emit `"</i></b>"` and set `S` to `None`; otherwise
      4. If `S` is `IB`, emit `"</b></i>"` and set `S` to `None`; otherwise
      5. If `S` is `None`, peek the next text style:

         1. If the next text style is `I`, emit `"<b><i>"` and set
            `S` to `BI`; otherwise
         2. Emit `"<i><b>"` and set `S` to `IB`.

      Otherwise;

   3. If `N` is 3:

      1. If `S` is `I`, emit `"<b>"`, and set `S` to `IB`; otherwise
      2. If `S` is `B`, emit `"</b>"` and set `S` to `None`; otherwise
      3. If `S` is `BI`, emit `"</i></b><i>"` and set `S` to `I`; otherwise
      4. If `S` is `IB`, emit `"</b>"` and set `S` to `I`; otherwise
      5. If `S` is `None`, emit `"<b>"` and set `S` to `B`.

      Otherwise;

   4. If `N` is 2:

      1. If `S` is `I`, emit `"</i>"`, and set `S` to `None`; otherwise
      2. If `S` is `B`, emit `"<i>"` and set `S` to `BI`; otherwise
      3. If `S` is `BI`, emit `"</i>"` and set `S` to `B`; otherwise
      4. If `S` is `IB`, emit `"</b></i><b>"` and set `S` to `B`; otherwise
      5. If `S` is `None`, emit `"<i>"` and set `S` to `I`.

   5. Let `LA` equal `A + N`.

5. Emit `Input[LA..]`;
6. If `S` is `B` or `IB`, emit `"</b>"`;
7. If `S` is `I` or `IB` or `BI`, emit `"</i>"`;
8. If `S` is `BI`, emit `"</b>"`;

## External links

While `S` is not the position of the end of the input:

1. Let `S` be the position of the character `'['`;
2. Let `E` be the position of the next character `'['`, or the end of the input
   if there is no next character;
3. Let `US` be `S + 1`;
4. If the text at `US` is not a case-insensitive match for one of the configured
   URI schemes, emit the range `S..E`, let `S` equal `E`, and restart the loop;
   otherwise
5. Advance `UE` to the position after the match;
6. If the text at `UE` is not a fuzzy IP address[^fuzzyip] or character in the
   URL set[^urlset], emit the range `S..E`, let `S` equal `E`, and restart the
   loop; otherwise
7. Advance `UE` to the position after the match;
8. Match zero or more characters in the URL set[^urlset] and advance `UE` to
   the position at the end of the match;
9. Let `TS` be the position of the end of the regular expression match `\p{Zs}*`
   starting at `UE`;
10. Let `TE` be the position of the next character at or after `TS` that is one
    of `[']'|'\x00'..='\x08'|'\x0a'..='\x1f'|'\u{FFFD}']`;
11. If the character at `TE` is not `']'`, emit the range `S..E`, let `S` equal
    `E`, and restart the loop; otherwise
12. Let `Break` be the position of the first `"&lt;"` or `"&gt;"` in the range
    `US..UE`[^ltgt];
13. If `Break` is none:

    1. Let `URL` be the text in the range `US..UE`;
    2. Let `Text` be the text in the range `TS..TE`.

    Otherwise:

    1. Let `URL` be the text in the range `TS..Break`;
    2. Let `Text` be the interpolation `{Input[Break..UE]} {Input[TS..TE]}`.

14. Run the [maybe make external image algorithm](#maybe-external-image) on
    `Text`;
15. If `Text` is empty:

    1. Let `Link Type` be `autonumber`;
    2. Increment the global counter `Link Ordinal` by 1;
    3. Let `Text` be the interpolation `"[{Link Ordinal}]"`.

    Otherwise:

    1. Let `Link Type` be `text`.

16. If `URL` starts with a configured URI scheme other than `"//"`, and `Text`
    does not contain `["-{"|"}-"]` let `Text` be the interpolation
    `"-{{R|{Text}}}-"`[^rawtext];
17. Run the [URL cleaning algorithm](#url-cleaning) on `URL`;
18. If the `URL` is not on the configurable whitelist of followable domains, let
    `Rel` be `"rel=\"nofollow\""`;
19. Emit a hyperlink with `"href"` attribute value `URL`, `"class"` attribute
    of the interpolation `"external {Link Type}"`, attribute `Rel`, and body
    `Text`;
20. Let `S` equal `E`.

[^ltgt]: These may be entities present in the original Wikitext or they may have
  been created by a literal `'<'` or `'>'` being replaced in an
  [earlier step](#escape-invalid-tags).

[^rawtext]: This escape sequence prevents the language converter from performing
  term replacement on the text.

[^urlset]: The negative character set
  `[^']'|'['|'<'|'>'|'"'|'\x00'..='\x20'|'\x7F'|\p{Zs}|'\u{FFFD}']`.

### Maybe external image

1. Let `URL` be the input;
2. Let `Images From` be the configurable list of URL prefixes that are allowed
   when hotlinking external images;
3. Set `Imagelike` if the `URL` is a syntactically valid URL string starting
   with `["http://"|"https://"]` and ending with a `'.'` followed by a
   case-insensitive match of `["avif|"gif"|"jpg"|"jpeg"|"png"|"svg"|"webp"]`.
4. Set `Allowed` if:

   1. `Imagelike` is set and the configuration allows all external image
      hotlinks; or
   2. `Imagelike` is set and `URL` starts with one of the strings in the
      configurable list of allowed external image URL prefixes; or
   3. `URL` matches any case-insensitive regular expression in the list
      computed by:

      1. Let `Whitelist` be the text of the interface message
         `"external_image_whitelist"`;
      2. Split `Whitelist` by `'\n'` and remove any lines that are empty or
         start with `'#'`;
      3. Treat each remaining line as a case-insensitive regular expression.

5. If `Allowed` is set:

   1. Let `Name` be the HTML entity encoded filename part of `URL`;
   2. HTML entity encode `URL`;
   3. Emit the interpolation `"<img src=\"{URL}\" alt=\"{Name}\">"`.

   Otherwise;

6. Emit none.

## Magic links

Let `P` be 0. While `P` is not the position of the end of the input:

1. Let `Skip` be a match of the regular expression `<a[ \t\r\n].*?</a>|<.*?>`;
2. If `Skip` is not none:

   1. Let `End` be the end position of the match;
   2. Emit the text in the range `P..End`;
   3. Let `P` equal `End`.

   Otherwise;

3. Let `URL` be the text matching a word boundary followed by:

   1. A configured absolute URI scheme, captured as `Scheme`; then
   2. An fuzzy IP address[^fuzzyip], or character in the URL set[^urlset]; then
   3. Zero or more characters in the URL set[^urlset].

   If `URL` is not none:

   1. Let `Start` and `End` be the start and end positions of `URL`;
   2. Let `Break` be the position of the first character or HTML entity in `URL`
      that decodes to `['<'|'>'|'\u{00a0}']`[^ltgt];
   3. If `Break` is not none, truncate `URL` at `Break`;
   4. Let `Trailing Punctuation` be `[','|';'|'\\'|'.'|':'|'!'|'?']`;
   5. If `URL` does not contain `(`, add `)` to `Trailing Punctuation`;
   6. Trim `Trailing Puncuation` from the end of `URL`, excluding any `';'` that
      is the terminator for an HTML entity;
   7. If the length of `URL` is not greater than the length of `Scheme`, emit
      text from the range `Start..End`, let `P` equal `End`, and restart the
      loop; otherwise
   8. Let `E` be the end position of `URL`;
   9. Run the [URL cleaning algorithm](#url-cleaning) on `URL`;
   10. Run the [maybe make external image algorithm](#maybe-external-image) on
      `URL`;
   11. If the result of the previous step is not none, emit its result;
       otherwise
   12. If the `URL` is not on a whitelist of followable domains, let `Rel` be
       `"rel=\"nofollow\""`;
   13. Emit text in the range `P..Start`;
   14. Emit a hyperlink with `"href"` attribute value `URL`, `"class"` attribute
      `"external free"`, attribute `Rel`, and body `URL`;
   15. Emit text in the range `E..End`;
   16. Let `P` equal `End`.

   Otherwise;

4. If magic links are enabled for RFC in the configuration, match a word
   boundary followed by:

   1. The string `"RFC"`; then
   2. At least one magic space[^magicspace]; then
   3. One or more `['0'..='9']`, captured as `ID`; then
   4. A word boundary.

   On success:

   1. Let `Start` and `End` be the start and end positions of the match;
   2. Let `URL` be the result of interpolating the localised interface message
      `"pubmedurl"` with argument `ID`;
   3. Emit text in the range `P..Start`;
   4. Emit a hyperlink with `"href"` attribute value `URL`, `"class"` attribute
      `"external mw-magiclink-rfc"`, and body the interpolation `"RFC {ID}"`;
   5. Let `P` equal `End`.

   Otherwise;

5. If magic links are enabled for PMID in the configuration, match a word
   boundary followed by:

   1. The string `"PMID"`; then
   2. At least one magic space[^magicspace]; then
   3. One or more `['0'..='9']`; then
   4. A word boundary.

   On success:

   1. Let `Start` and `End` be the start and end positions of the match;
   2. Let `URL` be the result of interpolating the localised interface message
      `"rfcurl"` with argument `ID`;
   3. Emit text in the range `P..Start`;
   4. Emit a hyperlink with `href` attribute value `URL`, `"class"` attribute
      `"external mw-magiclink-pmid"`, and body the interpolation
      `"PMID {ID}"`;
   5. Let `P` equal `End`.

   Otherwise;

6. If magic links are enabled for ISBN in the configuration, match a word
   boundary followed by:

   1. The string `"ISBN"`; then
   2. At least one magic space[^magicspace]; then
   3. The following, captured as `ID`:

      1. Optionally, `["978"|"979"]`, plus one optional magic space or hyphen;
         then
      2. A sequence of nine:

         1. `['0'..='9']`; then
         2. One optional magic space or hyphen;

      then;

      3. `['0'..='9'|'X'|'x']`.

      Then;
   4. A word boundary.

   On success:

   1. Let `Start` and `End` be the start and end positions of the match;
   2. Let `ISBN` be the `ID`, with magic spaces replaced with a space character;
   3. Let `N` be `ISBN` where all `['-'|' ']` are removed and `'x'` is replaced
      with `'X'`;
   4. Let `Page` be the configured alias for the special page `Booksources`;
   5. Let `URL` be the [partial URL](#title-glossary) of the title constructed
      using the interpolation `"Special:{Page}/{N}"`;
   6. Emit text in the range `P..Start`;
   7. Emit a hyperlink with `"href"` attribute value `URL`, `"class"` attribute
      `"internal mw-magiclink-isbn"`, and body the interpolation
      `"ISBN {ISBN}"`;
   8. Let `P` equal `End`.

   Otherwise;

7. Let `End` be the position of the end of the input;
8. Emit text in the range `P..End` and terminate here.

[^magicspace]: An HTML entity corresponding to a non-break space or `\p{Zs}`.

## Outlining

1. Let `Used IDs` be a global map with a string key and integer value;
2. Assert all inner text styles and extension tags have been converted to HTML
   tags.

For each balanced HTML tag in the input with a tag name
`["h1"|"h2"|"h3"|"h4"|"h5"|"h6"]`:

1. Let `Outline HTML` be the filtered subset of HTML content from the tag body:

   1. All text nodes;
   2. All non-empty tags `["b"|"bdi"|"i"|"q"|"s"|"strike"|"sub"|"sup"]`,
      without attributes;
   3. All non-empty `span` tags, without attributes other than `dir`.

2. Let `ID` be:

   1. If the tag has an `"id"` attribute, the value of the `"id"` attribute;
      otherwise
   2. Using the concatenation of all text nodes in the tag body:

      1. Replace each run of `[' '|'_']` with a single space character;
      2. Trim space characters from the start and end of the string;
      3. Decode HTML entities using the MediaWiki rules[^entity];
      4. Replace each run of Title spaces[^titlews] with a single space
         character;
      5. Trim Title trimmables[^titletr] from the end of the string;
      6. URL encode the string using the MediaWiki anchor encoding
         rules[^urlencode].

3. Let `F` be the ASCII lowercase case-folded `ID`;
4. Let `S` be the value from `Used IDs` for the key `F`;
5. If `S` is not none:

   1. Let `C` be `F`;
   2. While `C` is a key in `Used IDs`:

      1. Let `S` be `S + 1`;
      2. Let `C` be the interpolation `"{F}_{S}"`.

   3. Insert 1 to `Used IDs` for the key `C`;
   4. Replace `S` to `Used IDs` for the key `F`;
   5. Let `ID` be the interpolation `"{ID}_{S}"`.

   Otherwise;

6. Insert 2 to `Used IDs` for the key `F`;
7. Insert or replace the `"id"` attribute value from the original HTML tag with
   `ID`.

## Unstrip

For each [strip marker](#strip-marker) `M` in the input:

1. If the unstrip mode is `nowiki` and `M` is a `nowiki` marker, or if the
   unstrip mode is `general` and `M` is not a `nowiki` marker, or if the unstrip
   mode is `both`, replace the strip marker with the stored text matching the
   strip marker.

## “Block-level” Wikitext

For each line ending in `'\n'`:

1. Let `Line Start` be the input line start flag[^blstart], defaulting to set;
2. Let `Pre Open Match`, `Pre Close Match`, `In Pre`, `In Blockquote`,
   and `In Block Elem` be flags;
3. Let `Pending P Tag` and `Last Prefix` be empty;
4. <a name="bl3"></a>
   Let `L` be the current line;
5. If `Line Start` is clear:

   1. Set `Line Start`;
   2. Emit `L` and restart from the [previous step](#bl3).

6. Set `Pre Close Match` if `L` contains ASCII case-insensitive `"</pre"`;
7. Set `Pre Open Match` if `L` contains ASCII case-insensitive `"<pre"`;
8. If `In Pre` is set:

   1. Let `Prefix` and `Prefix 2` be empty;
   2. Let `T` equal `L`.

   Otherwise;

   1. Let `Prefix` be the sequence of characters matching `['*'|'#'|';'|':']` at
      the start of `L`;
   2. Let `Prefix 2` be `Prefix` with `';'` replaced by `':'`;
   3. Let `T` be the rest of the line after `Prefix`;
   4. Let `In Pre` equal `Pre Open Match`.

9. If `Prefix` is not empty and `Prefix 2` equals `Last Prefix`:

   1. Emit the [next item](#next-item) for the last character of `Prefix`;
   2. Let `Pending P Tag` be empty;
   3. If the last character in `Prefix` is `';'`:

      1. Let `P` be the result of [find colon no links](#find-colon-no-links)
         for `T`;
      2. If `P` is not none:

         1. Let `T` be `T[P + 1..]`;
         2. Emit the ASCII whitespace trimmed `T[..P]`;
         3. Emit the [next item](#next-item) for `':'`.

   Otherwise;

10. If `Prefix` is not empty or `Last Prefix` is not empty:

    1. Let `Common Prefix Length` be the length of the initial sequence of
       characters that are identical in both `Prefix` and `Last Prefix`;
    2. For each character `I` in `Last Prefix` that is not in `Prefix`, in
       right to left order, emit the result of [closing the list](#close-list)
       for `I`;
    3. If `Common Prefix Length` is not zero:

       1. If the length of `Prefix` equals `Common Prefix Length`, emit the
          [next item](#next-item) for the character at
          `Prefix[Common Prefix Length - 1]`;
       2. If `DT Open` is true and `Common Prefix Length` and the character at
          `Prefix[Common Prefix Length - 1]` is `':'`, emit the
          [next item](#next-item) for `':'`.

    4. If `Last Prefix` is not empty and the length of `Prefix` is greater than
       `Common Prefix Length`, emit `'\n'`;
    5. For each character `I` in `Prefix` starting at `Common Prefix Length`,
       from left to right:

       1. Emit the result of [opening the list](#open-list) for `I`;
       2. If `I` is `';'`:

          1. Let `P` be the result of [find colon no links](#find-colon-no-links)
             for `T`;
          2. If `P` is not none:

             ※ This allows runs of `:` to be converted into multiple `<dd>`,
               but only when the depth of the list is increasing, and should
               probably be considered undefined behaviour.

             1. Let `T` be `T[P + 1..]`;
             2. Emit the ASCII whitespace trimmed `T[..P]`;
             3. Emit the [next item](#next-item) for `':'`.

    6. If `Prefix` is empty and `Last Prefix` is not empty, emit `'\n'`;
    7. Let `Last Prefix` equal `Prefix 2`.

11. If `Prefix` is empty:

    1. Set `Open Match` if `T` contains an HTML start tag with a tag name
       matching the block list[^blockelem], or an HTML end tag with a tag name
       matching the anti-block list[^antiblockelem], or an HTML tag of either
       kind with the tag name `["tr"|"caption"|"dt"|"dd"|"li"]`;
    2. Set `Close Match` if `T` contains an HTML end tag with a tag name
       matching the block list[^blockelem], or an HTML start tag with a tag name
       matching the anti-block list[^antiblockelem], or an HTML tag of either
       kind with the tag name
       `["center"|"blockquote"|"div"|"hr"|"aside"|"figure"]`[^excludedelem];
    3. If `Open Match` is set or `Close Match` is set:

       1. Let `Pending P Tag` be empty;
       2. If `In Pre` is clear or `Pre Open Match` is set, emit the
          [closing paragraph](#close-paragraph);
       3. If `Pre Open Match` is set and `Pre Close Match` is clear, set
          `In Pre`;
       4. If the last case-insensitive match of the regular expression
          `<(/?)blockquote[\s>]` in `T` is not none, clear `In Blockquote` if
          the capture group is `"/"`, otherwise set `In Blockquote`;
       5. Set `In Block Elem` if `Close Match` is clear, otherwise clear
          `In Block Elem`;

       Otherwise;

    4. If `In Block Elem` is clear and `In Pre` is clear:

       1. If the first character in `T` is a space character; and
       2. `Last Paragraph` is `"pre"` or the ASCII whitespace trimmed `T` is not
          empty; and
       3. If `In Blockquote` is clear; then:

          1. If `Last Paragraph` is not `"pre"`:

             1. Let `Pending P Tag` be empty;
             2. Emit the [closing paragraph](#close-paragraph);
             3. Emit `"<pre>"`;
             4. Let `Last Paragraph` be `"pre"`.

          2. Remove the first character from `T`.

          Otherwise;

       4. If `T` starts with one or more matches of a complete HTML tag with tag
          name `["style"|"link"]`, each optionally trailed by ASCII whitespace:

          1. If `Pending P Tag` is not empty, emit the
             [closing paragraph](#close-paragraph) and let `Pending P Tag` be
             empty.

          Otherwise;

       5. If the ASCII whitespace trimmed `T` is empty:

          1. If `Pending P Tag` is not empty:

             1. Emit `Pending P Tag`;
             2. Emit `"<br>"`;
             3. Let `Pending P Tag` be empty;
             4. Let `Last Paragraph` be `"p"`.

             Otherwise;

          2. If `Last Paragraph` is not `'p'`, emit the
             [closing paragraph](#close-paragraph) and let `Pending P Tag` be
             `"<p>"`; otherwise
          3. Let `Pending P Tag` be `"</p><p>"`.

          Otherwise;

       6. If `Pending P Tag` is not empty:

          1. Emit `Pending P Tag`;
          2. Let `Pending P Tag` be empty;
          3. Let `Last Paragraph` be `"p"`.

          Otherwise;

       7. If `Last Paragraph` is not `"p"`:

          1. Emit the [closing paragraph](#close-paragraph);
          2. Emit `"<p>"`;
          3. Let `Last Paragraph` be `"p"`.

12. If `Pre Close Match` is set and `In Pre` is set, clear `In Pre`;
13. If `Pending P Tag` is empty:

    1. If `Prefix` is empty:

       1. Emit `T`;
       2. If `Not Last Line` is set or `Last Paragraph` is not empty, emit
          `'\n'`.

       Otherwise;

    2. Emit the ASCII whitespace trimmed `T`.

After the last line:

1. For each character `I` in `Prefix 2`, from right to left,
   emit the result of [closing the list](#close-list) for `I`;
2. If `Prefix 2` is not empty and `Last Paragraph` is not empty, emit `'\n'`;
3. Emit the [closing paragraph](#close-paragraph) with `At The End` set.

[^antiblockelem]: `["td"|"th"]`

[^blockelem]: `["table"|"h1"|"h2"|"h3"|"h4"|"h5"|"h6"|"pre"|"p"|"ul"|"ol"|"dl"]`

[^blstart]: This flag may be set when parsing Wikitext fragments in
  the `#interwikilink` parser function and the `<indicator>` extension tag.

[^excludedelem]: The MediaWiki algorithm also looks for tag names that start
                 with `mw:` or that match `meta property="mw:`. These are
                 implementation details.

### Find colon no links

Return the next position of `':'` in the input that is not inside an HTML tag,
the body of an HTML tag, or a language conversion tag.

### Close paragraph

1. If `Last Paragraph` is not empty:

   1. Emit the interpolation `"</{Last Paragraph}>"`;
   2. If `At The End` is clear, emit `'\n'`;

2. Clear `In Pre`;
3. Let `Last Paragraph` be empty.

### Open list

1. If `Char` is `'*'`, emit `"<ul><li>"`; otherwise
2. If `Char` is `'#'`, emit `"<ol><li>"`; otherwise
3. If `Char` is `':'`, emit `"<dl><dd>"`; otherwise
4. If `Char` is `';'`, emit `"<dl><dt>"` and set `DT Open`.

### Close list

1. If `Char` is `'*'`, emit `"</li></ul>"`; otherwise
2. If `Char` is `'#'`, emit `"</li></ol>"`; otherwise
3. If `Char` is `':'`:

   1. If `DT Open` is set, clear `DT Open` and emit `"</dt></dl>"`; otherwise
   2. Emit `"</dd></dl>"`.

### Next item

1. If `Char` is `['*'|'#']`, emit `"</li>\n<li>"`; otherwise
2. If `Char` is `[':'|';']`:

   1. If `DT Open` is set, emit `"</dt>\n"`; otherwise, emit `"</dd>\n"`;
   2. If `Char` is `';'`, set `DT Open` and emit `"<dt>"`; otherwise, clear
      `DT Open` and emit `"<dd>"`.

## Language conversion

Conceptually, language conversion is a state machine that runs in a single pass
over the raw Wikitext. When a conversion tag is encountered, the tag may either
mutate the state of the converter to add or remove terms from the translation
map, or it may emit some new text. For each run of plain text in the input,
substrings that match a key in the translation map are replaced by the
corresponding value from the map. The special `T` translation tag registers a
replacement title for the page through a side channel.

If language conversion is enabled in the configuration:

1. Let `Variant` be the preferred language variant;
2. Let `S` be 0;
3. Let `Translations` be a map;
4. Let `Title` be empty;

While `S` is not the position of the end of the input:

1. Let `P` be the position of the next HTML tag, HTML entity, character sequence
   `"-{"`, or end of the input;
2. Emit an [auto-converted value](#auto-conversion) for the text in the range
   `S..P`;
3. If `P` is an HTML start tag:

   1. If the tag name is one of `["code"|"pre"|"script"|"style"|"math"|"svg"]`,
      let `S` be the position after the end of the corresponding end tag, or the
      end of the input if there is no corresponding end tag; otherwise
   2. Let `A` be the result of [parsing the attributes](#parse-attributes) from
      the tag;
   3. For each attribute in `A` with the name `["alt"|"title"]`, if the value
      does not contain the character sequence `"://"`, replace the attribute
      value with the result of running
      [language conversion](#language-conversion) recursively on the attribute
      value;
   4. Emit the tag;
   5. Let `S` be the position after the tag.

   Otherwise;

4. If `P` is an HTML end tag, emit the tag and let `S` be the position after the
   tag; otherwise
5. If `P` is at the character sequence `"-{"`, let `R` be the result and `S` be
   the position after [processing a conversion tag](#process-conversion-tags)
   using the text in the range `P..` and references to `Translations` and
   `Title`;
6. Emit `R`.

### Process conversion tags

※ To avoid excessive recursion, vendors may wish to set a depth limit for
  recursion into this sequence.

1. Let `C` be a reference to the input converter’s translation map and `CT`
   a reference to the input converter’s currently stored page title;
2. Let `I` be the input text;
3. Assert that `I` starts with `"-{"`;
4. Let `S` be the start position;
5. Let `O` be empty;
6. Let `P` be the next position of the character sequence `["-{"|"}-"]`;
7. If `P` is none:

   1. Append the text in range `S..` to `O`;
   2. Emit `"-{"`;
   3. Emit the result of running [language conversion](#language-conversion) on
      `O` and terminate here.

   Otherwise;

8. Append the text in range `S..P` to `O`;
9. Let `S` equal `P`;
10. If the sequence at `P` is `"-{"`:

    1. Let `P` equal `P + 2`;
    2. Append the result of running [language conversion](#language-conversion)
       to `O`.

   Otherwise;

11. Assert the sequence at `P` is `"}-"`;
12. Let `P` equal `P + 2`;
13. Let `Title`, `Action`, `Display`, and `Conv Table` be the result of
    [parsing a rule](#language-rule-parsing) using `O` as the input and
    `Variant` as the language code;
14. If `Title` is not none, let `CT` equal `Title`;
15. For each key `Variant` and value `Pair` in `Conv Table`:

    1. Let `Norm V` be the [normalised](#variant-normalisation) `V`;
    2. If `Norm V` is none, continue to the next iteration of the loop; otherwise
    3. Let `T` be a reference to the map from `C` with the key `Norm V`;
    4. If `Action` is `add`, merge `Pair` into `T`; otherwise
    5. If `Action` is `remove`, remove all entries from `T` whose keys match the
       keys given in `Pairs`.

### Language rule parsing

1. Let `Variant` be the passed language code;
2. Let `Flags` be a set;
3. If the input contains `'|'`, let `F` and `Rules` be the left and right hand
   side of the input split at `'|'`, otherwise let `F` be empty and `Rules` be
   the input;
4. For each `Flag` in `F` separated by `';'`:

   1. Trim ASCII whitespace from the start and end of `Flag`;
   2. If `Flag` is one of `['A'|'T'|'R'|'D'|'-'|'H'|'N']` or a configured
      language variant code, set `Flag` in `Flags`.

5. If `Flags` is empty, set `'S'` on `Flags`; otherwise
6. From left to right, if `['R'|'N'|'-']` is in `Flags`, clear all other flags
   in `Flags`; otherwise
7. If `'T'` is in `Flags` and the length of `Flags` is 1, set `'H'` on `Flags`;
   otherwise
8. If `'H'` is in `Flags`, replace `Flags` with the set
   `[ '+', 'H', Flags.T, Flags.D ]`; otherwise:

   1. If `'A'` is in `Flags`, set `'+'` and `'S'` on `Flags`;
   2. If `'D'` is in `Flags`, clear `'S'` on `Flags`;
   3. If `Flags` contains any configured language variant codes, let
      `Variant Flags` be `Flags` and clear `Flags`.

9. If `Variant Flags` is not none:

   1. If `Variant` is in `Variant Flags`:

      1. Let `Rules` be the result of running
         [language conversion](#language-conversion) using `Rules` as the input
         and `Variant` as the variant.

      Otherwise:

      1. Let `Fallbacks` be the configured list of fallback variants for
         `Variant`;
      2. Let `Fallback` be the first value in `Fallbacks` that is in
         `Variant Flags`;
      3. If `Fallback` is not none, let `Rules` be the result of running
         [language conversion](#language-conversion) using `Rules` as the input
         and `Fallback` as the variant.

   2. Let `Flags` be the set `[ 'R' ]`.

10. If `'R'` is not in `Flags` and `'N'` is not in `Flags`:

    1. Replace `"=&gt;"` with `"=>"` in `Rules`[^ltgt];
    2. Let `Bidir Table` and `Unidir Table` be the result of
       [running the rules parsing algorithm](#parse-rules) using `Rules`.

11. If `Bidir Table` is none and `Unidir Table` is none:

    1. If `['+'|'-']` is in `Flags`:

       1. If `Rules` is not empty, for each variant `Variant` in the configured
          list of language variants, set the value of the key `Variant` in
          `Bidir Table` to `Rules`.

       Otherwise;

    2. If `'N'` is not in `Flags` and `'T'` is not in `Flags`, let `Flags` be the
       set `[ 'R' ]`.

12. Set `Error`;
13. For `Flag` in `Flags`:

    1. If `Flag` is `'R'`, let `Display` be `Rules` and clear `Error`; otherwise
    2. If `Flag` is `'N'`:

       1. Let `Variant` be the result of converting the ASCII whitespace trimmed
          `Rules` from a BCP-47 code to an internal language code;
       2. Let `Display` be the localised name of the language code given by
          `Variant`, or empty if the language code is not valid;
       3. Clear `Error`.

       Otherwise;

    3. If `Flag` is `'D'`, let `Display` be the
       [rules description](#rules-description) using `Bidir Table` and
       `Unidir Table`; otherwise
    4. If `Flag` is `'H'`, let `Display` be empty and clear `Error`; otherwise
    5. If `Flag` is `'-'`, let `Action` be `remove` and `Display` be empty and
       clear `Error`; otherwise
    6. If `Flag` is `'+'`, let `Action` be `add` and `Display` be empty and clear
       `Error`; otherwise
    7. If `Flag` is `'S'`, let `Display` be the
       [rule converted value](#rule-converted-value) of `Variant` if one exists,
       otherwise set `Error`; otherwise
    8. If `Flag` is `'T'`:

       1. Let `Title` be the rule converted value for title of `Variant` if one
          exists, otherwise set `Error`;
       2. Let `Display` be empty.

14. If `Error` is set, let `Display` be an [error string](#error-string) with
    a conversion failure message;
15. Let `Conv Table` be the result of running the
    [conversion table generator](#generate-conversion-table) with `Bidir Table`
    and `Unidir Table`;
16. Return `Title`, `Action`, `Display`, and `Conv Table`.

### Parse rules

1. Let `Rules` be the input;
2. Let `S` be 0;
3. Let `Vs` be the case-sensitive list of configured language codes and their
   corresponding BCP 47 codes in a non-capturing regular expression alternates
   group (i.e. `(?:A|B|C)`);
4. While `S` is not the position of the end of the input:

   1. Let `P` be the position of the next `';'` which is not an HTML entity
      terminator and which matches the interpolated regular expression
      `;\s*(?:([^;]*?=>\s*)?{Vs}\s*:|$)`, or the position of the end of
      `Rules` if there is no match;
   2. Let `Choice` be the text in the range `S..P`;
   3. Let `S` equal `P + 1`;
   4. If `Choice` does not contain `:`, restart from step 4; otherwise
   5. Let `V` and `To` be the left and right hand sides of `Choice` split at
      `:`;
   6. Trim ASCII whitespace from `V` and `To`;
   7. If `V` contains `"=>"`:

      1. Let `From` and `V` be the left and right hand sides of `V` split at
         `"=>"`;
      2. Trim ASCII whitespace from `From` and `V`;
      3. Let `Norm V` be the [normalised](#variant-normalisation) `V`;
      4. If `Norm V` is not none, `From` is not empty, `Norm V` is a key in
         `Unidir Table`, and the value from `Unidir Table` with the key `Norm V`
         is not an index map, replace the value with the key `Norm V` in
         `Unidir Table` with the index map `[ From => To ]`; otherwise
      5. If `From` is not empty and `Norm V` is not none, let `Table` be the
         value from `Unidir Table` with the key `Norm V` and insert `To` to
         `Table` with the key `From`.

      Otherwise:

      1. Let `Norm V` be the [normalised](#variant-normalisation) `V`;
      2. If `To` is not empty and `Norm V` is not none, insert `To` to
         `Bidir Table` with the key `Norm V`.

   8. If `Norm V` is none or `Norm V` has no localised display name, clear
      `Bidir Table` and `Unidir Table` and terminate here.

### Generate conversion table

1. Let `Bidir Table` and `Unidir Table` be the bidirectional translation and
   unidirectional translation tables;
2. Let `Marked` be a list;
3. Let `Conv Table` be a map;
4. For each configured variant `Variant`:

   1. If `Variant` is not a key in `Bidir Table`:

      1. Let `Fallbacks` be the configured list of fallback variants for
         `Variant`;
      2. Let `Fallback` be the first value in `Bidir Table` with a corresponding
         key in `Fallbacks`;
      3. If `Fallback` is not none, insert `Fallback` to `Bidir Table` with the
         key `Variant`.

   2. If `Variant` is a key in `Bidir Table`:

      1. For each `VO` in `Marked`:

         1. If the [converter’s manual level](#manual-level) for `Variant` is
            `bidirectional`:

            1. Let `K` be the value from `Bidir Table` with the key `VO`;
            2. Let `V` be the value from `Bidir Table` with the key `Variant`;
            3. Let `T` be a reference to the map in `Conv Table` with the key
               `Variant`;
            4. Insert `V` to `T` with the key `K`.

         2. If the [converter’s manual level](#manual-level) for `VO` is
            `bidirectional`:

            1. Let `K` be the value from `Bidir Table` with the key `Variant`;
            2. Let `V` be the value from `Bidir Table` with the key `VO`;
            3. Let `T` be a reference to the map in `Conv Table` with the key
               `VO`;
            4. Insert `V` to `T` with the key `K`.

      2. Append `Variant` to `Marked`.

   3. If the [converter’s manual level](#manual-level) for `Variant` is
      `["bidrectional"|"unidirectional"]` and `Variant` is a key in
      `Unidir Table`:

      1. Let `V` be the list with the key `Variant` from `Unidir Table`;
      2. If `Variant` is a key in `Conv Table`, splice `V` to the beginning of
         the corresponding value; otherwise
      3. Insert `V` to `Conv Table` using the key `Variant`.

5. Return `Conv Table`.

### Rules description

1. Let `Code Separator` and `Variant Separator` be the converter’s
   [code and variant separators](#code-and-variant-separators);
2. For each key `Variant` and value `Text` in `Bidir Table`:

   1. Emit the localised name of `Variant`;
   2. Emit `Code Separator`;
   3. Emit `Text`;
   4. Emit `Variant Separator`.

3. For each key `Variant` and value `Conversion` in `Unidir Table`:

   1. For each key `From` and value `To` in `Conversion`:

      1. Emit `From`;
      2. Emit `'⇒'`;
      3. Emit the localised name of `Variant`;
      4. Emit `Code Separator`;
      5. Emit `To`;
      6. Emit `Variant Separator`.

### Rule converted value

1. Let `Bidir Table` be the bidirectional table;
2. Let `Unidir Table` be the unidirectional table;
3. Let `Variant` be the language variant;
4. If `Bidir Table` and `Unidir Table` are empty, emit `Rules` and terminate
   here;
5. Let `Display` be the value from `Bidir Table` with the key `Variant`;
6. If `Display` is none, let `Display` be the value from `Bidir Table` with the
   first key that matches a configured fallback variant of `Variant`;
7. If `Display` is none, let `Display` be the first value of the list of values
   from `Unidir Table` with the key `Variant`;
8. If `Display` is none and the [converter’s manual level](#manual-level) for
   `Variant` is `disable`:

   1. If `Bidir Table` is empty, let `Display` be the first value of the list of
      values from the first value of `Unidir Table`; otherwise
   2. Let `Display` be the first value from `Bidir Table`.

9. Return `Display`.

### Code and variant separators

As of writing, the code and variant separators are:

* gan: `:` and `; `
* wuu: `：` and `；`
* zh: `：` and `；`
* all others: `:` and `;`

### Manual level

Each language converter specifies a map of language codes to “manual levels”,
which define which explicit conversions are enabled. A “manual level” can be one
of `["disable"|"unidirectional"|"bidirectional"]`.

As of writing, the “manual levels” are:

* gan: disable
* wuu: disable
* zh: disable
* zh-hans: unidirectional
* zh-hant: unidirectional

### Variant normalisation

1. Let `Variant` be the input;
2. Convert `Variant` to ASCII lowercase;
3. Replace `Variant` with its non-deprecated equivalent;
4. If `Variant` is in the list of configured variants, return it;
5. For each configured variant `R`, if the ASCII lowercase BCP 47 code of `R`
   equals `Variant`, return `R`;
6. Return none.

## Guard quotes

For the text content of each text node:

1. For each character `['?'|':'|';'|'!'|'%'|'»'|'›']` preceded by a space
   character and not followed by a Unicode word character, replace the preceding
   space with a no-break space;
2. For each character `['«'|'‹']` not preceded by a Unicode word character and
   followed by a space character, replace the following space character with a
   no-break space.

## P-wrapping

For each node `N` with parent node `P`:

1. If `N` is a text node:

   1. If `P` is a p-wrapping root[^proot] and `N` contains characters other than
      ASCII whitespace:

      1. Let `W` be a new `mw:pwrap` element;
      2. Insert `W` before `N` in `P`;
      3. Move `N` into `W`.

      Otherwise, if `P` is splittable[^splittable] and has no ancestor
      `mw:pwrap`, [split the stack](#split-the-stack) with node `N`.

   Otherwise;

2. If `N` is not an inline element[^inline] and `P` is `mw:pwrap`, move `N` to
   the parent of `P`; otherwise
3. If `P` is splittable[^splittable] and:

   1. `N` is an inline element[^inline] and `P` has no ancestor `mw:pwrap`; or
   2. `N` is not an inline element and `P` has an ancestor `mw:pwrap`.

   Then [split the stack](#split-the-stack) with node `N`; otherwise

4. If `P` is a p-wrapping root[^proot] and `N` is an inline element[^inline],
   [split the stack](#split-the-stack) with node `N`; otherwise
5. If `P` has an ancestor `mw:pwrap` and `N` is not an inline element[^inline]:

   1. Let `R` be the nearest p-wrapping root[^proot] ancestor of `P`;
   2. Let `DR` be the tree depth of `R`;
   3. Let `A` be the ancestor of `P` at tree depth `DR + 1`;
   4. Move `N` into `R` after `A`.

[^formatting]: The list of formatting elements defined by MediaWiki are `["a"`
  `|"b"|"big"|"code"|"em"|"font"|"i"|"nobr"|"s"|"small"|"strike"|"strong"|"tt"`
  `|"u"]`

[^inline]: The list of inline elements defined by MediaWiki are `["a"|"abbr"`
  `|"acronym"|"applet"|"audio"|"b"|"basefont"|"bdi"|"bdo"|"big"|"br"|"button"`
  `|"cite"|"code"|"data"|"del"|"dfn"|"em"|"font"|"i"|"iframe"|"img"|"input"`
  `|"ins"|"kbd"|"label"|"legend"|"map"|"mark"|"object"|"param"|"q"|"rb"|"rbc"`
  `|"rp"|"rt"|"rtc"|"ruby"|"s"|"samp"|"select"|"small"|"source"|"span"|"strike"`
  `|"strong"|"sub"|"sup"|"textarea"|"time"|"track"|"tt"|"u"|"var"|"video"`
  `|"wbr"]`

[^proot]: The list of p-wrapping roots is `["body"|"blockquote"]`.

### Split the stack

Using node `N`:

1. Let `R` be the nearest p-wrapping root[^proot] ancestor of `N`;
2. Let `DN` be the tree depth of `N`;
3. Let `DR` be the tree depth of `R`;
4. Let `A` be the ancestor of `N` at tree depth `DR + 1`;
5. Let `S` be a clone of the stack of elements in the depth range `DR + 1..DN`;
6. If `N` is a text node or inline element[^inline]:

   1. Let `W` be a new `mw:pwrap` element;
   2. Insert `W` after `A` in `R`;
   3. Append `S` to `W`.

   Otherwise, append `S` to `R`.

7. Move `N` into the deepest child of `S`.

## Format elements

For each element:

1. If the element has the tag name `mw:p-wrap`:

   1. If the element contains child nodes other than whitespace-only text nodes
      and comments:

      1. Emit `"<p>"`;
      2. Emit the element’s contents;
      3. Emit `"</p>"`.

   Otherwise, emit the element’s contents.

   Otherwise;

2. If the element has the tag name `["p"|"li"|"tr"]`, with no attributes, and
   the tag body consists only of ASCII whitespace:

   1. Let `Tag Name` be the tag name;
   2. Emit the interpolation `"<{Tag Name} class=\"mw-empty-elt\">"`;
   3. Emit the tag contents;
   4. Emit the interpolation `"</{Tag Name}>"`.

   Otherwise;

3. Emit `'<'`;
4. Emit the tag name;
5. For each attribute:

   1. Replace all instances of bare `['&'|'"']` in the attribute value with
      their respective entity encodings;[^formatattr]
   2. Emit the attribute as an HTML attribute.

6. Emit `'>'`;
7. If the tag is not an HTML void element:

   1. If the first character of the tag body is `'\n'` and the tag name is
      `["pre"|"textarea"|"listing"]`, emit `'\n'`;
   2. Emit the tag body;
   3. Emit `"</"`;
   4. Emit the tag name;
   5. Emit `'>'`.

[^formatattr]: MediaWiki does additional normalising of entities which is not
               necessary for correct rendering, so is excluded from this
               algorithm.

## Strip marker

Because strip markers are exposed to Lua scripts and parser functions, they MUST
match this format exactly:

1. The character sequence ``"\x7f'\"`UNIQ-"``;
2. The tag name of the extension tag;
3. The character `'-'`;
4. A lowercase hexadecimal ordinal which, in combination with the tag
   name, is unique within the *entire* document;
5. The character sequence ``"-QINU`\"'\x7f"``.

## Title

A valid Title is constructed from a string with an optional default namespace by
following these steps:

1. Let `Text` be the input after decoding HTML entities following the special
   MediaWiki HTML entity rules[^entity] and then normalising Unicode to NFC;
2. Replace each run of title spaces[^titlews] in `Text` with a single space
   character;
3. Trim title trimmables[^titletr] from the start and end of `Text`;
4. If `Text` starts with `':'`, delete the `':'` and set the `Maybe Main` flag;
5. If `Text` contains `':'`:

   1. Let `L` and `R` be the left and right hand side of `Text` split at `':'`;
   2. Trim a space character from the end of `L` and the start of `R`;
   3. If `L` matches a namespace alias, set `NS` to `L` and `Text` to `R`;
      otherwise
   4. If `L` matches an interwiki alias, set `IW` to `L` and `Text` to `R`.

6. If `NS` is not set:

   1. If `Text` contains `':'`:

      1. Let `L` and `R` be the left and right hand side of `Text` split at
         `':'`;
      2. Trim a space character from the start of `R`;
      3. If `L` matches a namespace alias, set `NS` to `L` and `Text` to `R`.

   Otherwise;

   2. If the `Maybe Main` flag is set, set `NS` to the main namespace; otherwise
   3. If a default namespace was given, set `NS` to the given namespace;
      otherwise
   4. Set `NS` to the main namespace.

7. If `Text` is empty, fail; otherwise
8. Let `Target` and `Hash` be the left and right hand side of `Text` split at
   `#`;
9. Trim a space character from the end of `Target`;
10. Check if `Target` is valid:

    1. Is not empty;
    2. Is not longer than 255 bytes (or 512 bytes if `NS` is the alias for the
       `Special` namespace);
    3. Is not equal to `.` or `..`;
    4. Does not start with `:`, `./`, or `../`;
    5. Does not end with `/.` or `/..`;
    6. Does not contain `~~~`, `/./`, or `/../`, HTML entities, or
       percent-encoded escape sequences;
    7. Contains only bytes in the configurable list of valid bytes.

11. If `Target` is not valid, fail; otherwise
12. Let `Title` be an empty string;
13. If `IW` is set, append the interpolation `"{IW}:"` to `Title`;
14. If `NS` is set:

    1. If the namespace’s case strategy is `First Letter`, case fold the
       first character of `NS` to uppercase;
    2. Append the interpolation `"{NS}:"` to `Title`.

15. Append `Target` to `Title`;
16. If `Hash` is set, append the interpolation `"#{Hash}"` to `Title`;
17. Return `Title`.

### Title glossary

```text
Interwiki:Namespace:Title/Sub/Page#Fragment
└─────────────────╴full╶──────────────────┘
└───────────╴prefixed╶───────────┘ └──1╶──┘ 1. fragment
└──╴2╶──┘ └────╴key/partial╶─────┘          2. interwiki
          └──╴3╶──┘ └───╴text╶───┘          3. namespace
                    └─╴base─┘ └4╶┘          4. subpage
                    └╴5╶┘                   5. root
```

To get a text is to get the corresponding substring from the title.

To get a URL is to get the corresponding substring from the title, replace
spaces with underscores, then URL encode all non-alphanumeric-ASCII bytes other
than `['-'|'_'|'.'|'!'|'$'|'('|')'|'*'|','|'/'|':'|';'|'@'|'~']`.

### Title link URL

1. Let `Title` be the title;
2. Let `Scheme` be the scheme;
3. Let `Query` be the query string;
4. If `Title` is external or `Scheme` is not none:

   1. Let `URL` be the [full](#title-glossary) URL;
   2. If `Query` is not none and not empty, let `Q` be `'?'`;
   3. Emit the interpolation `"{Scheme}{URL}{Q}{Query}"`.

   Otherwise;

5. Let `Fragment` be the [fragment](#title-glossary) of `Title`;
6. If the [prefixed](#title-glossary) text of `Title` is empty and `Fragment` is
   not empty, emit the interpolation `"#{Fragment}"`; otherwise
7. Emit the [local URL](#title-local-url) of `Title`.

### Title local URL

1. Let `Query` be the query string;
2. If `Query` is not none and not empty, let `Q` be `'?'`;
3. Let `URL` be the [partial](#title-glossary) URL of the title;
4. If `Fragment` is not none, let `H` be `'#'`;
5. Let `P` be the configuration article path;
6. Let `URL` be the interpolation `"{URL}{Q}{Query}{H}{Fragment}"`;
7. Replace `"$1"` in `P` with `URL`;
8. Emit `P`.

### Subpage resolution

If the input starts with `"../"` or `'/'`:

1. Let `Input` be the input title string;
2. Let `Text` be the input display text;
3. If the [namespace](#title-glossary) of the caller does not support subpages,
   return `Input` and terminate here; otherwise
4. Let `H` be the position of the first `'#'` in `Input`, or the position of the
   end of `Input` if there is no match;
5. Let `Target` be `Input[..H]` and `Hash` be `Input[H..]`;
6. Let `P` be the [prefixed text](#title-glossary) of the caller’s title;
7. Trim ASCII whitespace from `Target`;
8. If `Target` starts with `"../"`:

   1. Let `C` be the number of repetitions of `"../"` at the start of
      `Target`;
   2. Let `S` be `Target` with all repetitions of `"../"` trimmed from the
      start;
   3. If `C` is not below the number of `'/'` in `P`, emit `Target` and `Text`
      and terminate here; otherwise
   4. Using `'/'` as a delimiter, remove `C` segments from the end of `P`;
   5. If `S` ends with `'/'`:

      1. Trim all `'/'` from the end of `S`;
      2. If `Text` is empty, let `Text` be the interpolation `"{S}{Hash}"`.

   6. Trim ASCII whitespace from `S`;
   7. If `S` is not empty, prefix `'/'` to `S`;
   8. Let `Target` be the interpolation `"{P}{S}{Hash}"`.

   Otherwise;

9. If `Target` starts with `'/'`:

   1. Let `S` be `Target[1..]`;
   2. If `S` ends with `'/'`:

      1. Trim all `'/'` from the end of `S`;
      2. Let `Target` equal `S`.

   3. If `Text` is empty, let `Text` be the interpolation `"{Target}{Hash}"`.
   4. Trim ASCII whitespace from `S`;
   5. Let `Target` be the interpolation `"{P}/{S}{Hash}"`;

10. Emit `Target` and `Text`.

## URL cleaning

1. Decode HTML entities in `URL` following the special MediaWiki HTML entity
    rules[^entity];
2. URL encode bytes in `URL` matching
    `[']'|'['|'<'|'>'|'"'|'\x00'..='\x1f'|'\x7f'|'|']`;
3. Replace the space character in `URL` with `'+'`;
4. If the URL has a host-part:

    1. Remove characters in the host-part that are ignored in IDNs per RFC 8264;
    2. URL decode `['['|']']` in the host-part of `URL` that correspond to an
       IPv6 address.

</div>

[^entity]: The special MediaWiki entity rules are:

    1. Entities MUST end with `;`. (In HTML5, this is not required.)
    2. Non-standard entities `"&רלמ;"` and `"&رلم;"` decode to '\u{200f}'.

[^fuzzyip]: A match of the regular expression `[0-9.]+|\[[0-9A-Fa-f:.]+\]`.

[^ictag]: A pseudo-XML tag with a case-insensitive tag name
  `["includeonly"|"noinclude"|"onlyinclude"]`.

[^titletr]: `[' '|'\u{200e}'..='\u{200f}'|'\u{202a}'..='\u{202e}']`

[^titlews]: `['_'|' '|'\u{00a0}'|'\u{1680}'|'\u{180e}'|'\u{2000}'..='\u{200a}'`
            `|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}']`

[^urlencode]: For HTML5 mode, replace `['\t'|'\n'|'\x0c'|'\r'|' ']` with `'_'`.
  For legacy mode, percent encode all non-alphanumeric characters except for
  `[' '|'-'|'_'|'.'|':']`, then replace all `'%'` with `'.'` and all space
  character with `'_'`.
