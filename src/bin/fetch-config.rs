//! Fetches a MediaWiki configuration from a remote server and emits it to
//! stdout as Rust source code.

#![warn(clippy::pedantic, missing_docs, rust_2018_idioms)]

use proc_macro2::TokenStream;
use quote::quote;
use std::{borrow::Cow, collections::HashMap};

type MagicWords<'a> = HashMap<Cow<'a, str>, Vec<Cow<'a, str>>>;

/// Beware! Running this function may cause sadness and bleeding from orifices.
fn main() -> Result<(), DisplayError> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let mut args = pico_args::Arguments::from_env();
    let prefix = args.free_from_str::<String>().map_err(
        |_| "missing required url argument\n\nUsage: fetch-config https://wiki.example.com",
    )?;
    let body = fetch(&prefix)?;
    let response = serde_json::from_str::<api::Response<'_>>(&body)?;

    if !response.batch_complete {
        Err("batchcomplete was false")?;
    }

    let query = response.query;

    let magic_words = query
        .magic_words
        .into_iter()
        .map(|word| (word.name, word.aliases))
        .collect::<HashMap<_, _>>();

    // At some point, MW had no restriction at all on what these magic words
    // looked like, so some wikis actually used double fullwidth low lines
    // instead of underscores. As such, the entire alias including the
    // underscores needs to be kept, even though this is a waste of time and
    // memory in 99% of cases.
    let double_underscores = aliases_iter(&magic_words, query.double_underscores, &|alias| alias);

    let extension_tags = query.extension_tags.into_iter().map(|tag| {
        let tag = tag[1..tag.len() - 1].to_ascii_lowercase();
        quote!(#tag)
    });

    let function_hooks = aliases_iter(&magic_words, query.function_hooks, &trim_parser_fn);

    let api::General {
        lang_conversion,
        legal_title_chars,
        link_trail,
        magic_links,
    } = query.general;

    let api::MagicLinks { isbn, pmid, rfc } = magic_links;

    let interwiki_map = query.interwiki_map.into_iter().map(|v| {
        let k = v.prefix.to_ascii_lowercase();
        let v = &v.url;
        quote!(#k => #v)
    });

    let namespaces = namespaces_iter(query.namespaces, &query.namespace_aliases);

    let protocols = query.protocols.into_iter().map(|v| {
        let v = v.to_ascii_lowercase();
        quote!(#v)
    });

    let redirects = redirects_iter(&magic_words);

    let variables = aliases_iter(&magic_words, query.variables, &trim_variable);

    let file: syn::File = syn::parse_quote! {
        static CONFIG_SOURCE: ConfigurationSource = ConfigurationSource {
            annotation_tags: phf::phf_set! {},
            annotations_enabled: false,
            behavior_switch_words: phf::phf_map! {
                #(#double_underscores),*
            },
            extension_tags: phf::phf_set! {
                #(#extension_tags),*
            },
            function_hooks: phf::phf_map! {
                #(#function_hooks),*
            },
            interwiki_map: phf::phf_map! {
                #(#interwiki_map),*
            },
            language_conversion_enabled: #lang_conversion,
            link_trail: #link_trail,
            magic_links: MagicLinks {
                isbn: #isbn,
                pmid: #pmid,
                rfc: #rfc,
            },
            namespaces: &[ #(#namespaces),* ],
            protocols: phf::phf_set! {
                #(#protocols),*
            },
            redirect_magic_words: phf::phf_set! {
                #(#redirects),*
            },
            valid_title_bytes: #legal_title_chars,
            variables: phf::phf_map! {
                #(#variables),*
            }
        };
    };

    println!("{}", prettyplease::unparse(&file));

    Ok(())
}

/// Converts a list of registered keywords into a map of aliases to those
/// keywords.
fn aliases_iter<'a, I, F>(
    magic_words: &'a MagicWords<'_>,
    items: I,
    transform: &F,
) -> impl Iterator<Item = TokenStream>
where
    F: for<'b> Fn(&'b Cow<'b, str>) -> &'b str,
    I: IntoIterator<Item = Cow<'a, str>>,
{
    items.into_iter().flat_map(move |key| {
        let aliases = magic_words.get(&key).map(Vec::as_slice).unwrap_or_default();
        let key = key.to_lowercase();
        aliases.iter().map(move |alias| {
            // TODO: Technically, some magic words are case-sensitive and other
            // ones are not. So far, simplifying the implementation by case
            // folding and treating them all as case-insensitive has not broken
            // anything catastrophically.
            let alias = transform(alias).to_lowercase();
            quote!(#alias => #key)
        })
    })
}

/// Fetches site metadata from a MediaWiki instance.
fn fetch(prefix: &str) -> Result<String, ureq::Error> {
    // 'restrictions' might also be interesting
    // 'languagevariants' will be relevant when that is implemented
    const PROPS: &str = concat!(
        "doubleunderscores",
        "|extensiontags",
        "|functionhooks",
        "|general",
        "|interwikimap",
        "|magicwords",
        "|namespaces",
        "|namespacealiases",
        "|protocols",
        "|variables",
    );

    let result = ureq::get(format!("{prefix}/w/api.php"))
        .query("action", "query")
        .query("meta", "siteinfo")
        .query("format", "json")
        .query("formatversion", "2")
        .query("errorformat", "plaintext")
        .query("siprop", PROPS)
        .header(
            "User-Agent",
            format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        )
        .call()?;
    result.into_body().read_to_string()
}

/// Converts a raw list of namespaces and namespace aliases into a structured
/// list of namespaces.
fn namespaces_iter(
    namespaces: api::Namespaces<'_>,
    namespace_aliases: &api::NamespaceAliases<'_>,
) -> impl Iterator<Item = TokenStream> {
    namespaces.into_values().map(
        |api::Namespace {
             id,
             name,
             canonical,
             case,
             content,
             default_content_model,
             subpages,
         }| {
            let aliases = namespace_aliases
                .iter()
                .filter_map(|a| (a.id == id).then_some(&a.alias));
            let case = match case {
                api::NamespaceCase::CaseSensitive => quote!(CaseSensitive),
                api::NamespaceCase::FirstLetter => quote!(FirstLetter),
            };
            let canonical = canonical.map_or_else(|| quote!(None), |v| quote!(Some(#v)));
            let default_content_model =
                default_content_model.map_or_else(|| quote!(None), |v| quote!(Some(#v)));
            quote! {
                Namespace {
                    id: #id,
                    name: #name,
                    canonical: #canonical,
                    case: #case,
                    content: #content,
                    default_content_model: #default_content_model,
                    subpages: #subpages,
                    aliases: &[#(#aliases),*],
                }
            }
        },
    )
}

/// Creates a list of redirect keywords.
fn redirects_iter(magic_words: &MagicWords<'_>) -> impl Iterator<Item = TokenStream> {
    magic_words
        .get("redirect")
        .map_or(<_>::default(), Vec::as_slice)
        .iter()
        .map(|alias| {
            let v = alias.to_lowercase();
            quote!(#v)
        })
}

/// Trims bad characters from a variable or parser function alias.
fn trim_parser_fn<'a>(alias: &'a Cow<'a, str>) -> &'a str {
    // In MW, `Parser::setFunctionHook` does this conditional stripping of the
    // trailing colon.
    let alias = alias.strip_suffix(':').unwrap_or(alias);

    // In MW, a hash is *added* in `Parser::setFunctionHook` for functions which
    // were registered without the `SFH_NO_HASH` flag. This registration detail
    // is not actually communicated by the API and there are inexplicably some
    // core parser functions that have magic word aliases with hashes baked in
    // which are then registered using `SFH_NO_HASH` (e.g. bcp47). Again,
    // ignoring that this might cause template shadowing (though much less
    // likely than with variables, since parser functions are not considered
    // at all unless there is a colon in the expansion name-part), these are
    // also stripped to make the wiki.rs implementation more normal.
    alias.strip_prefix('#').unwrap_or(alias)
}

/// Trims bad characters from a variable alias.
fn trim_variable<'a>(alias: &'a Cow<'a, str>) -> &'a str {
    // Variables do not normally do this stripping but having a colon in a
    // variable name is “deprecated” as of MW 1.39 (whatever that means, since
    // removing support would break content). Treating them consistently makes
    // template handling significantly less stupid, although technically this
    // means registered variables might end up shadowing templates. Hopefully no
    // one was stupid enough to do this (lol, why do I keep pretending that
    // someone would not do stupid things in MW when there is SO MUCH EVIDENCE
    // to the contrary).
    alias.strip_suffix(':').unwrap_or(alias)
}

/// Data types for the MediaWiki siteinfo API.
mod api {
    use std::{
        borrow::Cow,
        collections::{BTreeMap, BTreeSet},
    };

    pub(super) type Namespaces<'a> = BTreeMap<Cow<'a, str>, Namespace<'a>>;
    pub(super) type NamespaceAliases<'a> = Vec<NamespaceAlias<'a>>;

    #[derive(serde::Deserialize)]
    pub(super) struct Response<'a> {
        #[serde(rename = "batchcomplete")]
        pub batch_complete: bool,
        #[serde(borrow)]
        pub query: Query<'a>,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct General<'a> {
        #[serde(rename = "langconversion")]
        pub lang_conversion: bool,
        #[serde(borrow, rename = "legaltitlechars")]
        pub legal_title_chars: Cow<'a, str>,
        #[serde(borrow, rename = "linktrail")]
        pub link_trail: Cow<'a, str>,
        #[serde(rename = "magiclinks")]
        pub magic_links: MagicLinks,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Interwiki<'a> {
        #[serde(borrow)]
        pub prefix: Cow<'a, str>,
        #[serde(borrow)]
        pub url: Cow<'a, str>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "UPPERCASE")]
    pub(super) struct MagicLinks {
        pub isbn: bool,
        pub pmid: bool,
        pub rfc: bool,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub(super) struct MagicWord<'a> {
        #[serde(borrow)]
        pub aliases: Vec<Cow<'a, str>>,
        #[serde(borrow)]
        pub name: Cow<'a, str>,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Namespace<'a> {
        pub id: i32,
        #[serde(borrow)]
        pub name: Cow<'a, str>,
        #[serde(borrow)]
        pub canonical: Option<Cow<'a, str>>,
        pub case: NamespaceCase,
        pub content: bool,
        #[serde(borrow, rename = "defaultcontentmodel")]
        pub default_content_model: Option<Cow<'a, str>>,
        pub subpages: bool,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct NamespaceAlias<'a> {
        pub id: i32,
        #[serde(borrow)]
        pub alias: Cow<'a, str>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub(super) enum NamespaceCase {
        CaseSensitive,
        FirstLetter,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Query<'a> {
        #[serde(borrow, rename = "doubleunderscores")]
        pub double_underscores: BTreeSet<Cow<'a, str>>,
        #[serde(borrow, rename = "extensiontags")]
        pub extension_tags: BTreeSet<Cow<'a, str>>,
        #[serde(borrow, rename = "functionhooks")]
        pub function_hooks: BTreeSet<Cow<'a, str>>,
        #[serde(borrow)]
        pub general: General<'a>,
        #[serde(borrow, rename = "interwikimap")]
        pub interwiki_map: Vec<Interwiki<'a>>,
        #[serde(borrow, rename = "magicwords")]
        pub magic_words: Vec<MagicWord<'a>>,
        #[serde(borrow)]
        pub namespaces: Namespaces<'a>,
        #[serde(borrow, rename = "namespacealiases")]
        pub namespace_aliases: NamespaceAliases<'a>,
        #[serde(borrow)]
        pub protocols: BTreeSet<Cow<'a, str>>,
        #[serde(borrow)]
        pub variables: BTreeSet<Cow<'a, str>>,
    }
}

/// Uses the [`Display`](core::fmt::Display) formatter for an error even when
/// the [`Debug`](core::fmt::Debug) formatter is requested.
struct DisplayError(Box<dyn std::error::Error>);

impl core::fmt::Debug for DisplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.0, f)
    }
}

impl<E: Into<Box<dyn std::error::Error>>> From<E> for DisplayError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}
