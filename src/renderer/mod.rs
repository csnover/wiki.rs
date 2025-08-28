use crate::db::{Database, article::Article};
use parse_wiki_text::{Configuration, Node, Parameter};
use std::borrow::Cow;
use config::{CONFIG, MAGIC_WORDS};

mod config;

#[derive(Debug, thiserror::Error)]
pub enum Error {}

pub struct ArticleRenderer<'a> {
    html: String,
    is_italic: bool,
    is_bold: bool,
    is_bold_italic: bool,
    is_comment: bool,
    transcluding: usize,
    config: Configuration,
    db: &'a Database<'a>,
    references: Vec<String>,
    stack: Vec<String>,
}

impl<'a> ArticleRenderer<'a> {
    pub fn render(db: &'a Database<'a>, article: &Article) -> Result<String, Error> {
        let mut renderer = Self::new(db);
        let root = renderer.config.parse(article.body.as_str());
        renderer.render_nodes(&root.nodes, &[]);
        Ok(renderer.html)
    }

    fn new(db: &'a Database<'a>) -> Self {
        Self {
            html: String::new(),
            is_italic: false,
            is_bold: false,
            is_bold_italic: false,
            is_comment: false,
            transcluding: 0,
            config: Configuration::new(&CONFIG),
            db,
            references: <_>::default(),
            stack: <_>::default(),
        }
    }

