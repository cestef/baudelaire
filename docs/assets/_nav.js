// Sidebar helpers: highlight the link for the current page and drive the
// collapsible navigation on small screens. Pure progressive enhancement — the
// links work without any of this.

function normalize(pathname) {
  return pathname.replace(/\/+$/, "") || "/";
}

export function markActiveNav() {
  const here = normalize(location.pathname);
  for (const link of document.querySelectorAll(".sidebar a")) {
    if (normalize(new URL(link.href).pathname) === here) {
      link.setAttribute("aria-current", "page");
    }
  }
}

// Collapsible subsections. The markup ships closed (and works with no JS); here
// we open the section holding the current page, mark its trail, and restore any
// the reader had expanded.
export function initNavSections() {
  const KEY = "nav-open";
  const sections = document.querySelectorAll("details[data-nav-section]");
  if (!sections.length) return;

  let opened;
  try {
    opened = new Set(JSON.parse(localStorage.getItem(KEY) || "[]"));
  } catch {
    opened = new Set();
  }

  const persist = () => {
    const open = [...sections]
      .filter((d) => d.open)
      .map((d) => d.dataset.navSection);
    try {
      localStorage.setItem(KEY, JSON.stringify(open));
    } catch {
      /* private mode — toggles just won't persist */
    }
  };

  for (const details of sections) {
    const id = details.dataset.navSection;
    const active = details.querySelector('a[aria-current="page"]');
    if (active) {
      details.open = true;
      details.classList.add("is-active");
    } else if (opened.has(id)) {
      details.open = true;
    }
    details.addEventListener("toggle", persist);
  }
}

export function initMobileNav() {
  const toggle = document.querySelector("[data-nav-toggle]");
  const sidebar = document.getElementById("sidebar");
  if (!toggle || !sidebar) return;

  toggle.addEventListener("click", () => {
    const open = sidebar.classList.toggle("is-open");
    toggle.setAttribute("aria-expanded", String(open));
  });
}
