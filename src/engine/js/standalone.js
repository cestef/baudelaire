// The bundle adapter for the single-file export: every route is already in the
// document, as JSON in a `<script type="application/json">`, so a navigation is
// a lookup and never a request. Concatenated with router.js into the one inline
// classic script the exported file carries. `ROUTES_ID`, `MODE`, and `ENTRY`
// come from the generated prelude.

const ROUTES = JSON.parse(document.getElementById(ROUTES_ID).textContent);

// Routes are keyed by permalink, which under clean URLs ends in a slash. A
// hand-written link is the one place that slash goes missing, so try it before
// calling a route unknown.
const routeOf = (path) => ROUTES[path] || ROUTES[path + "/"];

mountRouter({
  load: (path) => {
    const route = routeOf(path);
    return route ? Promise.resolve(route) : Promise.reject(new Error("no such route"));
  },
  // Links to anything the bundle does not carry stay ordinary links.
  known: (path) => Boolean(routeOf(path)),
  mode: MODE,
  entry: ENTRY,
});