    fn render_nodes(&mut self, nodes: &[Node], args: &[Parameter<'_>]) {
        for node in nodes {
            self.render_node(node, args);
        }
    }

    fn render_node(&mut self, node: &Node, args: &[Parameter<'_>]) {
        match node {
            Node::Bold { .. } => {
                if !self.is_bold {
                    self.open_tag("strong");
                } else {
                    self.close_tag("strong");
                }
                self.is_bold = !self.is_bold;
            }
            Node::BoldItalic { .. } => {
                if !self.is_bold_italic {
                    self.open_tag("strong");
                    self.open_tag("em");
                } else {
                    self.close_tag("em");
                    self.close_tag("strong");
                }
                self.is_bold_italic = !self.is_bold_italic;
            }
            Node::Italic { .. } => {
                if !self.is_italic {
                    self.open_tag("em");
                } else {
                    self.close_tag("em");
                }
                self.is_italic = !self.is_italic;
            }
            Node::Comment { .. } => {
                if !self.is_comment {
                    self.append("<!--");
                } else {
                    self.append("-->");
                }
                self.is_comment = !self.is_comment;
            }
            Node::Category { target, .. } => {
                self.append(&format!(
                    r#"<a class="category" href="/article/{target}">{target}</a>"#,
                ));
            }
            Node::CharacterEntity { character, .. } => {
                self.append_chr(*character);
            }
            Node::DefinitionList { items, .. } => {
                self.open_tag("dl");
                for itm in items {
                    self.render_nodes(&itm.nodes, args);
                }
                self.close_tag("dl");
            }
            Node::StartTag { name, .. } => {
                self.open_tag(name);
            }
            Node::EndTag { name, .. } => {
                self.close_tag(name);
            }
            Node::Heading { level, nodes, .. } => {
                let tag_name = format!("h{level}");

                self.open_tag(&tag_name);
                self.render_nodes(nodes, args);
                self.close_tag(&tag_name);
            }
            Node::HorizontalDivider { .. } => {
                self.void_tag("hr");
            }
            Node::Image { target, text, .. } => {
                self.open_tag("figure");
                self.append(&format!(r#"<img src="{target}"/>"#));

                self.open_tag("figcaption");
                self.render_nodes(text, args);
                self.close_tag("figcaption");
                self.close_tag("figure");
            }
            Node::Link { target, text, .. } => {
                self.append(&format!("<a href=\"/article/{target}\">"));
                self.render_nodes(text, args);
                self.append("</a>");
            }
            Node::Redirect { target, .. } => {
                self.append(&format!("<a href=\"/article/{target}\">Redirect</a>"));
            }
            Node::ExternalLink { nodes, .. } => {
                self.append(r##"<a href="#">"##);
                self.render_nodes(nodes, args);
                self.append("</a>");
            }
            Node::ParagraphBreak { .. } => {
                self.void_tag("p");
            }
            Node::MagicWord { start, end } => {
                // These are not all magic words, just the “behavior switches”
                log::trace!("MagicWord at {start} {end}");
            }
            Node::Parameter { name, default, .. } => {
                fn text_content<'b>(nodes: &[Node<'b>]) -> Cow<'b, str> {
                    match nodes {
                        [Node::Text { value, .. }] => Cow::Borrowed(value),
                        nodes => {
                            let mut text = String::new();
                            for node in nodes {
                                if let Node::Text { value, .. } = node {
                                    text += value;
                                }
                            }
                            Cow::Owned(text)
                        }
                    }
                }

                let param_name = text_content(name);
                let value = args
                    .iter()
                    .enumerate()
                    .find_map(|(index, arg)| {
                        let arg_name = arg.name.as_deref().map(text_content).unwrap_or_else(|| {
                            // Parameter 0 was the template name
                            Cow::Owned(format!("{}", index + 1))
                        });

                        (param_name == arg_name).then_some(&arg.value)
                    })
                    .or(default.as_ref());

                if let Some(value) = value {
                    self.render_nodes(value, args);
                } else {
                    log::trace!("ERROR: Could not find parameter {name:?};\n\nARGS:  {args:?}");
                }
            }
            Node::Template {
                name, parameters, ..
            } => {
                // let parameters = parameters.clone().extend_from_slice(args);
                self.render_template(name, parameters);
            }
            Node::Preformatted { nodes, .. } => {
                self.open_tag("pre");
                self.render_nodes(nodes, args);
                self.close_tag("pre");
            }
            Node::Table {
                attributes,
                captions,
                rows,
                ..
            } => {
                self.append("<table ");
                self.render_nodes(attributes, args);
                self.append(">");

                self.append("<thead><tr>");
                for cap in captions {
                    self.append("<th ");
                    if let Some(attributes) = cap.attributes.as_ref() {
                        self.render_nodes(attributes, args);
                    }
                    self.append(">");

                    self.render_nodes(&cap.content, args);
                    self.append("</th>");
                }
                self.append("</tr></thead>");

                self.append("<tbody>");
                for row in rows {
                    self.append("<tr ");
                    self.render_nodes(&row.attributes, args);
                    self.append(">");

                    for cell in &row.cells {
                        self.append("<td ");
                        if let Some(attributes) = cell.attributes.as_ref() {
                            self.render_nodes(attributes, args);
                        }
                        self.append(">");

                        self.render_nodes(&cell.content, args);
                        self.append("</td>");
                    }
                    self.append("</tr>");
                }
                self.append("</tbody></table>");
            }
            Node::Tag { name, nodes, .. } => self.render_tag(name, nodes, args),
            Node::Text { value, .. } => {
                self.append(value);
            }
            Node::OrderedList { items, .. } => {
                self.open_tag("ol");
                for itm in items {
                    self.open_tag("li");
                    self.render_nodes(&itm.nodes, args);
                    self.close_tag("li");
                }
                self.close_tag("ol");
            }
            Node::UnorderedList { items, .. } => {
                self.open_tag("ul");
                for itm in items {
                    self.open_tag("li");
                    self.render_nodes(&itm.nodes, args);
                    self.close_tag("li");
                }
                self.close_tag("ul");
            }
        }
    }

    fn is_template(name: &[Node<'_>]) -> Option<String> {
        /* From <https://www.mediawiki.org/wiki/Special:MyLanguage/Manual:Magic_words#How_magic_words_work>:

        1. Does it have an associated magic word ID? As a first step in
           resolving markup of the form {{XXX...}}, MediaWiki attempts to
           translate XXX to a magic word ID. The translation table is defined by
           $magicWords. If no magic word ID is associated with XXX, XXX is
           presumed to be a template.

        2. Is it a variable? If a magic word ID is found, MediaWiki next
           checks to see if it has any parameters. If no parameters are found,
           MediaWiki checks to see if the magic word ID has been declared as a
           variable ID. […] If the magic word ID has been classified as a
           variable, MediaWiki calls the ParserGetVariableValueSwitch function
           to get the value associated with the variable name.

        3. Is it a parser function? If there are any parameters or if the
           magic word ID is missing from the list of variable magic word IDs,
           then MediaWiki assumes that the magic word is a parser function or
           template. If the magic word ID is found in the list of declared
           parser functions, it is treated as a parser function and rendered
           using the function named $renderingFunctionName. Otherwise, it is
           presumed to be a template.
        */
        if let [Node::Text { value: name, .. }] = name
            && !name.starts_with('#')
        {
            let name = name
                .split_once(':')
                .map_or(*name, |(name, _)| name)
                .to_ascii_lowercase();
            if MAGIC_WORDS.contains(&name.as_str()) {
                None
            } else {
                let (first, rest) = name.split_at(1);
                Some(format!("Template:{}{rest}", first.to_ascii_uppercase()))
            }
        } else {
            None
        }
    }

    fn render_template(&mut self, name: &[Node<'_>], args: &[Parameter<'_>]) {
        // TODO: Insert a template from the Template: namespace, or if the template
        //       is {{#invoke:$name|$arg1|...}}, it's a Lua template from the Module: namespace

        // TODO: If the name split on : then we are missing parameters
        if let Some(tpl_name) = Self::is_template(name) {
            if let Ok(template) = self.db.get(&tpl_name) {
                let root = self.config.parse(template.body.as_str());
                self.transcluding += 1;
                self.render_nodes(&root.nodes, args);
                self.transcluding -= 1;
                log::trace!("=== END {name:?}");
            } else {
                log::trace!("ERROR: No template found for {name:?}");
            }
            return;
        }

        log::trace!("TODO: Template {name:?}");
        self.append("<details><summary>");
        self.render_nodes(name, args);
        self.append("</summary>");
        self.append("<dl>");
        for param in args {
            if let Some(name) = &param.name {
                self.append("<dt>");
                self.render_nodes(name, args);
                self.append("</dt>");
            }
            self.append("<dd>");
            self.render_nodes(&param.value, args);
            self.append("</dd>");
        }
        self.append("</dl></details>");
        log::trace!("=== END {name:?}");
    }

    fn render_tag(&mut self, name: &str, nodes: &[Node<'_>], args: &[Parameter<'_>]) {
        match name {
            "noinclude" if self.transcluding != 0 => {}
            "includeonly" if self.transcluding == 0 => {}
            "onlyinclude" => {
                log::trace!("TODO: Insane <onlyinclude> nonsense");
            }
            "nowiki" => {
                log::trace!("TODO: Nowiki: {nodes:?}");
            }
            "noinclude" | "includeonly" => self.render_nodes(nodes, args),
            "ref" => {
                // Due to transclusion it is necessary to render immediately
                // instead of storing the node list for later
                let mut renderer = Self::new(self.db);
                renderer.render_nodes(nodes, args);
                self.references.push(renderer.html);
                self.append(&format!(
                    r##"<a href="#ref_{0}">[{0}]</a>"##,
                    self.references.len()
                ));
            }
            "references" => {
                self.open_tag("ol");
                for (id, reference) in core::mem::take(&mut self.references).iter().enumerate() {
                    self.append(&format!(r#"<li id="ref_{id}">"#));
                    self.append(reference);
                    self.close_tag("li");
                }
                self.close_tag("ol");
            }
            _ => {
                log::trace!("Tag {name}");
                self.open_tag(name);
                self.render_nodes(nodes, args);
                self.close_tag(name);
            }
        }
    }

    fn append(&mut self, data: &str) {
        self.html.push_str(data);
    }

    fn append_chr(&mut self, data: char) {
        self.html.push(data);
    }

    fn open_tag(&mut self, tag: &str) {
        self.append_chr('<');
        self.append(tag);
        self.append_chr('>');
    }

    fn close_tag(&mut self, tag: &str) {
        self.append("</");
        self.append(tag);
        self.append_chr('>');
    }

    fn void_tag(&mut self, tag: &str) {
        self.append_chr('<');
        self.append(tag);
        self.append("/>");
    }
}
