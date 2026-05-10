//! A parser for the MediaWiki Wikitext test file format.

// Yo dawg, we put a parser in your parser tests so you can parse while you test
// parsers.
//
// This code is heavily adapted from the Parsoid grammar at
// <https://github.com/wikimedia/mediawiki-services-parsoid>
// based on
// Git-Commit-ID: 9cc7fc706b727c392b53fe7fe571747901424065
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyright: 2011-07-20 Brion Vibber <brion@pobox.com>

use serde_json_borrow::{Map, Value};
use std::{borrow::Cow, collections::HashMap};

/// The error type for the test parser.
pub(super) type Error = peg::error::ParseError<peg::str::LineCol>;

/// A collection of named test sections.
pub(super) type Sections<'input> = HashMap<&'input str, SectionText<'input>>;

/// A test file.
pub(super) struct Testfile<'input> {
    /// The chunks of a test file.
    pub(super) chunks: Vec<Chunk<'input>>,
}

impl<'a> Testfile<'a> {
    /// Parses a test file.
    pub(super) fn parse(code: &'a str) -> Result<Self, Error> {
        testfile::parse(code)
    }
}

/// A test file chunk.
pub(super) enum Chunk<'input> {
    /// An article chunk.
    Article {
        /// The title of the article.
        title: &'input str,
        /// The body of the article.
        text: &'input str,
    },
    /// A comment chunk.
    Comment,
    /// A function hook chunk.
    FunctionHooks,
    /// A hooks chunk.
    Hooks,
    /// An ignored line.
    Line,
    /// A test chunk.
    Test {
        /// The name of the test.
        name: &'input str,
        /// The byte position of the test in the test file.
        pos: usize,
        /// The subsections of the test.
        sections: Sections<'input>,
    },
}

/// A semi-structured result metadata chunk for a test.
#[derive(Debug)]
pub(super) struct Metadata<'input> {
    /// The expected flags.
    pub flags: Option<&'input str>,
    /// The expected page display title.
    pub title: Option<&'input str>,
    /// The expected table of contents outline.
    pub toc: Option<Vec<Toc<'input>>>,
}

/// A partially processed metadata part.
enum MetadataPart<'input> {
    /// A flags part.
    Flags(&'input str),
    /// A title part.
    Title(&'input str),
    /// An outline part.
    Toc(Vec<Toc<'input>>),
}

/// A test section.
#[derive(Debug)]
pub(super) struct Section<'input> {
    /// The name of the section.
    pub name: &'input str,
    /// The contents of the section.
    pub text: SectionText<'input>,
}

/// Test section content.
#[derive(Debug)]
pub(super) enum SectionText<'input> {
    /// A key-value section.
    Kv(HashMap<Cow<'input, str>, Value<'input>>),
    /// A metadata section.
    Meta(Metadata<'input>),
    /// A plain text section.
    Text(Cow<'input, str>),
}

impl SectionText<'_> {
    /// Returns true if a value with the given `key` exists in a key-value
    /// section.
    pub fn contains(&self, key: &str) -> bool {
        match self {
            SectionText::Kv(kv) => kv.get(key).is_some(),
            SectionText::Meta(_) | SectionText::Text(_) => false,
        }
    }

    /// Gets a string value with the given `key` from a key-value section.
    pub fn get(&self, key: &str) -> Option<&str> {
        match self {
            SectionText::Kv(kv) => kv.get(key).and_then(Value::as_str),
            SectionText::Meta(_) | SectionText::Text(_) => None,
        }
    }

    /// Gets the metadata of a metadata section.
    pub fn meta(&self) -> Option<&Metadata<'_>> {
        match self {
            SectionText::Kv(_) | SectionText::Text(_) => None,
            SectionText::Meta(metadata) => Some(metadata),
        }
    }

    /// Gets the text of a text section.
    pub fn text(&self) -> Option<&str> {
        match self {
            SectionText::Kv(_) | SectionText::Meta(_) => None,
            SectionText::Text(text) => Some(text),
        }
    }
}

/// A table of contents entry.
#[derive(Debug)]
pub(super) struct Toc<'input> {
    // pub anchor: &'input str,
    // pub index: &'input str,
    // pub level: u32,
    /// The expected HTML contents of the entry.
    pub line: &'input str,
    // pub number: &'input str,
    // pub offset: Option<u32>,
    /// The expected HTML heading tag.
    pub tag: &'input str,
    // pub title: Option<&'input str>,
}

