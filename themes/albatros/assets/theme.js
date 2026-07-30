// The dark-mode toggle. Nothing else on the page needs script: the colours are
// CSS custom properties, and the OS preference already drives them until a
// reader states one of their own here.

const KEY = "albatros-theme";
const root = document.documentElement;

const stored = localStorage.getItem(KEY);
if (stored === "light" || stored === "dark") {
  root.dataset.theme = stored;
}

const current = () =>
  root.dataset.theme ??
  (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");

for (const button of document.querySelectorAll("[data-theme-toggle]")) {
  button.addEventListener("click", () => {
    const next = current() === "dark" ? "light" : "dark";
    root.dataset.theme = next;
    localStorage.setItem(KEY, next);
  });
}
