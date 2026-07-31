// Everything on a documentation page that only the browser can know: which
// sidebar link is the current page, what headings the compiled body ended up
// with, and which groups the reader collapsed.
//
// The search palette is not here: `generate { search { ui } }` emits a
// self-mounting client at /search.js, and this theme only restyles it.

const THEME_KEY = "phares-theme";
const GROUPS_KEY = "phares-collapsed";
const root = document.documentElement;

/* Colour scheme ----------------------------------------------------------- */

const stored = localStorage.getItem(THEME_KEY);
if (stored === "light" || stored === "dark") {
  root.dataset.theme = stored;
}

const scheme = () =>
  root.dataset.theme ??
  (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");

for (const button of document.querySelectorAll("[data-theme-toggle]")) {
  button.addEventListener("click", () => {
    const next = scheme() === "dark" ? "light" : "dark";
    root.dataset.theme = next;
    localStorage.setItem(THEME_KEY, next);
  });
}

/* Sidebar ----------------------------------------------------------------- */

const sidebar = document.getElementById("sidebar");
const here = location.pathname.replace(/\/index\.html$/, "/");

// Mark the current page, and open every group above it, so a deep page opens
// with its own place in the manual visible.
for (const link of sidebar?.querySelectorAll(".nav-link") ?? []) {
  if (new URL(link.href, location.origin).pathname !== here) continue;
  link.classList.add("active");
  link.setAttribute("aria-current", "page");
  for (let node = link.closest("details"); node; node = node.parentElement?.closest("details")) {
    node.open = true;
  }
  link.scrollIntoView({ block: "nearest" });
}

// A reader who collapses a group keeps it collapsed across pages. Stored as a
// list of ids rather than per-group keys, so clearing it is one removal.
const collapsed = new Set(JSON.parse(localStorage.getItem(GROUPS_KEY) ?? "[]"));
for (const group of sidebar?.querySelectorAll("[data-nav-group]") ?? []) {
  const id = group.dataset.navGroup;
  // Never re-collapse the group holding the current page: it was just opened
  // above, and hiding the reader's own position is the one unhelpful outcome.
  if (collapsed.has(id) && !group.querySelector(".nav-link.active")) group.open = false;
  group.addEventListener("toggle", () => {
    group.open ? collapsed.delete(id) : collapsed.add(id);
    localStorage.setItem(GROUPS_KEY, JSON.stringify([...collapsed]));
  });
}

// The narrow-screen drawer.
for (const button of document.querySelectorAll("[data-nav-toggle]")) {
  button.addEventListener("click", () => {
    const open = root.dataset.nav !== "open";
    root.dataset.nav = open ? "open" : "closed";
    button.setAttribute("aria-expanded", String(open));
  });
}

/* On-page contents -------------------------------------------------------- */

// Built from the rendered article rather than from the layout: the headings are
// in the page's own body, which the template never sees. `html { anchors }`
// gives each one an id, so a heading without one is a heading nothing can link.
const toc = document.querySelector("[data-toc]");
const headings = [...document.querySelectorAll(".content h2[id], .content h3[id]")];

if (toc && headings.length > 1) {
  const list = toc.querySelector(".toc-list");
  for (const heading of headings) {
    const item = document.createElement("li");
    item.className = `toc-item toc-${heading.tagName.toLowerCase()}`;
    const link = document.createElement("a");
    link.href = `#${heading.id}`;
    // The heading's own text, minus the anchor glyph a stylesheet may add.
    link.textContent = heading.textContent.trim();
    item.append(link);
    list.append(item);
  }
  toc.hidden = false;

  // Highlight the section being read. `rootMargin` pins the trigger line near
  // the top of the viewport, so the entry lights up as its heading arrives
  // rather than when the whole section is on screen.
  const links = new Map(headings.map((h, i) => [h.id, list.children[i].firstChild]));
  const spy = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        for (const link of links.values()) link.classList.remove("active");
        links.get(entry.target.id)?.classList.add("active");
      }
    },
    { rootMargin: "-72px 0px -70% 0px" },
  );
  for (const heading of headings) spy.observe(heading);
}
