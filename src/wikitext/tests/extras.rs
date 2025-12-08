use super::*;

macro_rules! run_extras_tests {
    ($($name:ident => $input:expr),* $(,)?) => {
        $(#[test]
        fn $name() {
            run_test_for_goldenfile(stringify!($name), $input);
        })*
    }
}

// TODO: These extra tests are basically just things that were broken during the
// initial implementation of the parser, and *hopefully* once the test suite
// taken from Parsoid is fully hooked up with expected outputs from *our* parser
// these can just go away
run_extras_tests! {
    arg_whitespace => "{{a | b = http://www.example.com/ | c = d}}",
    autolink_1 => "https://mediawiki.org\nhttps://mediawiki.org\npre http://example.com post\n<nowiki>https://mediawiki.org</nowiki>",
    autolink_2 => "<code>http://</code> or <code>https://</code> or http:// or https://",
    autolink_3 => "https://mediawiki.org.\n(https://mediawiki.org)\nhttps://mediawiki.org/a(b).\n",
    // TODO: Need to override config, these magic links are disabled by default
    autolink_4 => "ISBN 0-7475-3269-9 or ISBN 000000000x or ISBN 0-9&nbsp;9999-2222 or ISBN 978-00000-00000 or (invalid:) ISBN 938-00000-00000\nPMID 1923.23232\nRFC 42",
    balance => "'''b'''bi'' i\nn '''ii'' n\nn '''ii'' n",
    balance_2 => " '''b''' n'''i''n",
    balance_3 => "nn'''ib''' b'''i''n",
    extension_tags => "<ref/><ref>a</ref>",
    ext_broken => "<ref><!-- oops</ref>",
    heading_1 => r#"==<span id="Alternate Section Title"></span>Section heading=="#,
    image_1 => "[[File:filename.extension|alt=a|caption]]",
    include => "<noinclude>hello</noinclude><onlyinclude>hello</onlyinclude><includeonly>did i just typo before?</includeonly>",
    include_2 => "<includeonly>{{a}}</includeonly><noinclude>{{b}}</noinclude>",
    link_ampersand => "[[Hello & world]]",
    link_args => "[[Link|a|b=c|d=e=f]]mazing",
    link_stupid_args => "[[Link|link =lol]]",
    link_trail => "[[Yes]]yes [[No]]!!",
    link_with_kv_in_args => "{{a|[[b|alt=]]}}",
    links_1 => "[[Main Page]]

[[Help:Contents]]

[[Extension:DynamicPageList (Wikimedia)]]

[[Help:Editing pages#Preview|previewing]]

[[#See also|different text]]",
    links_2 => "[[Help]]s

[[Help]]<nowiki />ful advice",
    links_3 => "[https://mediawiki.org MediaWiki]

[https://mediawiki.org]

[//en.wikipedia.org Wikipedia]

[mailto:info@example.org email me]

[mailto:info@example.org?Subject=URL%20Encoded%20Subject&body=Body%20Text info]

[{{fullurl:{{FULLPAGENAME}}|action=edit}} Edit this page]",
    list_0 => "* a\n* b\n** c\n*** d\nno more list\n* a\n\n",
    list_1 => "* Lists are easy to do:
** start every line
* with a star
** more stars mean
*** deeper levels",
    list_2 => "* A newline
* in a list
marks the end of the list.
Of course
* you can
* start again.",
    list_3 => "* You can also
** break lines<br>inside lists<br>like this",
    list_4 => "; Definition lists
; term : definition
; semicolon plus term
: colon plus definition",
    list_5 => "; Mixed definition lists
; item 1 : definition
:; sub-item 1 plus term
:: two colons plus definition
:; sub-item 2 : colon plus definition
; item 2
: back to the main list",
    list_6 => "* Or create mixed lists
*# and nest them
*#* like this
*#*; definitions
*#*: work:
*#*; apple
*#*; banana
*#*: fruits",
    list_7 => "<ol>
  <li>list item A1
    <ol>
      <li>list item B1</li>
      <li>list item B2</li>
    </ol>continuing list item A1
  </li>
  <li>list item A2</li>
</ol>",
    marker_in_attr => &format!(
        r#"<abbr a="{MARKER_PREFIX}1{MARKER_SUFFIX}"
                 b='{MARKER_PREFIX}2{MARKER_SUFFIX}'
                 c={MARKER_PREFIX}3{MARKER_SUFFIX}>:-(</abbr>
        "<abbr a="a{MARKER_PREFIX}4{MARKER_SUFFIX}b{MARKER_PREFIX}5{MARKER_SUFFIX}c"
               b='a{MARKER_PREFIX}6{MARKER_SUFFIX}b{MARKER_PREFIX}7{MARKER_SUFFIX}c'
               c=a{MARKER_PREFIX}8{MARKER_SUFFIX}b{MARKER_PREFIX}9{MARKER_SUFFIX}c>:-(</abbr>"#
    ),
    new_line_in_table_data => "{|\n|B\nC||d\n|}",
    redirect => "#REDIRECT [[Hello world]]\n\n----\nText content\nMore text content\n\nThird line\n",
    strip_marker => &format!("{MARKER_PREFIX}1{MARKER_SUFFIX}"),
    table_1 => "{| hello\n|good || bye || friend\n |}\n",
    table_2 => r#"{| class="wikitable" style="margin:auto"
|+ Caption text
|-
! Header text !! Header text !! Header text
|-
| Example || Example || Example
|-
| Example || Example || Example
|-
| Example || Example || Example
|}"#,
    table_3 => "{|\n|Orange\n|Apple\n|-\n|Bread\n|Pie\n|-\n|Butter\n|Ice cream\n|}\n",
    table_4 => "{|\n|Orange\n|}\n",
    table_5 => "{|\n! A !! B !! C\n|}\n",
    table_6 => r#"{| class="wikitable"
!colspan="6"|Shopping List
|-
|rowspan="2"|Bread & Butter
|Pie
|Buns
|Danish
|colspan="2"|Croissant
|-
|Cheese
|colspan="2"|Ice cream
|Butter
|Yogurt
|}"#,
    table_7 => "{| class=\"a\"\n!colspan=\"6\"|A\n|-\n|rowspan=\"2\"|B\n|}",
    table_8 => r#"<div class="noresize">
{| class="wikitable"
! colspan="6" |Shopping List
|-
| rowspan="2" |Areallyreallyreallyreallylongstringwillcauseyourtableto
| Pie
| Buns
| Danish
| colspan="2" |Croissantsmaycausetexttoincreasethesizeofyourcolumnsoitbreaksoutofthecontent area if you do not wrap the table with noresize.
|-
| Cheese
| colspan="2" |Ice cream
| Butter
| Yogurt
|}
</div>"#,
    table_caption => r#"{| class="wikitable"
|+ style="caption-side:bottom; color:#e76700;"|''Food complements''
|-
! style="color:green" | Fruits
! style="color:red" | Fats
|-
|Orange
|Butter
|-
|Pear
|Pie
|-
|Apple
|Ice cream
|}"#,
    table_html => r#"{| valign="top"
|-
|<ul><ol start="125"><li>a</li><li>bb</li><li>ccc</li></ol></ul>
|<ul><ol start="128"><li>ddd</li><li>ee</li><li>f</li></ol></ul>
|}"#,
    table_multi_cell_attr => r#"{| class="wikitable"
| Orange || Apple     || style="text-align:right;" | 12,333.00
|-
| Bread || Pie       || style="text-align:right;" | 500.00
|-
| Butter || Ice cream || style="text-align:right;" | 1.00
|}"#,
    table_partial => r#"{| class="wikitable"
| Orange
| Apple
| style="text-align:right;" | 12,333.00
|-"#,
    table_span => r#"{| class="wikitable" style="width: 85%;"
| colspan="2" | This column width is 85% of the screen width
|-
| style="width: 30%"| '''This column is 30% counted from 85% of the screen width'''
| style="width: 70%"| '''This column is 70% counted from 85% of the screen width'''
|}"#,
    tag_extlink => "[<span>[//localhost:3000/Template%3ADate%20and%20time%20templates?action=edit edit]</span>]",
    tpl_with_autolink => "{{a|https://example.com|c=d e}}",
    tpl_with_tag => r#"{{a|<div k="v"></div>}}"#
}
