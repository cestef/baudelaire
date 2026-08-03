// Sidebar helpers: highlight the link for the current page and drive the
// collapsible navigation on small screens. Pure progressive enhancement: the
// links work without any of this.

function normalize(pathname: string): string {
  return pathname.replace(/\/+$/, "") || "/";
}

export function markActiveNav(): void {
  const here = normalize(location.pathname);
  for (const link of document.querySelectorAll<HTMLAnchorElement>(
    ".sidebar a",
  )) {
    if (normalize(new URL(link.href).pathname) === here) {
      link.setAttribute("aria-current", "page");
    }
  }
}

// Collapsible subsections. The markup ships closed (and works with no JS); here
// we open the section holding the current page, mark its trail, and restore any
// the reader had expanded.
export function initNavSections(): void {
  const KEY = "nav-open";
  const sections = document.querySelectorAll<HTMLDetailsElement>(
    "details[data-nav-section]",
  );
  if (!sections.length) return;

  let opened: Set<string>;
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(KEY) || "[]");
    opened = new Set(Array.isArray(stored) ? stored.map(String) : []);
  } catch {
    opened = new Set();
  }

  const persist = () => {
    const open = [...sections]
      .filter((d) => d.open)
      .map((d) => d.dataset.navSection)
      .filter((id): id is string => id !== undefined);
    try {
      localStorage.setItem(KEY, JSON.stringify(open));
    } catch {
      /* private mode: toggles just won't persist */
    }
  };

  for (const details of sections) {
    const id = details.dataset.navSection;
    const active = details.querySelector('a[aria-current="page"]');
    if (active) {
      details.open = true;
      details.classList.add("is-active");
    } else if (id !== undefined && opened.has(id)) {
      details.open = true;
    }
    details.addEventListener("toggle", persist);
  }
}

// The sidebar is its own scroll box, and every click is a full page load, so
// without this it snaps back to the top on each navigation. The offset rides in
// sessionStorage: per tab, gone when the tab is, which is the lifetime of the
// reading session it belongs to.
export function keepNavScroll(): void {
  const KEY = "nav-scroll";
  const sidebar = document.getElementById("sidebar");
  if (!sidebar) return;

  const stored = () => {
    try {
      return Number(sessionStorage.getItem(KEY)) || 0;
    } catch {
      return 0;
    }
  };

  // Restore first, then check the current page's link is actually on screen: a
  // jump into a section the reader had scrolled away from would otherwise land
  // them on an offset that hides where they are. The correction is applied to
  // `scrollTop` by hand rather than with `scrollIntoView`, which would walk up
  // and scroll the article too.
  sidebar.scrollTop = stored();
  const active = sidebar.querySelector('a[aria-current="page"]');
  if (active) {
    const link = active.getBoundingClientRect();
    const box = sidebar.getBoundingClientRect();
    if (link.top < box.top || link.bottom > box.bottom) {
      sidebar.scrollTop += link.top - box.top - (box.height - link.height) / 2;
    }
  }

  // Written on the way out rather than on every scroll frame: one storage write
  // per navigation. `pagehide` covers a click, a reload and a back/forward.
  addEventListener("pagehide", () => {
    try {
      sessionStorage.setItem(KEY, String(sidebar.scrollTop));
    } catch {
      /* private mode: the sidebar just starts at the top */
    }
  });
}

export function initMobileNav(): void {
  const toggle = document.querySelector("[data-nav-toggle]");
  const sidebar = document.getElementById("sidebar");
  if (!toggle || !sidebar) return;

  toggle.addEventListener("click", () => {
    const open = sidebar.classList.toggle("is-open");
    toggle.setAttribute("aria-expanded", String(open));
  });
}
