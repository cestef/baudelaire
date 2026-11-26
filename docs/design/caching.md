# Caching and fingerprints

Read before touching `graph/`, `Prepare`, `Renderer`, an emitter's `inputs`, or anything that feeds a page's wrapper.

**`mode` must stay out of the cache fingerprint**, or `build` and `serve` invalidate
each other's cache on every switch.

**Render-side inputs are tracked per page, not fingerprinted whole.** A page records
what it actually consulted: the permalinks its links resolved to (`Entry::links`), the
variants its images matched (`Entry::srcsets`), the assets its references named
(`Entry::assets`). Editing an asset no page links rebuilds nothing. Each of these
records *negative* probes too, and must: a reference to something absent has to
invalidate when that thing appears. Narrowing one of these without recording the
absences is how you ship stale HTML out of a green build.

**What typst does not open, no page can record.** `Deps` is built from the file
store's accessed paths, so anything a build reads by another route is invisible to
per-page tracking and has to be fingerprinted whole, in `SiteInputs`. There are two:
the generated `/.baudelaire/site/*.typ` modules (served from memory, so they resolve to no
path) and the site's own fonts (`typst { fonts { paths } }`, resolved by *name* out
of a store built by walking a directory). The font case shipped stale: naming a
directory was in the config hash, replacing a file inside one already named was not,
so a new cut of a face rebuilt nothing and the cards and PDFs kept the old outlines
out of a green build. Adding a third such source means a third field there.

**A processor that answers `Processor::inputs` promises `claims()` is complete.**
`inputs` names the slices of the site a whole-site pass reads (`Reads` in
`engine/emit/reads.rs`); when they hash the same as the last build's, the pass
does not run. Two things hold that up, and both are silent when broken: a slice
left out of the list is a file that stops being regenerated, and a skip has to
push every `claims()` path through `Emit::keep`, because the sweep deletes
whatever this build did not produce. That is why `Redirects` and `Feeds` answer
`None` in the configurations where they write files at destinations the page set
decides, and `Epub` always: `claims()` cannot name those, so nothing would keep
them. `Reads::Listing` deliberately excludes a page's own body, which reaches a
listing through `Reads::Markup`; hashing it there would rebuild every sitemap on
every edit.

**Narrowing a cache fingerprint means bumping `Renderer::SCHEMA`.** A warm manifest
written under the old, wider rules records none of the new per-page probes, so every
entry reads as valid under the narrower ones. That is a site-wide stale serve on
upgrade, and the bump is the only thing standing in front of it.

**Nothing site-wide may go into a page's wrapper text.** `Prepare::input` hashes the
generated wrapper as the page's cache fingerprint, so any value naming other pages ties
every page's identity to all of them: the section tree used to live there, and one
retitle was a cold rebuild of the whole site. It is now a generated file
(`.baudelaire/generated/sections.typ`) that templates import, so typst's own file
tracking scopes it to the templates that read it. `page.nav` (prev/next only) is the
line: it names two neighbours, not the site.
