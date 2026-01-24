use quote::quote;

// TODO: Copy code bad oog.
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

fn main() -> Result<(), DisplayError> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let mut args = pico_args::Arguments::from_env();
    let prefix = args.free_from_str::<String>().map_err(
        |_| "missing required url argument\n\nUsage: fetch-config https://wiki.example.com",
    )?;
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

    let response = result.into_body().read_json::<api::Response>()?;
    assert!(response.batch_complete);

    let api::Query {
        double_underscores,
        extension_tags,
        function_hooks,
        general,
        interwiki_map,
        magic_words,
        namespaces,
        namespace_aliases,
        protocols,
        variables,
    } = response.query;
    let api::General {
        lang_conversion,
        legal_title_chars,
        link_trail,
        magic_links,
    } = general;
    let api::MagicLinks { isbn, pmid, rfc } = magic_links;

    let double_underscores = double_underscores.into_iter().map(|v| {
        let v = v.to_ascii_lowercase();
        quote!(#v)
    });
    let extension_tags = extension_tags.into_iter().map(|tag| {
        let tag = tag[1..tag.len() - 1].to_ascii_lowercase();
        quote!(#tag)
    });
    let function_hooks = function_hooks.into_iter().map(|v| {
        let v = v.to_ascii_lowercase();
        quote!(#v)
    });
    let interwiki_map = interwiki_map.into_iter().map(|v| {
        let k = v.prefix.to_ascii_lowercase();
        let v = &v.url;
        quote!(#k => #v)
    });
    let namespaces = namespaces.into_values().map(
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
    );
    let protocols = protocols.into_iter().map(|v| {
        let v = v.to_ascii_lowercase();
        quote!(#v)
    });
    let redirects = magic_words
        .into_iter()
        .find_map(|v| {
            v.name.eq_ignore_ascii_case("redirect").then(|| {
                v.aliases
                    .into_iter()
                    .map(|v| {
                        let v = v.to_ascii_lowercase();
                        quote!(#v)
                    })
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_default();
    let variables = variables.into_iter().map(|v| {
        let v = v.to_ascii_lowercase();
        quote!(#v)
    });

    let file: syn::File = syn::parse_quote! {
        static CONFIG_SOURCE: ConfigurationSource = ConfigurationSource {
            annotation_tags: phf::phf_set! {},
            annotations_enabled: false,
            behavior_switch_words: phf::phf_set! {
                #(#double_underscores),*
            },
            extension_tags: phf::phf_set! {
                #(#extension_tags),*
            },
            function_hooks: phf::phf_set! {
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
            variables: phf::phf_set! {
                #(#variables),*
            }
        };
    };

    println!("{}", prettyplease::unparse(&file));

    Ok(())
}

mod api {
    #[derive(serde::Deserialize)]
    pub(super) struct Response {
        #[serde(rename = "batchcomplete")]
        pub batch_complete: bool,
        pub query: Query,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct General {
        #[serde(rename = "langconversion")]
        pub lang_conversion: bool,
        #[serde(rename = "legaltitlechars")]
        pub legal_title_chars: String,
        #[serde(rename = "linktrail")]
        pub link_trail: String,
        #[serde(rename = "magiclinks")]
        pub magic_links: MagicLinks,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Interwiki {
        pub prefix: String,
        pub url: String,
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
    pub(super) struct MagicWord {
        pub aliases: Vec<String>,
        pub name: String,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Namespace {
        pub id: i32,
        pub name: String,
        pub canonical: Option<String>,
        pub case: NamespaceCase,
        pub content: bool,
        #[serde(rename = "defaultcontentmodel")]
        pub default_content_model: Option<String>,
        pub subpages: bool,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct NamespaceAlias {
        pub id: i32,
        pub alias: String,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub(super) enum NamespaceCase {
        CaseSensitive,
        FirstLetter,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Query {
        #[serde(rename = "doubleunderscores")]
        pub double_underscores: Vec<String>,
        #[serde(rename = "extensiontags")]
        pub extension_tags: Vec<String>,
        #[serde(rename = "functionhooks")]
        pub function_hooks: Vec<String>,
        pub general: General,
        #[serde(rename = "interwikimap")]
        pub interwiki_map: Vec<Interwiki>,
        #[serde(rename = "magicwords")]
        pub magic_words: Vec<MagicWord>,
        pub namespaces: std::collections::BTreeMap<String, Namespace>,
        #[serde(rename = "namespacealiases")]
        pub namespace_aliases: Vec<NamespaceAlias>,
        pub protocols: Vec<String>,
        pub variables: Vec<String>,
    }
}