peg::parser! {grammar testfile() for str {
  pub rule parse() -> Testfile<'input>
  = comment_or_blank_line()*
    _version:format()?
    comment_or_blank_line()*
    _options:(sec:option_section() end(<>) { sec })?
    chunks:chunk()+
  { Testfile { chunks } }

  rule format() -> u8
  = "!!" ws()? i("version") ws()+ v:$(['0'..='9']+) rest_of_line()
  { v.parse().unwrap() }

  rule chunk() -> Chunk<'input>
  = comment_or_blank_line()
  / article()
  / test()
  / hooks()
  / functionhooks()
  // Final fallback production is a catch-all, since some ancient
  // parserTest files have garbage text between tests and in the old
  // hand-coded parser test parser this was just ignored as a comment.
  / line()
  { Chunk::Line }

  rule comment_or_blank_line() -> Chunk<'input>
  = comment()
  / ws()? _nl:eol()
  { Chunk::Line }

  rule comment() -> Chunk<'input>
  = "#"
    _text:rest_of_line()
  { Chunk::Comment }

  rule article() -> Chunk<'input>
  = start(<"article">)
    title:line()
    start(<"text">)
    text:text()
    end(<"article">)
  { Chunk::Article { title, text } }

  rule config_section() -> Section<'input>
  = start(<"config">) items:config_list()?
  { Section { name: "config", text: SectionText::Kv(items.unwrap_or_default()) } }

  rule config_list() -> HashMap<Cow<'input, str>, Value<'input>>
  = c:(t:a_config_line() eol()+ { t })+
  { c.into_iter().collect() }

  rule hooks() -> Chunk<'input>
  = start(<"hooks" ":"?>)
    _text:text()
    end(<"hooks">)
  { Chunk::Hooks }

  rule functionhooks() -> Chunk<'input>
  = start(<"functionhooks" ":"?>)
    _text:text()
    end(<"functionhooks" ":"?>)
  { Chunk::FunctionHooks }

  rule test() -> Chunk<'input>
  = pos:position!()
    start(<"test">)
    name:text()
    sections:(config_section() / option_section() / metadata_section() / section())*
    end(<>)
  {
    let mut sections = sections.into_iter().map(|section| {
        (section.name, section.text)
    }).collect::<HashMap<_, _>>();

    // pegjs parser handles item options as follows:
    //   item option             value of item.options.parsoid
    //    <none>                          undefined
    //    parsoid                             ""
    //    parsoid=wt2html                  "wt2html"
    //    parsoid=wt2html,wt2wt        ["wt2html","wt2wt"]
    //    parsoid={"modes":["wt2wt"]}    {modes:['wt2wt']}

    if let Some(SectionText::Kv(options)) = sections.get_mut("options") &&
        let Some(parsoid) = options.get_mut("parsoid")
    {
        if parsoid.as_str() == Some("") {
            *parsoid = Value::Object(<_>::default());
        } else if let Value::Str(s) = parsoid {
            let map = Map::from([("modes", core::mem::take(s))]);
            *parsoid = Value::Object(map);
        } else if let Value::Array(v) = parsoid && let [s @ Value::Str(_)] = v.as_mut_slice() {
            let map = Map::from([("modes", core::mem::take(s))]);
            *parsoid = Value::Object(map);
        }
    }

    Chunk::Test { name: name.trim_ascii_end(), pos, sections }
  }

  rule section() -> Section<'input>
  = "!!" ws()?
    // Avoid silently matching any of the sections that are supposed to be
    // structured
    !("test" / "end" / "options" / "config" / "metadata")
    name:$([^' '|'\t'|'\r'|'\n']+)
    rest_of_line()
    text:text()
  { Section { name, text: SectionText::Text(text.into()) } }

  rule option_section() -> Section<'input>
  = start(<"options">) opts:option_list()?
  { Section { name: "options", text: SectionText::Kv(opts.unwrap_or_default()) } }

  rule option_list() -> HashMap<Cow<'input, str>, Value<'input>>
  = o:(t:an_option() (([' '|'\t'] / eol())+) { t })+
  { o.into_iter().collect() }

  rule metadata_section() -> Section<'input>
  = name:start(<$("metadata" ("/" [^'\n']+)?)>) meta:metadata()
  { Section { name, text: SectionText::Meta(meta) } }

  // Options that affect the generated metadata are:
  // showflags
  // showtitle
  // showtocdata
  // Probably the order that they appear in the options also defines the order
  // they appear in the output, but this is just barely regular enough that it
  // can be parsed without knowing the order or which options were specified
  pub rule metadata() -> Metadata<'input>
  = parts:(!"!!" part:metadata_part() { part })+
  {
      let mut flags = None;
      let mut title = None;
      let mut toc = None;
      for part in parts {
          match part {
              MetadataPart::Flags(f) => flags = Some(f),
              MetadataPart::Title(t) => title = Some(t),
              MetadataPart::Toc(t) => toc = Some(t),
          }
      }
      Metadata { flags, title, toc }
  }

  rule metadata_part() -> MetadataPart<'input>
  = title:metadata_title() { MetadataPart::Title(title) }
  / flags:metadata_flags() { MetadataPart::Flags(flags) }
  / toc:metadata_toc() { MetadataPart::Toc(toc) }

  rule metadata_flags() -> &'input str
  = "flags=" flags:rest_of_line()
  { flags }

  rule metadata_toc() -> Vec<Toc<'input>>
  = "Sections:" eol() toc:metadata_toc_line()* !" "
  { toc }

  // h2 index:1 toclevel:1 number:1 title:Parser_test off:0 anchor/linkAnchor:a line:a
  rule metadata_toc_line() -> Toc<'input>
  = " "+
    tag:$("h" ['1'..='6'])
    " index:" _index:$([^' ']*)
    " toclevel:" _level:number()
    " number:" _number:$(['0'..='9'|'.']+)
    " title:" _title:("NULL" { None } / s:$([^' ']*) { Some(s) })
    " off:" _offset:("NULL" { None } / n:number() { Some(n) })
    " anchor/linkAnchor:" _anchor:$([^' ']*)
    " line:" line:rest_of_line()
  { Toc { line, tag } }

  rule metadata_title() -> &'input str
  = !("flags=" / "Sections:" / "!!")
    title:rest_of_line()
  { title }

  rule a_config_line() -> (Cow<'input, str>, Value<'input>)
  = k:option_name() v:config_value()
  { (Cow::Borrowed(k), v) }

  rule config_value() -> Value<'input>
  = ws()? "=" ws()? t:valid_json_value() { t }

  // from PHP parser in tests/parser/parserTest.inc:parseOptions()
  //   foo
  //   foo=bar
  //   foo="bar baz"
  //   foo=[[bar baz]]
  //   foo={...json...}
  //   foo=bar,"baz quux",[[bat]]
  rule an_option() -> (Cow<'input, str>, Value<'input>)
  = k:option_name() v:option_value()?
  { (k.to_ascii_lowercase().into(), v.unwrap_or_default()) }

  rule option_name() -> &'input str
  = $([^' '|'\t'|'\n'|'='|'!']+)

  rule option_value() -> Value<'input>
  = ws()? "=" ws()? ovl:option_value_list()
  { ovl }

  rule option_value_list() -> Value<'input>
  = v:an_option_value() ++ (ws()? "," ws()?)
  { if v.len() == 1 { v.into_iter().next().unwrap() } else { Value::Array(v) } }

  rule an_option_value() -> Value<'input>
  = v:link_target_value()
  { Value::Str(v) }
  / v:(quoted_value() / plain_value() / json_value())
  {
    if v.starts_with('"') || v.starts_with('{') {
      serde_json::from_str(v).unwrap()
    } else {
      Value::Str(Cow::Borrowed(v))
    }
  }

  rule link_target_value() -> Cow<'input, str>
  = "[[" v:$([^']'|'\n']*) "]]"
  { serde_json::to_string(v).unwrap().into() }

  rule valid_json_value() -> Value<'input>
  = v:$(quoted_value() / plain_value() / array_value() / json_value())
  {? serde_json::from_str(v).map_err(|_| "invalid json") }

  rule quoted_value() -> &'input str
  = $("\"" ([^'\\'|'"'|'\n'] / "\\" [^'\n'])* "\"")

  rule plain_value() -> &'input str
  = $([^' '|'\t'|'\n'|'"'|'\''|'['|']'|'='|','|'!'|'{']+)

  rule array_value() -> &'input str
  = $("[" ([^'"'|'['|']'|'\n'] / quoted_value() / array_value() / eol())* "]")

  rule json_value() -> &'input str
  = $("{" ([^'"'|'{'|'}'|'\n'] / quoted_value() / json_value() / eol())* "}")

  rule i(lit: &'static str)
  = quiet!{
    input:$([_]*<{lit.chars().count()}>)
    {? if input.eq_ignore_ascii_case(lit) { Ok(()) } else { Err(lit) } }
  } / expected!(lit)

  rule start<T>(r: rule<T>) -> T
  = "!!" ws()? name:r() ws()? eol()
  { name }

  rule end<T>(r: rule<T>)
  = "!!" ws()? ("end" r()?) ws()? eolf()

  rule eol() -> &'input str
  = $("\n")

  rule eolf() -> &'input str
  = $("\n" / ![_] "")

  rule ws()
  = [' '|'\t']+

  rule number() -> u32
  = n:$(['0'..='9']+)
  { n.parse().unwrap() }

  rule rest_of_line() -> &'input str
  = t:$([^'\n']*)
    eol()
  { t }

  rule line() -> &'input str
  = (!"!!")
    t:rest_of_line()
  { t }

  rule text() -> &'input str
  = $(line()*)
}}
