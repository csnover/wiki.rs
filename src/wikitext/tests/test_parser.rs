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

use crate::wikitext::codemap::Spanned;
use serde_json::{Map, Value};
use std::{borrow::Cow, collections::HashMap};

pub(super) type Error = peg::error::ParseError<peg::str::LineCol>;

pub(super) struct Testfile<'input> {
    pub(super) chunks: Vec<Spanned<Chunk<'input>>>,
}

impl<'a> Testfile<'a> {
    pub(super) fn parse(code: &'a str) -> Result<Self, Error> {
        testfile::parse(code)
    }
}

pub(super) enum Chunk<'input> {
    Comment,
    Line,
    Article {
        title: &'input str,
        _text: &'input str,
    },
    FunctionHooks,
    Test {
        name: &'input str,
        sections: HashMap<&'input str, SectionText<'input>>,
    },
    Hooks,
}

pub(super) struct Section<'input> {
    pub name: &'input str,
    pub text: SectionText<'input>,
}

pub(super) enum SectionText<'input> {
    Text(Cow<'input, str>),
    Kv(HashMap<Cow<'input, str>, Value>),
}

peg::parser! {grammar testfile() for str {
  pub rule parse() -> Testfile<'input>
  = comment_or_blank_line()*
    _version:format()?
    comment_or_blank_line()*
    _options:(sec:option_section() end(<>) { sec })?
    chunks:spanned(<chunk()>)+
  { Testfile { chunks } }

  rule format() -> u8
  = "!!" ws()? i("version") ws()+ v:$(['0'..='9']+) rest_of_line()
  { v.parse().unwrap() }

  rule option_section() -> Section<'input>
  = start(<"options">) opts:option_list()?
  { Section { name: "options", text: SectionText::Kv(opts.unwrap_or_default()) } }

  rule option_list() -> HashMap<Cow<'input, str>, Value>
  = o:(t:an_option() (([' '|'\t'] / eol())+) { t })+
  { o.into_iter().collect() }

  rule chunk() -> Chunk<'input>
  = comment_or_blank_line()
  / article()
  / test()
  / hooks()
  / functionhooks()
    // Final fallback production is a catch-all, since some ancient
    // parserTest files have garbage text between tests and in the old
    // hand-coded parser test parser this was just ignored as a comment.
  / _l:line()
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
  { Chunk::Article { title, _text: text } }

  rule test() -> Chunk<'input>
  = start(<"test">)
    name:text()
    sections:(section() / config_section() / option_section())*
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
        if parsoid == "" {
            *parsoid = Value::Object(<_>::default());
        } else if let Value::String(s) = parsoid {
            let mut map = Map::with_capacity(1);
            map.insert("modes".to_string(), core::mem::take(s).into());
            *parsoid = Value::Object(map);
        } else if let Value::Array(v) = parsoid && let [s @ Value::String(_)] = v.as_mut_slice() {
            let mut map = Map::with_capacity(1);
            map.insert("modes".to_string(), core::mem::take(s));
            *parsoid = Value::Object(map);
        }
    }

    Chunk::Test { name, sections }
  }

  rule config_section() -> Section<'input>
  = start(<"config">) items:config_list()?
  { Section { name: "config", text: SectionText::Kv(items.unwrap_or_default()) } }

  rule config_list() -> HashMap<Cow<'input, str>, Value>
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

  /////////////
  // Section //
  /////////////

  rule section() -> Section<'input>
  = "!!" ws()?
    (!"test") (!"end") (!"options") (!"config")
    name:$([^' '|'\t'|'\r'|'\n']+)
    rest_of_line()
    text:text()
  { Section { name, text: SectionText::Text(text.into()) } }

  rule a_config_line() -> (Cow<'input, str>, Value)
  = k:option_name() v:config_value()
  { (Cow::Borrowed(k), v) }

  rule config_value() -> Value
  = ws()? "=" ws()? t:valid_json_value() { t }

  // from PHP parser in tests/parser/parserTest.inc:parseOptions()
  //   foo
  //   foo=bar
  //   foo="bar baz"
  //   foo=[[bar baz]]
  //   foo={...json...}
  //   foo=bar,"baz quux",[[bat]]
  rule an_option() -> (Cow<'input, str>, Value)
  = k:option_name() v:option_value()?
  { (k.to_ascii_lowercase().into(), v.unwrap_or_default()) }

  rule option_name() -> &'input str
  = $([^' '|'\t'|'\n'|'='|'!']+)

  rule option_value() -> Value
  = ws()? "=" ws()? ovl:option_value_list()
  { ovl }

  rule option_value_list() -> Value
  = v:an_option_value() ++ (ws()? "," ws()?)
  { if v.len() == 1 { v.into_iter().next().unwrap() } else { Value::Array(v) } }

  rule an_option_value() -> Value
  = v:(link_target_value()
    / t:quoted_value() { Cow::Borrowed(t) }
    / t:plain_value() { Cow::Borrowed(t) }
    / t:json_value() { Cow::Borrowed(t) }
  )
  {
    if v.starts_with('"') || v.starts_with('{') {
      serde_json::from_str(&v).unwrap()
    } else {
      Value::String(v.into_owned())
    }
  }

  rule link_target_value() -> Cow<'input, str>
  = "[[" v:$([^']'|'\n']*) "]]"
  { serde_json::to_string(v).unwrap().into() }

  rule valid_json_value() -> Value
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

  rule spanned<T>(r: rule<T>) -> Spanned<T>
  = start:position!() node:r() end:position!()
  { Spanned::new(node, start, end) }

  rule i(lit: &'static str)
  = quiet!{
    input:$([_]*<{lit.chars().count()}>)
    {? if input.eq_ignore_ascii_case(lit) { Ok(()) } else { Err(lit) } }
  } / expected!(lit)

  rule start<T>(r: rule<T>)
  = "!!" ws()? r() ws()? eol()

  rule end<T>(r: rule<T>)
  = "!!" ws()? ("end" r()?) ws()? eolf()

  rule eol() -> &'input str
  = $("\n")

  rule eolf() -> &'input str
  = $("\n" / ![_] "")

  rule ws()
  = [' '|'\t']+

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
