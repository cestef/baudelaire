// The shared client-side router: link interception, history, and container
// swapping. It never decides *what* a route contains: an adapter supplies
// `load(path)`, so the same core drives the SPA runtime (which fetches the real
// page) and the single-file export (which reads a route out of the inlined
// bundle).
//
// No top-level `import`/`export` here. This file is concatenated into a classic
// inline `<script>` for the standalone export, which has to run from `file://`,
// where module scripts are blocked outright; the `baudelaire:spa` module build
// appends its own export list instead.

// Dispatched on `document` around every swap, so a host page can re-run
// whatever it wires up on load (syntax highlighting, a table of contents).
const NAVIGATING = "baudelaire:navigating";
const NAVIGATE = "baudelaire:navigate";

// Where the mounted router parks itself, so a document can only ever have one.
// The single-file export inlines its own; a site that also loads `spa.js` (and
// inlines it into every page) would otherwise mount a second one inside the
// export, with two listeners fighting over the same clicks.
const MOUNTED = "__baudelaireRouter";

// Same-site check that also holds on `file://`, where every document's origin
// is the opaque "null" and comparing origins would reject every internal link.
const sameSite = (url) => url.protocol === location.protocol && url.host === location.host;

// Whether an anchor points somewhere this router could serve: a same-site link
// that is not asking for a new browsing context or a download.
const routable = (a) =>
  Boolean(a) &&
  Boolean(a.href) &&
  !a.target &&
  !a.hasAttribute("download") &&
  (a.getAttribute("rel") || "").split(/\s+/).indexOf("external") < 0 &&
  sameSite(new URL(a.href));

// Whether a click means "follow this link here", as opposed to the modified
// clicks the browser owns: new tab, new window, download, save.
const plain = (event) =>
  !event.defaultPrevented &&
  event.button === 0 &&
  !(event.metaKey || event.ctrlKey || event.shiftKey || event.altKey);

// The anchor an event happened inside, if any.
const anchorOf = (event) => (event.target.closest ? event.target.closest("a[href]") : null);

// Mount the router. `load(path)` resolves to `{ title, html }`; `known(path)`,
// when given, reports whether a path is routable at all, so a link the router
// cannot serve is left to the browser instead of being intercepted and then
// failed. `select` names the swapped container (null swaps the whole body),
// `mode` is "hash" or "history", and `entry` is the route already rendered.
function mountRouter({
  load,
  known = null,
  select = null,
  mode = "history",
  prefetch = "none",
  entry = null,
}) {
  // First one wins: the router already driving this document keeps driving it.
  if (window[MOUNTED]) return window[MOUNTED];

  const host = () => (select ? document.querySelector(select) : document.body);
  const serves = (path) => !known || known(path);
  let current = entry || location.pathname;

  // The route the address bar currently names, or null when it names none.
  const routed = () => {
    if (mode !== "hash") return location.pathname;
    // A fragment that does not start with `/` is an in-page anchor, not a
    // route: leaving it to the browser is how heading links keep working
    // alongside hash routing.
    const fragment = location.hash.slice(1);
    return fragment.startsWith("/") ? fragment : null;
  };

  // Nodes inserted through `innerHTML` never execute, so a page-local script
  // would silently die after the first navigation. Re-creating them restores
  // the ordinary load behaviour.
  //
  // Except for external ones, which run once per document and not once per
  // navigation: re-running the site's bundle on every swap would mount whatever
  // it mounts a second and a third time.
  const ran = new Set();
  const activate = (node) => {
    for (const stale of node.querySelectorAll("script")) {
      const src = stale.getAttribute("src");
      if (src && ran.has(src)) {
        stale.remove();
        continue;
      }
      if (src) ran.add(src);
      const script = document.createElement("script");
      for (const { name, value } of stale.attributes) script.setAttribute(name, value);
      script.textContent = stale.textContent;
      stale.replaceWith(script);
    }
  };

  const show = (node, path, route, hash) => {
    document.dispatchEvent(new CustomEvent(NAVIGATING, { detail: { path } }));
    if (route.title) document.title = route.title;
    node.innerHTML = route.html;
    activate(node);
    current = path;
    const anchor = hash && document.getElementById(hash.slice(1));
    if (anchor) anchor.scrollIntoView();
    else window.scrollTo(0, 0);
    // Move focus into the new content: without this a keyboard user stays
    // parked on the link they just followed and a screen reader announces
    // nothing at all.
    node.setAttribute("tabindex", "-1");
    node.focus({ preventScroll: true });
    document.dispatchEvent(new CustomEvent(NAVIGATE, { detail: { path } }));
  };

  const go = async (path, hash) => {
    const node = host();
    let route;
    try {
      if (!node) throw new Error(`this page has no \`${select}\` to swap`);
      route = await load(path);
    } catch (error) {
      // A route neither the bundle nor the server has, or a page laid out
      // differently enough to have no container, is not ours to render: hand it
      // back to the browser rather than leaving a dead link behind.
      console.warn(`baudelaire: ${path} could not be loaded:`, error);
      location.assign(path + hash);
      return;
    }
    show(node, path, route, hash);
  };

  const navigate = (path, hash) => {
    if (path === current && !hash) return;
    if (mode === "hash") {
      // `location.hash` rather than `pushState`: a `file://` document has an
      // opaque origin and browsers reject `pushState` there outright. The
      // hashchange this fires is what performs the swap.
      location.hash = path + hash;
      return;
    }
    history.pushState({ path }, "", path + hash);
    go(path, hash);
  };

  document.addEventListener("click", (event) => {
    const a = anchorOf(event);
    if (!plain(event) || !routable(a)) return;
    const url = new URL(a.href);
    // A bare `#section` on the current page: leave the scroll to the browser.
    if (url.pathname === location.pathname && url.hash && mode === "history") return;
    if (!serves(url.pathname)) return;
    event.preventDefault();
    navigate(url.pathname, url.hash);
  });

  if (mode === "hash") {
    window.addEventListener("hashchange", () => {
      const path = routed();
      if (path && path !== current && serves(path)) go(path, "");
    });
  } else {
    window.addEventListener("popstate", () => {
      const path = location.pathname;
      if (path !== current) go(path, location.hash);
    });
  }

  if (prefetch !== "none") warm(load, serves, prefetch);

  // A route deep-linked before any click (a shared `#/blog/post/`, a reload)
  // has to replace the entry the shell was built around.
  const initial = routed();
  if (initial && initial !== current && serves(initial)) go(initial, "");

  window[MOUNTED] = { navigate };
  return window[MOUNTED];
}

// Warm a link's target before it is clicked. `load` is expected to memoize, so
// a warmed path costs nothing again when it is actually navigated to.
function warm(load, serves, prefetch) {
  const seen = new Set();
  const pull = (a) => {
    if (!routable(a)) return;
    const { pathname } = new URL(a.href);
    if (seen.has(pathname) || !serves(pathname)) return;
    seen.add(pathname);
    Promise.resolve(load(pathname)).catch(() => {});
  };

  if (prefetch === "visible") {
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        observer.unobserve(entry.target);
        pull(entry.target);
      }
    });
    // Re-run after every swap: the links that just arrived are the ones worth
    // watching, and the ones observed before are gone.
    const observe = () => document.querySelectorAll("a[href]").forEach((a) => observer.observe(a));
    observe();
    document.addEventListener(NAVIGATE, observe);
    return;
  }

  // "hover": pointer-over and keyboard focus, the two moments intent shows.
  for (const event of ["pointerover", "focusin"]) {
    document.addEventListener(event, (e) => pull(anchorOf(e)));
  }
}
