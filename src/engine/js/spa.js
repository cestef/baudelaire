// The fetch adapter: turns the ordinary multi-file output into single-page
// navigation. Concatenated with router.js to form both the generated `spa.js`
// client and the `baudelaire:spa` virtual module.
//
// Nothing here changes what the site *is*: every route stays a real document a
// browser can load on its own, so a visitor without JavaScript, a crawler, and
// a deep link all get the same page they always did. `ROOT` and `PREFETCH` come
// from the generated prelude.

// Path -> pending parse. Memoized so a warmed link costs one fetch, not two,
// and a revisit costs none.
const pages = new Map();

function fetchRoute(path) {
  let pending = pages.get(path);
  if (pending) return pending;
  pending = fetch(path, { credentials: "same-origin" })
    .then((response) => {
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      return response.text();
    })
    .then((text) => {
      // The browser's own parser, over a document it would otherwise have
      // loaded itself: no markup is ever taken apart by hand.
      const doc = new DOMParser().parseFromString(text, "text/html");
      const source = ROOT ? doc.querySelector(ROOT) : doc.body;
      if (!source) throw new Error(`the response has no \`${ROOT}\``);
      return { title: doc.title, html: source.innerHTML };
    });
  // A failure must not be cached as the page's content forever: a reload of a
  // route that was briefly 503 should try again.
  pending.catch(() => pages.delete(path));
  pages.set(path, pending);
  return pending;
}

// Start intercepting navigation. Options override the configured defaults, so
// a bundling user can point the runtime at a different container.
function mountSpa(options = {}) {
  return mountRouter({
    load: fetchRoute,
    select: ROOT,
    mode: "history",
    prefetch: PREFETCH,
    ...options,
  });
}
