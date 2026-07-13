// Light/dark theme, remembered in localStorage. When nothing is stored the
// page follows the OS `prefers-color-scheme` (handled in CSS), so there is no
// flash for the common case — the toggle only records an explicit override.

const KEY = "bd-theme";
const root = document.documentElement;

function prefersDark() {
  return matchMedia("(prefers-color-scheme: dark)").matches;
}

function isDark() {
  return root.dataset.theme ? root.dataset.theme === "dark" : prefersDark();
}

export function initTheme() {
  const stored = localStorage.getItem(KEY);
  if (stored) root.dataset.theme = stored;

  const button = document.querySelector("[data-theme-toggle]");
  if (!button) return;

  const sync = () => button.setAttribute("aria-pressed", String(isDark()));
  sync();

  button.addEventListener("click", () => {
    const next = isDark() ? "light" : "dark";
    root.dataset.theme = next;
    localStorage.setItem(KEY, next);
    sync();
  });
}
