// The dark-mode toggle. Nothing else on the page needs script: the colours are
// CSS custom properties, and the OS preference already drives them until a
// reader states one of their own here.
//
// Reading the stored choice is `boot` in `parts.typ`, which runs before the
// first paint; this module only writes it.

const KEY = "albatros-theme";
const root = document.documentElement;

const isDark = () =>
  root.dataset.theme
    ? root.dataset.theme === "dark"
    : matchMedia("(prefers-color-scheme: dark)").matches;

for (const button of document.querySelectorAll("[data-theme-toggle]")) {
  const sync = () => button.setAttribute("aria-pressed", String(isDark()));
  sync();
  button.addEventListener("click", () => {
    root.dataset.theme = isDark() ? "light" : "dark";
    localStorage.setItem(KEY, root.dataset.theme);
    sync();
  });
}
