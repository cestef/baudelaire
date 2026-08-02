#let frontmatter = (
  title: "SPA & single-file export",
  order: 2,
)
#import "/templates/theme.typ": callout

Three independent switches for how a browser gets from one page to the next.
Set one, all three, or none.

```kdl
navigation {
  spa {
    root "main"
    prefetch "hover"
  }
}
```

None of them replace the ordinary build. `dist` still holds every page as its
own document; these write artifacts beside it.

== Client-side navigation

`navigation { spa }` writes `dist/spa.js`. Load it once, from your template:

```typ
#import "@baudelaire/html:0.1.0": h

#h("script", type: "module", src: "/spa.js")
```

From then on a click on an internal link fetches the target page, swaps the
contents of `root`, and updates the URL through the History API. Everything
outside `root` survives the navigation: a sticky header keeps its scroll, a
sidebar keeps its open sections, an audio player keeps playing.

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`root`],
  [str],
  [`main`],
  [CSS selector for the element swapped on navigation.],

  [`prefetch`],
  [`none` / `hover` / `visible`],
  [`hover`],
  [When a link's target is fetched ahead of the click.],
)

`hover` warms on pointer-over or keyboard focus. `visible` warms as soon as the
link scrolls into view, which fetches far more than gets clicked. `none` never
warms.

#callout(kind: "note")[
  Every route is still a real document. A visitor without JavaScript, a crawler,
  and a deep link all get the page they always did, and a click the router can't
  serve is handed back to the browser.
]

=== Re-running your own code

A swap replaces markup, so anything you wired up on load has to run again. The
router announces every navigation on `document`:

```js
document.addEventListener("baudelaire:navigate", (event) => {
  highlight(document.querySelector("main"));
  console.log("now at", event.detail.path);
});
```

`baudelaire:navigating` fires just before the swap, `baudelaire:navigate` just
after, both with `detail.path`. An inline `script` inside the swapped content
runs again on every navigation. A `script src` runs once per document, as it
would if the browser had loaded each page itself, so a bundle never mounts its
widgets twice.

=== Mounting it yourself

Already bundling JavaScript? Import the runtime from the `baudelaire:spa`
#link("../lookup/js-modules.typ")[virtual module] instead of loading `spa.js`,
and mount it where you like:

```js
import { mountSpa } from "baudelaire:spa";

mountSpa({ select: "#content", prefetch: "visible" });
```

`select`, `prefetch` and `mode` override the configured defaults. The module
also exports `mountRouter` if you want to drive the core with your own loader.
It's served whether or not the `spa { }` block is set: importing it is itself
the opt-in, and the block only supplies the defaults `mountSpa()` starts from.

== One file, whole site

```kdl
html {
  embed #true
}

navigation {
  standalone {
    file "site.html"
    entry "/"
    router "hash"
  }
}
```

The build writes `dist/site.html`: the entry page's markup, every other page
stored as a route in a JSON island, and the router inlined as a classic script.
Open it and you get the entry page. Follow a link and it swaps in a route and
puts `#/blog/post/` in the address bar. No server anywhere.

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`file`],
  [path],
  [`site.html`],
  [Name of the exported file, written into `dist`.],

  [`entry`],
  [str],
  [site home],
  [Permalink of the page the file opens on.],

  [`router`],
  [`hash` / `history`],
  [`hash`],
  [How the URL addresses a route once the file is open.],
)

`html { embed #true }` is what makes the file self-contained: it inlines
stylesheets, scripts and images as `data:` URIs so nothing points at an
`/assets/` path the file can't reach. See
#link("../build/assets.typ")[the asset pipeline]. Without it the build warns,
and the export loses its styling the moment it leaves your machine.

Resources every page shares (the site stylesheet, the site bundle) are stated
once in the file's head rather than once per route, so the export grows with
your content and not with pages times assets. A stylesheet only one page links
stays with that page.

`hash` is the only mode that works from `file://`: it never asks the browser for
a path the filesystem would have to have, and a fragment that isn't a route
(`#installation`) is left alone, so heading links keep working. Use `history`
only when a host is configured to answer every path with this file.

#callout(kind: "warn")[
  The entry page's head becomes the whole file's head. Point `entry` at a page
  whose head is representative of the site, usually the home page.
]

#callout(kind: "note")[
  With both `spa` and `standalone` on, the exported file uses its own inlined
  router and ignores the `spa.js` your pages load. Only one router ever drives a
  document, and the export's wins.
]

== Browser-native prefetch

```kdl
navigation {
  speculation {
    prefetch "moderate"
    prerender "conservative"
  }
}
```

Adds a `speculationrules` script to each page's head, asking the browser itself
to fetch, or fully render, an internal link's target before it's clicked.
Nothing ships, nothing mounts, and a browser without the API ignores it.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Default], [Does]),
  [`prefetch`],
  [`moderate`],
  [How eagerly the browser fetches a linked page.],

  [`prerender`],
  [`none`],
  [How eagerly it renders one ahead of the click.],
)

Both take `none`, `conservative`, `moderate`, `eager` or `immediate`. `prefetch`
costs bytes. `prerender` costs a hidden page render and runs the target's
scripts, so it's off by default. The rules are scoped to your own base path, so
a site under `/docs` never speculates on a neighbor sharing the host.

Treat this as an alternative to `spa`, not a companion: with the SPA runtime
mounted, its own prefetch already warms the same links.

== What gets written

#table(
  columns: 2,
  align: (left, left),
  table.header([Artifact], [When]),
  [`dist/spa.js`], [`navigation { spa }` is set.],
  [`dist/site.html`], [`navigation { standalone }` is set (or whatever `file` names).],
  [`baudelaire:spa`], [Always, for bundlers.],
)
