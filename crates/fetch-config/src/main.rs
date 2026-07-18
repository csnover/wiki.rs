//! Fetches a MediaWiki configuration from a remote server and emits it to
//! stdout as Rust source code.

#![allow(
    clippy::missing_docs_in_private_items,
    clippy::too_many_lines,
    reason = "this is a boring utility"
)]

use core::fmt;
use proc_macro2::{Literal, TokenStream};
use quote::quote;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
};

#[derive(Debug, thiserror::Error)]
#[error("{err}\n... {}", {
    let line_col = err.line().checked_sub(1).zip(err.column().checked_sub(1));
    let text = line_col.and_then(|(line, col)| {
        body.lines().nth(line).map(|line| &line[col..line.ceil_char_boundary(col + 64)])
    });
    text.unwrap_or("?")
})]
struct SerdeError {
    body: String,
    err: serde_json::Error,
}

type MagicWords<'a> = HashMap<&'a str, &'a [Cow<'a, str>]>;

/// Beware! Running this function may cause sadness and bleeding from orifices.
fn main() -> Result<(), DisplayError> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let mut args = pico_args::Arguments::from_env();
    let prefix = args.free_from_str::<String>().map_err(
        |_| "missing required url argument\n\nUsage: fetch-config https://wiki.example.com",
    )?;
    let body = fetch(&prefix)?;
    let response = match serde_json::from_str::<api::Response<'_>>(&body) {
        Ok(value) => value,
        Err(err) => return Err(SerdeError { body, err })?,
    };

    if !response.batch_complete {
        Err("batchcomplete was false")?;
    }

    let query = response.query;

    let case_sensitive_words = {
        // These aliases may be duplicated after trimming, so it is necessary to
        // collect in order to deduplicate. As a free bonus this also makes the
        // output sorted in a way which is satisfying for anyone with OCD
        let mut map = BTreeMap::new();
        let case_sensitive_words = query
            .magic_words
            .iter()
            .filter_map(|word| (word.case_sensitive).then_some((&word.name, &word.aliases)));
        for (name, aliases) in case_sensitive_words {
            let trim = if query.function_hooks.contains(name) {
                trim_parser_fn
            } else if query.variables.contains(name) {
                trim_variable
            } else {
                fn identity(s: &str) -> &str {
                    s
                }
                identity
            };
            for alias in aliases {
                let alias = trim(alias);
                map.insert(alias.to_lowercase(), alias);
            }
        }
        map.into_iter()
            .map(|(folded, alias)| quote!(#folded => #alias))
    };

    let magic_words = query
        .magic_words
        .iter()
        .map(|word| (word.name.as_ref(), word.aliases.as_slice()))
        .collect::<HashMap<_, _>>();

    let extra_words = {
        let mut map = BTreeMap::<_, Vec<&str>>::new();
        let extra_words = magic_words.iter().filter(|&(word, _)| {
            !query.double_underscores.contains(*word)
                && !query.function_hooks.contains(*word)
                && !query.variables.contains(*word)
                && *word != "redirect"
        });
        for (word, aliases) in extra_words {
            for alias in *aliases {
                let words = map.entry(alias.to_lowercase()).or_default();
                words.push(word);
            }
        }
        map.into_iter().map(|(alias, keys)| {
            let keys = keys.into_iter().map(str::to_lowercase);
            quote!(#alias => &[#(#keys),*])
        })
    };

    // At some point, MW had no restriction at all on what these magic words
    // looked like, so some wikis actually used double fullwidth low lines
    // instead of underscores. As such, the entire alias including the
    // underscores needs to be kept, even though this is a waste of time and
    // memory in 99% of cases.
    let double_underscores = aliases_iter(&magic_words, query.double_underscores, &|alias| alias);

    let extension_tags = query.extension_tags.into_iter().map(|tag| {
        let tag = tag[1..tag.len() - 1].to_lowercase();
        quote!(#tag)
    });

    let function_hooks = aliases_iter(&magic_words, &query.function_hooks, &trim_parser_fn);

    let api::General {
        gallery_options,
        image_whitelist,
        image_whitelist_enabled,
        lang,
        lang_conversion,
        legal_title_chars,
        link_prefix,
        link_trail,
        magic_links,
        main_page,
        thumb_limits,
    } = query.general;

    let gallery_options = {
        let api::GalleryOptions {
            caption_length,
            height,
            mode,
            per_row,
            show_bytes,
            show_dimensions,
            width,
        } = gallery_options;

        let caption_length = match caption_length {
            api::GalleryCaptionLength::Fixed(value) => {
                let value = Literal::u32_unsuffixed(value);
                quote!(GalleryCaptionLength::Fixed(#value))
            }
            api::GalleryCaptionLength::UseCss(value) => {
                quote!(GalleryCaptionLength::UseCss(#value))
            }
        };

        let height = Literal::u32_unsuffixed(height);

        let mode = match mode.as_ref() {
            "nolines" => quote!(NoLines),
            "packed" => quote!(Packed),
            "packed-hover" => quote!(PackedHover),
            "packed-overlay" => quote!(PackedOverlay),
            "slideshow" => quote!(Slideshow),
            _ => quote!(Traditional),
        };

        let per_row = if per_row <= 0 {
            quote!(None)
        } else {
            let per_row = Literal::i32_unsuffixed(per_row);
            quote!(Some(#per_row))
        };

        let width = Literal::u32_unsuffixed(width);

        quote! {
            GalleryOptions {
                caption_length: #caption_length,
                height: #height,
                mode: GalleryMode::#mode,
                per_row: #per_row,
                show_bytes: #show_bytes,
                show_dimensions: #show_dimensions,
                width: #width,
            }
        }
    };

    // To use the wfMessage whitelist, image_whitelist_enabled is true and
    // image_whitelist is `[""]`.
    // To use any external image, image_whitelist is `[""]` and
    // image_whitelist_enabled is false.
    // To use a whitelist, image_whitelist_enabled is true and image_whitelist
    // is not `[""]`.
    // Good API!
    let image_hotlinking = if image_whitelist_enabled.is_none() && image_whitelist.is_some() {
        quote!(Enabled)
    } else if image_whitelist_enabled == Some(false) && image_whitelist.is_none() {
        quote!(Disabled)
    } else {
        let config = image_whitelist.into_iter().flatten();
        let message = image_whitelist_enabled.unwrap();
        quote!(Whitelist {
            config: &[#(#config),*],
            message: #message,
        })
    };

    let interlanguage_map = query.interwiki_map.iter().filter_map(|v| {
        v.bcp47.as_ref().map(|bcp47| {
            let k = v.prefix.to_ascii_lowercase();
            quote!(#k => #bcp47)
        })
    });

    let interwiki_map = query.interwiki_map.iter().map(|v| {
        let k = v.prefix.to_lowercase();
        let v = &v.url;
        quote!(#k => #v)
    });

    let interwiki_self = query
        .interwiki_map
        .iter()
        .filter(|&v| v.self_alias)
        .map(|v| {
            let k = v.prefix.to_lowercase();
            quote!(#k)
        });

    // Old language codes might map to the same BCP-47s. In an effort to avoid
    // making everything else bad for this one edge case, just avoid emitting
    // the same BCP-47 twice. To keep the indexes correct, dumping into a set
    // seems easiest, though probably technically in this case it should make
    // sure to alias back to the standard code instead of a non-standard one.
    let mut emitted_bcp47 = HashSet::new();
    let language_bcp47 = query
        .language_info
        .values()
        .enumerate()
        .filter_map(|(index, lang)| {
            let bcp47 = &lang.bcp47;
            let index = Literal::usize_unsuffixed(index);
            emitted_bcp47
                .insert(bcp47)
                .then(|| quote!(#bcp47 => #index))
        });

    let enabled_languages = query
        .languages
        .into_iter()
        .map(|lang| lang.code)
        .collect::<HashSet<_>>();

    let languages = query.language_info.iter().map(|(code, lang)| {
        let autonym = &lang.autonym;
        let is_enabled = enabled_languages.contains(code);
        let is_rtl = lang.dir == "rtl";
        let name = &lang.name;
        quote!(#code => Language {
            autonym: #autonym,
            is_enabled: #is_enabled,
            is_rtl: #is_rtl,
            name: #name,
        })
    });

    let language_conversions = query.language_variants.into_values().flat_map(|variants| {
        variants.into_iter().map(|(k, v)| {
            let v = &v.fallbacks;
            quote!(#k => &[ #(#v),* ])
        })
    });

    let api::MagicLinks { isbn, pmid, rfc } = magic_links;

    let namespaces = namespaces_iter(query.namespaces, &query.namespace_aliases);

    let protocols = query.protocols.into_iter().map(|v| {
        let v = v.to_ascii_lowercase();
        quote!(#v)
    });

    let redirects = redirects_iter(&magic_words);

    // These aliases are case-insensitive, but that does not stop them from
    // being duplicated in the source configuration, so like the case-sensitive
    // words, deduplication is needed
    let (mut special_pages_aliases, mut special_pages_canonical) =
        (BTreeMap::new(), BTreeMap::new());
    for page in &query.special_page_aliases {
        let v = page.real_name.as_ref();
        if let Some(alias) = page.aliases.first()
            && alias != v
        {
            special_pages_canonical.insert(v, alias.as_ref());
        }
        for alias in &page.aliases {
            let k = alias.to_lowercase();
            special_pages_aliases.insert(k, v);
        }
    }
    let special_pages_aliases = special_pages_aliases
        .into_iter()
        .map(|(k, v)| quote!(#k => #v));
    let special_pages_canonical = special_pages_canonical
        .into_iter()
        .map(|(k, v)| quote!(#k => #v));

    let thumb_limits = thumb_limits.into_values().map(Literal::u32_unsuffixed);

    let variables = aliases_iter(&magic_words, &query.variables, &trim_variable);

    let file: syn::File = syn::parse_quote! {
        static CONFIG_SOURCE: ConfigurationSource = ConfigurationSource {
            annotation_tags: phf::phf_set! {},
            annotations_enabled: false,
            behavior_switch_words: phf::phf_map! {
                #(#double_underscores),*
            },
            case_sensitive_words: phf::phf_map! {
                #(#case_sensitive_words),*
            },
            extension_tags: phf::phf_set! {
                #(#extension_tags),*
            },
            extra_words: phf::phf_map! {
                #(#extra_words),*
            },
            function_hooks: phf::phf_map! {
                #(#function_hooks),*
            },
            gallery_options: #gallery_options,
            image_hotlinking: ImageHotlinking::#image_hotlinking,
            interlanguage_map: phf::phf_map! {
                #(#interlanguage_map),*
            },
            interwiki_map: phf::phf_map! {
                #(#interwiki_map),*
            },
            interwiki_self: phf::phf_set! {
                #(#interwiki_self),*
            },
            language: #lang,
            language_bcp47: phf::phf_map! {
                #(#language_bcp47),*
            },
            language_conversion_enabled: #lang_conversion,
            language_conversions: phf::phf_map! {
                #(#language_conversions),*
            },
            languages: phf::phf_ordered_map! {
                #(#languages),*
            },
            link_prefix: #link_prefix,
            link_trail: #link_trail,
            magic_links: MagicLinks {
                isbn: #isbn,
                pmid: #pmid,
                rfc: #rfc,
            },
            main_page: #main_page,
            namespaces: &[ #(#namespaces),* ],
            protocols: phf::phf_set! {
                #(#protocols),*
            },
            redirect_magic_words: phf::phf_set! {
                #(#redirects),*
            },
            special_pages: SpecialPages {
                aliases: phf::phf_map! {
                    #(#special_pages_aliases),*
                },
                canonical: phf::phf_map! {
                    #(#special_pages_canonical),*
                },
            },
            thumb_limits: &[ #(#thumb_limits),* ],
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
fn aliases_iter<'a, F, I, T>(
    magic_words: &'a MagicWords<'_>,
    items: I,
    transform: &F,
) -> impl Iterator<Item = TokenStream>
where
    F: for<'b> Fn(&'b str) -> &'b str,
    I: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    items.into_iter().flat_map(move |key| {
        let key = key.as_ref();
        let aliases = magic_words.get(key).copied().unwrap_or_default();
        let key = key.to_lowercase();
        aliases.iter().map(move |alias| {
            let alias = transform(alias).to_lowercase();
            quote!(#alias => #key)
        })
    })
}

/// Fetches site metadata from a MediaWiki instance.
fn fetch(prefix: &str) -> Result<String, ureq::Error> {
    const LANG_PROPS: &str = concat!("autonym", "|bcp47", "|dir", "|name",);

    // 'restrictions' might also be interesting
    // 'languagevariants' will be relevant when that is implemented
    const SITE_PROPS: &str = concat!(
        "doubleunderscores",
        "|extensiontags",
        "|functionhooks",
        "|general",
        "|interwikimap",
        "|languages",
        "|languagevariants",
        "|magicwords",
        "|namespaces",
        "|namespacealiases",
        "|protocols",
        "|specialpagealiases",
        "|variables",
    );

    let result = ureq::get(format!("{prefix}/w/api.php"))
        .query("action", "query")
        .query("meta", "languageinfo|siteinfo")
        .query("format", "json")
        .query("formatversion", "2")
        .query("errorformat", "plaintext")
        .query("liprop", LANG_PROPS)
        .query("siprop", SITE_PROPS)
        .header(
            "User-Agent",
            format!(
                "wiki.rs {}/{}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            ),
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
            let id = Literal::i32_unsuffixed(id);
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
        .copied()
        .unwrap_or_default()
        .iter()
        .map(|alias| {
            let v = alias.to_lowercase();
            quote!(#v)
        })
}

/// Trims bad characters from a variable or parser function alias.
fn trim_parser_fn(alias: &str) -> &str {
    // In MW, `Parser::setFunctionHook` does this unconditional stripping of the
    // trailing colon.
    let alias = alias.strip_suffix([':', '：']).unwrap_or(alias);

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
fn trim_variable(alias: &str) -> &str {
    // Variables do not normally do this stripping but having a colon in a
    // variable name is “deprecated” as of MW 1.39 (whatever that means, since
    // removing support would break content). Treating them consistently makes
    // template handling significantly less stupid, although technically this
    // means registered variables might end up shadowing templates. Hopefully no
    // one was stupid enough to do this (lol, why do I keep pretending that
    // someone would not do stupid things in MW when there is SO MUCH EVIDENCE
    // to the contrary).
    alias.strip_suffix([':', '：']).unwrap_or(alias)
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
    pub(super) struct EnabledLanguage<'a> {
        #[serde(borrow)]
        pub code: Cow<'a, str>,
    }

    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    pub(super) enum GalleryCaptionLength {
        Fixed(u32),
        UseCss(bool),
    }

    #[derive(serde::Deserialize)]
    pub(super) struct GalleryOptions<'a> {
        #[serde(rename = "captionLength")]
        pub caption_length: GalleryCaptionLength,
        #[serde(rename = "imageHeight")]
        pub height: u32,
        #[serde(borrow)]
        pub mode: Cow<'a, str>,
        #[serde(rename = "imagesPerRow")]
        pub per_row: i32,
        #[serde(rename = "showBytes")]
        pub show_bytes: bool,
        #[serde(rename = "showDimensions")]
        pub show_dimensions: bool,
        #[serde(rename = "imageWidth")]
        pub width: u32,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct General<'a> {
        #[serde(borrow, rename = "galleryoptions")]
        pub gallery_options: GalleryOptions<'a>,
        #[serde(borrow, rename = "externalimages")]
        pub image_whitelist: Option<Vec<Cow<'a, str>>>,
        #[serde(rename = "imagewhitelistenabled")]
        pub image_whitelist_enabled: Option<bool>,
        #[serde(borrow)]
        pub lang: Cow<'a, str>,
        #[serde(rename = "langconversion")]
        pub lang_conversion: bool,
        #[serde(borrow, rename = "legaltitlechars")]
        pub legal_title_chars: Cow<'a, str>,
        #[serde(borrow, rename = "linkprefixcharset")]
        pub link_prefix: Cow<'a, str>,
        #[serde(borrow, rename = "linktrail")]
        pub link_trail: Cow<'a, str>,
        #[serde(rename = "magiclinks")]
        pub magic_links: MagicLinks,
        #[serde(borrow, rename = "mainpage")]
        pub main_page: Cow<'a, str>,
        #[serde(rename = "thumblimits")]
        pub thumb_limits: BTreeMap<u32, u32>,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Interwiki<'a> {
        #[serde(borrow)]
        pub bcp47: Option<Cow<'a, str>>,
        #[serde(borrow)]
        pub prefix: Cow<'a, str>,
        #[serde(default, rename = "localinterwiki")]
        pub self_alias: bool,
        #[serde(borrow)]
        pub url: Cow<'a, str>,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Language<'a> {
        #[serde(borrow)]
        pub autonym: Cow<'a, str>,
        #[serde(borrow)]
        pub bcp47: Cow<'a, str>,
        #[serde(borrow)]
        pub dir: Cow<'a, str>,
        #[serde(borrow)]
        pub name: Cow<'a, str>,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct LanguageVariant<'a> {
        #[serde(borrow)]
        pub fallbacks: Vec<Cow<'a, str>>,
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
        pub case_sensitive: bool,
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
    pub(super) struct SpecialPageAlias<'a> {
        #[serde(borrow, rename = "realname")]
        pub real_name: Cow<'a, str>,
        #[serde(borrow)]
        pub aliases: Vec<Cow<'a, str>>,
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
        #[serde(borrow, rename = "languageinfo")]
        pub language_info: BTreeMap<Cow<'a, str>, Language<'a>>,
        #[serde(borrow, rename = "languagevariants")]
        pub language_variants: BTreeMap<Cow<'a, str>, BTreeMap<Cow<'a, str>, LanguageVariant<'a>>>,
        #[serde(borrow)]
        pub languages: Vec<EnabledLanguage<'a>>,
        #[serde(borrow, rename = "magicwords")]
        pub magic_words: Vec<MagicWord<'a>>,
        #[serde(borrow)]
        pub namespaces: Namespaces<'a>,
        #[serde(borrow, rename = "namespacealiases")]
        pub namespace_aliases: NamespaceAliases<'a>,
        #[serde(borrow)]
        pub protocols: BTreeSet<Cow<'a, str>>,
        #[serde(borrow, rename = "specialpagealiases")]
        pub special_page_aliases: Vec<SpecialPageAlias<'a>>,
        #[serde(borrow)]
        pub variables: BTreeSet<Cow<'a, str>>,
    }
}

/// Uses the [`Display`](core::fmt::Display) formatter for an error even when
/// the [`Debug`](core::fmt::Debug) formatter is requested.
struct DisplayError(Box<dyn core::error::Error>);

impl fmt::Debug for DisplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<E: Into<Box<dyn core::error::Error>>> From<E> for DisplayError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}
