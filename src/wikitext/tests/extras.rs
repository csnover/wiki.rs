use super::*;

#[test]
fn new_line_in_table_data() {
    run_test("{|\n|B\nC||d\n|}");
    panic!();
}

#[test]
fn include() {
    run_test(
        "<noinclude>hello</noinclude><onlyinclude>hello</onlyinclude><includeonly>did i just typo before?</includeonly>",
    );
    panic!();
}

#[test]
fn tag_extlink() {
    run_test(
        "[<span>[//localhost:3000/Template%3ADate%20and%20time%20templates?action=edit edit]</span>]",
    );
    panic!();
}

#[test]
fn extension_tags() {
    run_test("<ref/><ref>a</ref>");
    panic!();
}

#[test]
fn balance() {
    run_test("'''b'''bi'' i\nn '''ii'' n\nn '''ii'' n");
    panic!();
}

#[test]
fn balance_2() {
    run_test(" '''b''' n'''i''n");
    panic!();
}

#[test]
fn balance_3() {
    run_test("nn'''ib''' b'''i''n");
    panic!();
}

#[test]
fn link_ampersand() {
    run_test("[[Hello & world]]");
    panic!();
}

#[test]
fn pathological() {
    run_test(&"{".repeat(30));
    run_test(&"!".repeat(30));
    run_test(&"[".repeat(30));
    run_test(&"-{".repeat(30));
    run_test(&"{|".repeat(30));
    run_test(&"<ref>".repeat(30));
    panic!();
}

#[test]
fn include_2() {
    run_test("<includeonly>{{a}}</includeonly><noinclude>{{b}}</noinclude>");
    panic!();
}

#[test]
fn link_with_kv_in_args() {
    run_test("{{a|[[b|alt=]]}}");
    panic!();
}

#[test]
fn arg_whitespace() {
    run_test("{{a | b = http://www.example.com/ | c = d}}");
    panic!();
}

#[test]
fn link_trail() {
    run_test("[[Yes]]yes [[No]]!!");
    panic!();
}

#[test]
fn link_args() {
    run_test("[[Link|a|b=c|d=e=f]]mazing");
    panic!();
}

#[test]
fn link_stupid_args() {
    run_test("[[Link|link =lol]]");
    panic!();
}

#[test]
fn tpl_with_autolink() {
    run_test("{{a|https://example.com|c=d e}}");

    panic!();
}

#[test]
fn strip_marker() {
    run_test(&format!("{MARKER_PREFIX}1{MARKER_SUFFIX}"));
    panic!();
}

#[test]
fn tpl_with_tag() {
    run_test(r#"{{a|<div k="v"></div>}}"#);
    panic!()
}

#[test]
fn hello_world() {
    run_test("#REDIRECT [[Hello world]]\n\n----\nText content\nMore text content\n\nThird line\n");
}

#[test]
fn list_0() {
    run_test("* a\n* b\n** c\n*** d\nno more list\n* a\n\n");
    panic!();
}

#[test]
fn list_1() {
    run_test(
        "* Lists are easy to do:
** start every line
* with a star
** more stars mean
*** deeper levels",
    );
}

#[test]
fn list_2() {
    run_test(
        "* A newline
* in a list
marks the end of the list.
Of course
* you can
* start again.",
    );
}

#[test]
fn list_3() {
    run_test(
        "* You can also
** break lines<br>inside lists<br>like this",
    );
}

#[test]
fn list_4() {
    run_test(
        "; Definition lists
; term : definition
; semicolon plus term
: colon plus definition",
    );
}

#[test]
fn list_5() {
    run_test(
        "; Mixed definition lists
; item 1 : definition
:; sub-item 1 plus term
:: two colons plus definition
:; sub-item 2 : colon plus definition
; item 2
: back to the main list",
    );
}

#[test]
fn list_6() {
    run_test(
        "* Or create mixed lists
*# and nest them
*#* like this
*#*; definitions
*#*: work:
*#*; apple
*#*; banana
*#*: fruits",
    );
}

#[test]
fn list_7() {
    run_test(
        "<ol>
  <li>list item A1
    <ol>
      <li>list item B1</li>
      <li>list item B2</li>
    </ol>continuing list item A1
  </li>
  <li>list item A2</li>
</ol>",
    );
}

#[test]
fn ext_broken() {
    run_test("<ref><!-- oops</ref>");
}

#[test]
fn table() {
    run_test("{| hello\n|good || bye || friend\n |}\n");
}

#[test]
fn table_2() {
    run_test(
        r#"{| class="wikitable" style="margin:auto"
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
    );
}

#[test]
fn table_3() {
    run_test("{|\n|Orange\n|Apple\n|-\n|Bread\n|Pie\n|-\n|Butter\n|Ice cream\n|}\n");
}

#[test]
fn table_4() {
    run_test("{|\n|Orange\n|}\n");
}

#[test]
fn table_5() {
    run_test("{|\n! A !! B !! C\n|}\n");
}
#[test]

fn table_6() {
    run_test(
        r#"{| class="wikitable"
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
    );
}

#[test]
fn table_7() {
    run_test("{| class=\"a\"\n!colspan=\"6\"|A\n|-\n|rowspan=\"2\"|B\n|}");
    panic!();
}

#[test]
fn table_8() {
    run_test(
        r#"<div class="noresize">
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
    );
}

#[test]
fn table_partial() {
    run_test(
        r#"{| class="wikitable"
| Orange
| Apple
| style="text-align:right;" | 12,333.00
|-"#,
    );
}

#[test]
fn table_multi_cell_attr() {
    run_test(
        r#"{| class="wikitable"
| Orange || Apple     || style="text-align:right;" | 12,333.00
|-
| Bread || Pie       || style="text-align:right;" | 500.00
|-
| Butter || Ice cream || style="text-align:right;" | 1.00
|}"#,
    );
}

