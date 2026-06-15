#![expect(missing_docs, reason = "these are tests")]

use libwikitext_common::title::{Namespace, Title};
use libwikitext_data::CONFIG;

#[test]
fn join() {
    let base = Title::new(&CONFIG, "Talk:A/b/c", None).unwrap();
    assert_eq!(
        base.join("Absolute"),
        ("Absolute".into(), "Absolute".into())
    );
    assert_eq!(base.join("/d"), ("Talk:A/b/c/d".into(), "/d".into()));
    assert_eq!(base.join("/d#F"), ("Talk:A/b/c/d#F".into(), "/d#F".into()));
    assert_eq!(base.join("/d///"), ("Talk:A/b/c/d".into(), "d".into()));
    assert_eq!(base.join("/d/#F"), ("Talk:A/b/c/d#F".into(), "d#F".into()));
    assert_eq!(base.join("/d/e"), ("Talk:A/b/c/d/e".into(), "/d/e".into()));
    assert_eq!(
        base.join("../z"),
        ("Talk:A/b/z".into(), "Talk:A/b/z".into())
    );
    assert_eq!(base.join("../z/"), ("Talk:A/b/z".into(), "z".into()));
    assert_eq!(base.join("../../y"), ("Talk:A/y".into(), "Talk:A/y".into()));
    assert_eq!(base.join("../../y///"), ("Talk:A/y".into(), "y".into()));
    assert_eq!(
        base.join("../../y///#F"),
        ("Talk:A/y#F".into(), "y#F".into())
    );
    assert_eq!(
        base.join("../../../x"),
        ("../../../x".into(), "../../../x".into())
    );
}

#[test]
fn from_str() {
    let title = Title::new(&CONFIG, "Wikidata:Talk:Aa/Bb/Cc#Dd/Ee/Ff", None).unwrap();
    assert_eq!(title.namespace().id, Namespace::TALK);
    assert_eq!(title.base_text(), "Aa/Bb");
    assert_eq!(title.fragment(), Some("Dd/Ee/Ff"));
    assert_eq!(title.full_text(), "wikidata:Talk:Aa/Bb/Cc#Dd/Ee/Ff");
    assert_eq!(title.interwiki(), Some("wikidata"));
    assert_eq!(title.key(), "Talk:Aa/Bb/Cc");
    assert_eq!(title.prefixed_text(), "wikidata:Talk:Aa/Bb/Cc");
    assert_eq!(title.root_text(), "Aa");
    assert_eq!(title.subpage_text(), "Cc");
    assert_eq!(title.text(), "Aa/Bb/Cc");
}

#[test]
fn interwiki() {
    let title = Title::new(&CONFIG, "Wikidata:File:A.png", None).unwrap();
    assert_eq!(title.interwiki(), Some("wikidata"));
    assert_eq!(title.namespace().id, Namespace::FILE);
    assert_eq!(title.key(), "File:A.png");

    let title = Title::new(&CONFIG, ":Wikidata:File:A.png", None).unwrap();
    assert_eq!(title.interwiki(), Some("wikidata"));
    assert_eq!(title.namespace().id, Namespace::FILE);
    assert_eq!(title.key(), "File:A.png");

    let title = Title::new(&CONFIG, ":File:A.png", None).unwrap();
    assert_eq!(title.interwiki(), None);
    assert_eq!(title.namespace().id, Namespace::FILE);
    assert_eq!(title.key(), "File:A.png");

    let title = Title::new(&CONFIG, "File:A.png", None).unwrap();
    assert_eq!(title.interwiki(), None);
    assert_eq!(title.namespace().id, Namespace::FILE);
    assert_eq!(title.key(), "File:A.png");

    let title = Title::new(&CONFIG, ":Wikipedia:Wikipedia:Foo", None).unwrap();
    assert_eq!(title.interwiki(), None);
    assert_eq!(title.namespace().id, Namespace::PROJECT);
    assert_eq!(title.key(), "Wikipedia:Wikipedia:Foo");
    assert_eq!(title.text(), "Wikipedia:Foo");

    let title = Title::new(&CONFIG, "Wikipedia:Wikipedia:Foo", None).unwrap();
    assert_eq!(title.interwiki(), None);
    assert_eq!(title.namespace().id, Namespace::PROJECT);
    assert_eq!(title.key(), "Wikipedia:Wikipedia:Foo");
    assert_eq!(title.text(), "Wikipedia:Foo");

    let title = Title::new(&CONFIG, "Wikipedia:Foo", None).unwrap();
    assert_eq!(title.interwiki(), None);
    assert_eq!(title.namespace().id, Namespace::PROJECT);
    assert_eq!(title.key(), "Wikipedia:Foo");
    assert_eq!(title.text(), "Foo");

    let title = Title::new(&CONFIG, ":Wikipedia:Foo", None).unwrap();
    assert_eq!(title.interwiki(), None);
    assert_eq!(title.namespace().id, Namespace::PROJECT);
    assert_eq!(title.key(), "Wikipedia:Foo");
    assert_eq!(title.text(), "Foo");
}
