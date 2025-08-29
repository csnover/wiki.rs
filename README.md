# wiki.rs (β)

wiki.rs is a high-performance offline reader for MediaWiki dumps.

Many MediaWiki installations provide [full database dumps] of all their articles
in a compressed format. Some people think that it would be convenient to quickly
and easily view those dumps directly. Unfortunately, the readers which actually
support this format are almost universally slow—taking hours just to *load* a
dump—or they do not support many of the features required to render a typical
MediaWiki article. This just won’t do<em>‼</em>

[full database dumps]: https://en.wikipedia.org/wiki/Wikipedia:Database_download

## wiki.rs tries to be fast

wiki.rs starts up in under one second (when `index.txt` is on an SSD), loads and
renders base articles in hundreds of milliseconds[^1], and can search across all
article titles in a database with tens of millions of records in a few dozen
milliseconds.

[^1]: Full page renders with templates and scripts can sometimes take much
      longer. This is partly because not much effort has been spent yet on
      profiling and optimising, and partly because the multistream bz2 database
      format is simply not designed with performance in mind. Speed improves
      somewhat over time as common templates and modules are decompressed and
      cached in memory.

## wiki.rs is self-contained

wiki.rs and its dependencies are written entirely in Rust. This makes it both
the trendiest wiki reader of the year and also the coolest wiki reader of the
year. But mostly it means never having to deal with C dependencies, or PHP
engines, or Java runtimes.

(You do need to bring your own web browser, but you’ve got one of those
already.)

## wiki.rs supports all the things

wiki.rs supports templates, parser functions, extension tags, and Lua modules,
so it renders the things you expect to see, like info boxes, and nav boxes, and
message boxes, and all the other boxes. So many boxes.

Speaking of which…

## wiki.rs is beautiful, maybe?

wiki.rs tries to offer a superior reading experience with more designery design
and typography than what you get with a typical MediaWiki installation. However,
because nearly every article in a wiki is actually a unique snowflake with
bespoke inline styles, this doesn’t always work perfectly. Also, maybe you hate
things that try to be aesthetic, so this is actually bad? Well, it’s open
source, so do what you need to do to make yourself happy. :-)

## wiki.rs has its limits

wiki.rs requires external configuration for each MediaWiki installation because
this information is not included in the database dumps. The current built-in
configuration is suitable for use with dumps from a little web site you may have
heard of called Wikipedia. It may or may not work properly with other wikis.

Similarly, precomputed data about which pages belong to which categories does
not exist within the database dump, so it is not possible to list all pages in a
given category, or perform operations which rely on such information.

Media files are not included in article databases, so loading media is not
currently supported. Interwiki services will probably never be supported, which
means any article that relies on Wikidata or Wikimedia Commons will be missing
some data.

Most extension tags are currently stubs, or are unimplemented completely, so
things like maps show up as giant blobs of unformatted code.

Any features which rely on client-side scripts are not supported.

## Usage

You need to first [download a *multistream* dump] and decompress *just the
index.txt* file. Then, run:

`cargo run --release -- [options] <index.txt> <database.xml.bz2>`

By default, wiki.rs will start at `localhost:3000`.

Use `cargo run -- --help` for additional options.

[download a *multistream* dump]: https://en.wikipedia.org/wiki/Wikipedia:Database_download

## Debugging for nerds

Everything is ｂ🆁⌾ʞ𝐄𝓝? Wikitext is a mess of a format and this is my first day
writing code, so of course it is!

First: various things emit logs, so set the environment variable
`RUST_LOG=trace` to see more of those as you ponder your life choices as I once
did.

### Cargo profile

When debugging, use the `dev-fast` profile unless you want to sit there all day
waiting for an unoptimised bz2 decompressor: `cargo run --profile dev-fast`

### Source inspection

The page `/source/{Article name}[?mode={mode}][&include]` shows the source code
of an article. The first column is byte offset and the second column is the line
number. The third column is the source code, but you probably could guess that.

* `mode` options are:
  * `raw` - Show the source code directly from the database
  * `tree` - Show the parsed AST

* `include` flag is:

  In `tree` mode, the default is to view source in noinclude mode. Setting this
  flag shows the source in include (transcluded) mode.

### Parser output inspection

The `wikitext::inspectors` module contains debug helpers for converting token
trees from the parser into more intelligible output.

### Lua inspection

If a bad thing is happening, it is possible that it is buried by a `pcall` that
is silently consuming an earlier error that is the true source of the problem.
Information on errors suppressed by `pcall` are logged at the `debug` log level.
(Note that `Module:Message box` is lazy and calls to `mw.title.new` in a way
that fails, so this will show up a lot, and is unlikely to be the problem.)

Otherwise, a module can be replaced by adding it to `crate::db::HACKS`, and that
modified version can have calls to `debug.inspect` (or whatever) inserted to
more easily inspect the internal state of the VM. Sometimes `print`-based
debugging is the best debugging.

More rarely, modules may break because Scribunto still relies on a version of
Lua which was EOL’d in 2012—which is understandable given how little time has
passed since then—but the VM that wiki.rs uses targets the *current* version of
Lua. One would be forgiven for thinking that because these are all 5.x versions
that it hardly matters since surely they’re backwards compatible, but actually
PUC-Rio don’t follow semver at all. Oops! When a version compatibility problem
happens, it is usually due to a change to the stdlib, and those can be fixed
with surgery (see e.g. `table.insert`).