#[test]
fn table_caption() {
    run_test(
        r#"{| class="wikitable"
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
    );
}

#[test]
fn table_span() {
    run_test(
        r#"{| class="wikitable" style="width: 85%;"
| colspan="2" | This column width is 85% of the screen width
|-
| style="width: 30%"| '''This column is 30% counted from 85% of the screen width'''
| style="width: 70%"| '''This column is 70% counted from 85% of the screen width'''
|}"#,
    );
}

#[test]
fn table_html() {
    run_test(
        r#"{| valign="top"
|-
|<ul><ol start="125"><li>a</li><li>bb</li><li>ccc</li></ol></ul>
|<ul><ol start="128"><li>ddd</li><li>ee</li><li>f</li></ol></ul>
|}"#,
    );
}

#[test]
fn links_1() {
    run_test(
        "[[Main Page]]

[[Help:Contents]]

[[Extension:DynamicPageList (Wikimedia)]]

[[Help:Editing pages#Preview|previewing]]

[[#See also|different text]]",
    );
}

#[test]
fn links_2() {
    run_test(
        "[[Help]]s

[[Help]]<nowiki />ful advice",
    );
}

#[test]
fn links_3() {
    run_test(
        "[https://mediawiki.org MediaWiki]

[https://mediawiki.org]

[//en.wikipedia.org Wikipedia]

[mailto:info@example.org email me]

[mailto:info@example.org?Subject=URL%20Encoded%20Subject&body=Body%20Text info]

[{{fullurl:{{FULLPAGENAME}}|action=edit}} Edit this page]",
    );
}

#[test]
fn autolink_1() {
    run_test(
        "https://mediawiki.org\nhttps://mediawiki.org\npre http://example.com post\n<nowiki>https://mediawiki.org</nowiki>",
    );
}

#[test]
fn autolink_2() {
    run_test("<code>http://</code> or <code>https://</code> or http:// or https://");
    panic!();
}

#[test]
fn autolink_3() {
    run_test("https://mediawiki.org.\n(https://mediawiki.org)\nhttps://mediawiki.org/a(b).\n");
}

// TODO: Need to override config
#[test]
fn autolink_4() {
    run_test(
        "ISBN 0-7475-3269-9 or ISBN 000000000x or ISBN 0-9&nbsp;9999-2222 or ISBN 978-00000-00000 or (invalid:) ISBN 938-00000-00000\nPMID 1923.23232\nRFC 42",
    );
}

#[test]
fn heading_1() {
    run_test(r#"==<span id="Alternate Section Title"></span>Section heading=="#);
}

#[test]
fn image_1() {
    run_test("[[File:filename.extension|alt=a|caption]]");
}
