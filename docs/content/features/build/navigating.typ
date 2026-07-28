#let frontmatter = (
  order: 14,
  title: "SPA and single-file export",
  tags: ("feature", "output", "javascript"),
)
#import "/templates/theme.typ": callout

Two outputs share one router. `spa` keeps the site exactly as it is and swaps
pages in place instead of reloading them. `standalone` folds the whole site into
one HTML file you can mail, hand over on a USB stick, or open from disk.

Neither replaces the ordinary build: `dist` still holds every page as its own
document, and these add artifacts beside it.

They are independent, and each is useful on its own. Turn on `spa` for a hosted
site that should navigate without reloads. Turn on `standalone` to hand the
whole site to someone as a file; the export carries its own copy of the router,
so it needs nothing else in the build. They share an implementation, not a
dependency: set either, both, or neither.

Both, along with browser-native `speculation` below, live in the top-level
`navigation` block: everything there answers the same question, how the browser
gets from one page to the next.

#callout(kind: "note")[
  With both on, the exported file uses its own inlined router and ignores the
  `spa.js` your pages load. Only one router ever drives a document, and the
  export's wins.
]

== Client-side navigation

```kdl
navigation {
  spa {
    root "main"        // the element swapped on navigation
    prefetch "hover"   // "none" | "hover" | "visible"
  }
}
```

The build writes `spa.js` at the root of `dist`. Load it once, from any template:

```html
<script type="module" src="/spa.js"></script>
```

From then on a click on an internal link fetches the target page, swaps the
contents of `root`, and updates the URL through the History API. Everything
outside `root` survives the navigation: a sticky header keeps its scroll, a
sidebar keeps its open sections, an audio player keeps playing.

#callout(kind: "note")[
  This is progressive enhancement, not a rewrite. Every route is still a real
  document, so a visitor without JavaScript, a crawler, and a deep link all get
  the same page they always did. If a click cannot be served (an unreachable
  page, a layout with no `root`), the router hands it back to the browser.
]

`prefetch` decides when a link's target is warmed: `hover` on pointer-over or
keyboard focus, `visible` as soon as it scrolls into view (which warms far more
than gets clicked), `none` never.

=== Re-running your own code

A swap replaces markup, so anything you wired up on load has to be wired up
again. The router announces every navigation on `document`:

```js
document.addEventListener("baudelaire:navigate", (event) => {
  highlight(document.querySelector("main"));
  console.log("now at", event.detail.path);
});
```

`baudelaire:navigating` fires just before the swap, `baudelaire:navigate` just
after. Inline `<script>` inside the swapped content runs again on every
navigation; a `<script src>` runs once per document, as it would if the browser
had loaded each page itself, so a bundle never mounts its widgets twice.

=== Bundling it yourself

If you already bundle JavaScript, import the runtime from the
#raw("baudelaire:spa") virtual module instead of loading `spa.js`, and mount it
where you like:

```js
import { mountSpa } from "baudelaire:spa";

mountSpa({ select: "#content", prefetch: "visible" });
```

The module is served whether or not the `navigation { spa }` block is set: importing it is
itself the opt-in, and the block only supplies the defaults `mountSpa()` starts
from.

== One file, whole site

```kdl
html { embed #true }

navigation {
  standalone {
    file "site.html"   // written to dist
    entry "/"          // the page the file opens on
    router "hash"      // "hash" | "history"
  }
}
```

The build writes `dist/site.html`: the entry page's markup, plus every other
page stored as a route in a JSON island, plus the router. Opening it shows the
entry page; following a link swaps in a route and puts `#/blog/post/` in the
address bar. No server is involved at any point.

`html { embed #true }` is what makes the file self-contained: it inlines
stylesheets, scripts, and images as `data:` URIs so nothing is left pointing at
a `/assets/` path the file cannot reach. Without it the build warns, and the
export will lose its styling the moment it leaves your machine.

Resources every page shares (the site stylesheet, the site bundle) are stated
once in the file's head rather than once per route, so the export grows with the
size of your content and not with pages times assets. A stylesheet only one page
links stays with that page.

=== Hash or history

`hash` is the default and the only mode that works from `file://`: it never asks
the browser for a path the filesystem would have to have. A fragment that is not
a route (`#installation`) is left alone, so heading links keep working.

Use `history` only when the file is served by a host configured to answer every
path with it.

#callout(kind: "warning")[
  The entry page's `<head>` becomes the file's `<head>`, and the whole export is
  one document. Give `entry` a page whose head is representative of the site,
  which the home page usually is.
]

== Browser-native prefetch, no runtime

```kdl
navigation {
  speculation {
    prefetch "moderate"      // none | conservative | moderate | eager | immediate
    prerender "conservative"
  }
}
```

Emits a `<script type="speculationrules">` asking the browser itself to fetch,
or fully render, an internal link's target before it is clicked. Nothing ships,
nothing mounts, and a browser without the API ignores it.

`prefetch` costs bytes; `prerender` costs a hidden page render and runs the
target's scripts, so it defaults to off. The rules are scoped to your own base
path, so a site under `/docs` never speculates on a neighbour sharing the host.

This is an alternative to `spa`, not a companion: with the SPA runtime mounted,
its own prefetch already warms the same links.

== What gets written

/ #raw("dist/spa.js"): the navigation client, when `navigation { spa }` is set.
/ #raw("dist/site.html"): the single-file export, when `navigation { standalone }`
  is set.
/ #raw("baudelaire:spa"): the virtual module, always available to bundlers.
