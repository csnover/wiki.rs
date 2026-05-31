//! Wikitext rendering test cases from MediaWiki.

#[cfg(test)]
mod config;
#[cfg(test)]
mod test;
#[cfg(test)]
mod test_parser;

#[test]
fn waaaaaa() {
    // let source = "{{well this is\n====\nkinda crazy|do|you|think=not|}}\n<!-- a -->      <!-- b -->    \n!\n<!-- c -->      <!-- d -->    \n    <!-- e -->    \n    <!-- f -->    \n<!-- g -->      <!-- h -->    ";

    //     let source = "{{[[Foo|bar}}]]

    // But close-brace is not a valid character in a link title:
    // {{[[Foo}}|bar]]

    // However, we can still tell this was handled as a link in the preprocessor:
    // [[Foo}}|bar]]";

    let source = "*#*#;*;;foo : bar
*#*#;boo : baz";

    let parser = libwikitext_parse::Parser::new(&config::CONFIG);
    // let root = parser.preprocess(source, false).unwrap();
    let root = parser.parse(source).unwrap();
    eprintln!(
        "{:#?}",
        libwikitext_parse::inspect(&libwikitext_parse::FileMap::new(source), &root.root)
    );
}
