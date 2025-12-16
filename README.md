# ![wiki.rs](.github/ferris.is.my.hero.donotuse.final2.realfinal.jpg_(copy).png)

wiki.rs is a high-performance offline reader of multistream bz2 MediaWiki dumps.

## Usage #######################################################################

Download a [*multistream* dump], decompress *just* the index.txt file, then run:

`wiki-rs [options] <index.txt> <database.xml.bz2>`

wiki.rs will start at `localhost:3000` by default. Run `wiki-rs --help` to see a
list of all the options you can use with wiki.rs to change how it works for you.

[*multistream* dump]:  https://en.wikipedia.org/wiki/Wikipedia:Database_download

## Why did you do this?! #######################################################

Many great people, most of whom are also quite smart and cool, have been talking
about how useful it would be if they could quickly read whatever might be inside
a MediaWiki database dump, given that quite a few sites that run on MediaWiki do
provide them. Alas, most of the readers which already supported this format were
very slow, or were annoying to install, or they did not support all the features
required to actually render a *full & complete* version of many stored articles.
Nerd-snipe someone who is also “not” procrastinating and this what you will get.

### wiki.rs tries to be fast ###################################################

wiki.rs starts up in under one second (when `index.txt` is on an SSD), loads and
renders base articles in hundreds of milliseconds[^1], and can search across all
article titles in a database with tens of millions of records in about a second.

[^1]: Full page renders with templates and scripts can take more time than this.
      This is partly because not much effort has been spent yet on profiling and
      optimising, and partly due to how the multistream bz2 format is simply not
      designed with performance in mind. Caching improves page performance after
      various common templates and modules have already been loaded once before.

### wiki.rs is self-contained ##################################################

wiki.rs and its dependencies are written entirely in Rust. This makes it at once
the trendiest wiki dump reader of the year and also the coolest wiki dump reader
of the year. But mostly it means never having to deal with awful C dependencies,
or PHP engines, or Java runtimes. You do need to bring your own web browser, but
it seems strongly implausible that you don’t already have at least one of those.

### wiki.rs supports all Wikitext features #####################################

wiki.rs supports Wikitext templates, parser functions, extension tags, and comes
with a Lua engine written in pure Rust, so it will render all of the things that
you expect to see, like info boxes, and nav boxes, and message boxes, and all of
the other boxes. So many boxes. Maybe we can edit some of these out during post…

### wiki.rs is beautiful, maybe? ###############################################

wiki.rs tries to offer superior reading experiences with a more designery design
and typography than what you get with a typical MediaWiki installation. However,
because nearly every wiki page is a unique snowflake with bespoke inline styles,
this doesn’t always work perfectly. Also, maybe you hate things that try to look
aesthetic, so this is actually bad? Well, it’s open source, so you can do all of
the things you need to do to create your own personal graphic design heaven. :-)

### wiki.rs has its limits #####################################################

wiki.rs needs separate configuration information for each MediaWiki installation
because this information isn’t included in the database dumps. The configuration
that comes built in to wiki.rs is suitable for use with dumps from a little site
you may have heard of once or twice before named Wikipedia; it may or may not do
the right thing for other wikis right now. Data about which articles belong to a
given category also do not exist in any precomputed form within the database, so
browsing by category isn’t possible, and category pages only show a description.

Media files are not included in these multistream databases, so loading media is
also not currently supported (though could be supported later with an additional
download). Interwiki services will probably never be supported, so articles that
pull data from Wikidata or Wikimedia Commons will be missing whichever facts are
pulled from those other places. (Perhaps the amount of interwiki dependency that
exists right now could be a risk to the mission of providing access to knowledge
over the longer term, particularly since these extra databases are much larger‽)

Multiple extension tags are partially or completely unimplemented at the moment,
which means features like maps show up as blobs of unformatted code rather than,
like, a map or whatever. Also, anything which relies on client-side scripting is
not supported right now. Here are all of the currently supported extension tags:

* `<indicator>`, which is used to show indicator badges at the top of some pages
* `<math>` (partial), which is used to display mathematical formulae and symbols
* `<nowiki>`, which is used to stop sections of text from being read as Wikitext
* `<poem>`, which is used to write a record of a poet’s soul crying out in verse
* `<pre>`, which is used to display preformatted text, exactly like the HTML tag
* `<ref>` & `<references>`, which are used to collect and list article citations
* `<section>` (partial), which is used to paste bits of articles into other ones
* `<syntaxhighlight>`, which is like `<pre>` with syntax highlighting for coders
* `<templatedata>` (stub), which is used to document the parameters of templates
* `<templatestyles>`, which is used to inject separate CSS files into the output
* `<timeline>`, which is used to render one or more time series like a bar chart

## Debugging for nerds #########################################################

Everything is ｂ🆁⌾ʞ𝐄𝓝? Wikitext is a mess of a format, and this is my first day
being a programmer, so of course it is! The first step is to set the environment
variable `RUST_LOG=trace`, and the second step is to question all of the various
life choices which led you to this moment of debugging a random Wikitext reader.

### Cargo profile ##############################################################

When debugging, use the `dev-fast` profile (unless you want to sit there all day
waiting for a slow unoptimised bz2 decompressor): `cargo run --profile dev-fast`

### Ad-hoc evaluation ##########################################################

Visit `/eval` to type arbitrary Wikitext into the mystery box. It will reveal to
you profound secrets about the origin of the universe. Or maybe bugs. Who knows?

### Source inspection ##########################################################

Visit `/source/{Article name}[?mode={mode}][&include]` to view the raw source of
some article. The first column is the byte offset; the second column is the line
number. The third column is the source text, but you probably would have figured
that out yourself (unless it is also *your* first day being a programmer, too?).

* `mode` is used to change the operating mode of the inspector. The choices are:
  * `raw` - Show the raw source directly from the database (this is the default)
  * `tree` - Shows the dump of the abstract syntax tree from the Wikitext parser

* The `include` flag is useful only in `tree` mode. By default the parser is run
  in no-include mode. Setting this flag will run it in the include mode instead.

### Parser output inspection ###################################################

Compiling using `cargo … --features peg/trace` will enable tracing for rust-peg.
This will generate quite a lot of output to `stdout` so is mostly only useful in
some reduced test case that can be inspected with `cargo test … path::to::test`.

The `wikitext::inspectors` module contains helpers for making the parser’s token
trees (which were designed specifically to avoid retaining references to article
text) into outputs which are actually possible for humans to read and interpret.

### Lua inspection #############################################################

When a bad thing happens, it is possible that the true cause of the bad thing is
buried in some earlier `pcall` that silently discarded the original error. Calls
to `pcall` that raise errors are logged at the `debug` log level. (Note that the
`Module:Message box` module is lazy and often makes calls to `mw.title.new` with
a `nil` argument, so this will show up a lot and is unlikely to be the problem.)

Otherwise, a module can be replaced by adding it to `crate::db::HACKS`, and that
replacement can be whatever you want. Try adding a `debug.inspect` (or whatever)
to more easily inspect what the heck is going on inside the Lua virtual machine.

More rarely, modules may break because MediaWiki relies on a version of Lua that
was EOL’d in 2012. Given how little time has passed between 2012 and today, it’s
understandable that this is still the case, but the virtual machine that wiki.rs
uses is written to conform to the *latest* version of Lua. One could be forgiven
for believing that, since these are all 5.x releases, it shouldn’t really matter
since surely they’re fully backwards compatible—but, actually, PUC-Rio don’t use
semantic versioning at all, so minor releases are not actually fully compatible.
Whoops! When these compatibility problems occur, they are almost always due to a
change to the stdlib, and these can be fixed with medication. (See, for example,
the replacements of `table.insert` and `table.remove`, in `lua::stdlib::table`.)
