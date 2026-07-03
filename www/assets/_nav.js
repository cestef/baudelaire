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

export function initMobileNav() {
  const toggle = document.querySelector("[data-nav-toggle]");
  const sidebar = document.getElementById("sidebar");
  if (!toggle || !sidebar) return;

  toggle.addEventListener("click", () => {
    const open = sidebar.classList.toggle("is-open");
    toggle.setAttribute("aria-expanded", String(open));
  });
}
